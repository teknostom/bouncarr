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

pub async fn handle_websocket_proxy(client_socket: WebSocket, target_url: String) {
    use axum::extract::ws::Message;

    tracing::info!(
        "Attempting to connect to upstream WebSocket: {}",
        target_url
    );

    // Connect to the upstream WebSocket server (URL should already be ws://)
    let upstream_result = connect_async(&target_url).await;

    let (upstream_ws, response) = match upstream_result {
        Ok(conn) => {
            tracing::info!("Successfully connected to upstream WebSocket");
            conn
        }
        Err(e) => {
            tracing::error!(
                "Failed to connect to upstream WebSocket '{}': {}",
                target_url,
                e
            );
            return;
        }
    };

    tracing::debug!("Upstream WebSocket response: {:?}", response);

    // Split both WebSocket connections
    let (mut client_sink, mut client_stream) = client_socket.split();
    let (mut upstream_sink, mut upstream_stream) = upstream_ws.split();

    // Create two tasks to forward messages in both directions
    let client_to_upstream = async move {
        while let Some(msg) = client_stream.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    if upstream_sink
                        .send(TungsteniteMessage::Text(text))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(Message::Binary(data)) => {
                    if upstream_sink
                        .send(TungsteniteMessage::Binary(data.to_vec()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(Message::Ping(data)) => {
                    if upstream_sink
                        .send(TungsteniteMessage::Ping(data.to_vec()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(Message::Pong(data)) => {
                    if upstream_sink
                        .send(TungsteniteMessage::Pong(data.to_vec()))
                        .await
                        .is_err()
                    {
                        break;
                    }
                }
                Ok(Message::Close(_)) => {
                    let _ = upstream_sink.send(TungsteniteMessage::Close(None)).await;
                    break;
                }
                Err(_) => break,
            }
        }
    };

    let upstream_to_client = async move {
        while let Some(msg) = upstream_stream.next().await {
            match msg {
                Ok(TungsteniteMessage::Text(text)) => {
                    if client_sink.send(Message::Text(text)).await.is_err() {
                        break;
                    }
                }
                Ok(TungsteniteMessage::Binary(data)) => {
                    if client_sink.send(Message::Binary(data)).await.is_err() {
                        break;
                    }
                }
                Ok(TungsteniteMessage::Ping(data)) => {
                    if client_sink.send(Message::Ping(data)).await.is_err() {
                        break;
                    }
                }
                Ok(TungsteniteMessage::Pong(data)) => {
                    if client_sink.send(Message::Pong(data)).await.is_err() {
                        break;
                    }
                }
                Ok(TungsteniteMessage::Close(_)) => {
                    let _ = client_sink.send(Message::Close(None)).await;
                    break;
                }
                Err(_) => break,
                _ => {}
            }
        }
    };

    // Run both forwarding tasks concurrently
    tokio::select! {
        _ = client_to_upstream => {},
        _ = upstream_to_client => {},
    }
}
