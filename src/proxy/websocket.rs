use crate::error::{AppError, Result};
use axum::{
    body::Body,
    extract::{FromRequestParts, WebSocketUpgrade, ws::WebSocket},
    http::Request,
    response::Response,
};
use futures_util::{SinkExt, StreamExt};
use tokio_tungstenite::{connect_async, tungstenite::protocol::Message as TungsteniteMessage};

pub async fn proxy_websocket_connection(
    req: Request<Body>,
    target_url: String,
) -> Result<Response> {
    // Extract WebSocketUpgrade from the request
    let (mut parts, _body) = req.into_parts();
    let ws = match WebSocketUpgrade::from_request_parts(&mut parts, &()).await {
        Ok(ws) => ws,
        Err(e) => {
            tracing::error!("Failed to extract WebSocket upgrade: {}", e);
            return Err(AppError::ProxyError(format!(
                "Failed to upgrade to WebSocket: {}",
                e
            )));
        }
    };

    Ok(ws.on_upgrade(move |socket| handle_websocket_proxy(socket, target_url)))
}

pub async fn handle_websocket_proxy(mut client_socket: WebSocket, target_url: String) {
    use axum::extract::ws::Message;

    tracing::debug!("Connecting to upstream WebSocket");

    // Connect to the upstream WebSocket server (URL should already be ws://)
    let upstream_result = connect_async(&target_url).await;

    let (upstream_ws, _response) = match upstream_result {
        Ok(conn) => {
            tracing::debug!("WebSocket connection established");
            conn
        }
        Err(e) => {
            tracing::error!("Failed to connect to upstream WebSocket: {}", e);
            // Send close frame with error to client
            let error_message = format!("Failed to connect to upstream: {}", e);
            let _ = client_socket
                .send(Message::Close(Some(axum::extract::ws::CloseFrame {
                    code: 1011, // Internal server error
                    reason: error_message.into(),
                })))
                .await;
            return;
        }
    };

    #[cfg(debug_assertions)]
    tracing::debug!("Upstream WebSocket response: {:?}", _response);

    // Split both WebSocket connections
    let (mut client_sink, mut client_stream) = client_socket.split();
    let (mut upstream_sink, mut upstream_stream) = upstream_ws.split();

    // Create two tasks to forward messages in both directions
    let client_to_upstream = async move {
        while let Some(msg) = client_stream.next().await {
            let result = match msg {
                Ok(Message::Text(text)) => upstream_sink.send(TungsteniteMessage::Text(text)).await,
                Ok(Message::Binary(data)) => {
                    upstream_sink
                        .send(TungsteniteMessage::Binary(data.to_vec()))
                        .await
                }
                Ok(Message::Ping(data)) => {
                    upstream_sink
                        .send(TungsteniteMessage::Ping(data.to_vec()))
                        .await
                }
                Ok(Message::Pong(data)) => {
                    upstream_sink
                        .send(TungsteniteMessage::Pong(data.to_vec()))
                        .await
                }
                Ok(Message::Close(_)) => {
                    let _ = upstream_sink.send(TungsteniteMessage::Close(None)).await;
                    break;
                }
                Err(e) => {
                    tracing::debug!("Client WebSocket error: {}", e);
                    break;
                }
            };

            if result.is_err() {
                tracing::debug!("Failed to send message to upstream, closing connection");
                break;
            }
        }
    };

    let upstream_to_client = async move {
        while let Some(msg) = upstream_stream.next().await {
            let result = match msg {
                Ok(TungsteniteMessage::Text(text)) => client_sink.send(Message::Text(text)).await,
                Ok(TungsteniteMessage::Binary(data)) => {
                    client_sink.send(Message::Binary(data)).await
                }
                Ok(TungsteniteMessage::Ping(data)) => client_sink.send(Message::Ping(data)).await,
                Ok(TungsteniteMessage::Pong(data)) => client_sink.send(Message::Pong(data)).await,
                Ok(TungsteniteMessage::Close(_)) => {
                    let _ = client_sink.send(Message::Close(None)).await;
                    break;
                }
                Err(e) => {
                    tracing::debug!("Upstream WebSocket error: {}", e);
                    break;
                }
                _ => continue,
            };

            if result.is_err() {
                tracing::debug!("Failed to send message to client, closing connection");
                break;
            }
        }
    };

    // Run both forwarding tasks concurrently
    tokio::select! {
        _ = client_to_upstream => {},
        _ = upstream_to_client => {},
    }
}
