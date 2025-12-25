use crate::AppState;
use crate::error::{AppError, Result};
use axum::{body::Body, extract::State, http::Request, response::Response};
use http_body_util::BodyExt;
use std::sync::Arc;

pub async fn proxy_handler(
    State(state): State<Arc<AppState>>,
    req: Request<Body>,
) -> Result<Response> {
    // Extract app name from the first path segment
    let path = req.uri().path();
    let app_name = path
        .trim_start_matches('/')
        .split('/')
        .next()
        .unwrap_or("")
        .to_string();

    // Check if this is a WebSocket upgrade request by looking at headers
    let is_websocket = req
        .headers()
        .get("upgrade")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.eq_ignore_ascii_case("websocket"))
        .unwrap_or(false);

    if is_websocket {
        tracing::debug!("WebSocket upgrade request detected for {}", path);
        return handle_websocket_upgrade_raw(state, app_name, req).await;
    }
    // Find the arr app configuration
    let arr_app = state
        .config
        .arr_apps
        .iter()
        .find(|app| app.name == app_name)
        .ok_or_else(|| {
            let available_apps: Vec<_> = state.config.arr_apps.iter().map(|a| &a.name).collect();
            tracing::warn!(
                "Request for unknown app '{}'. Available apps: {:?}. \
                Make sure to configure URL Base in your *arr app settings.",
                app_name,
                available_apps
            );
            AppError::AppNotFound(format!(
                "App '{}' not found. Available apps: {:?}. \
                Hint: Configure URL Base to '/{app_name}' in your *arr app settings.",
                app_name, available_apps
            ))
        })?;

    // Build target URL
    let path_and_query = req
        .uri()
        .path_and_query()
        .map(|pq| {
            // Remove the /app_name prefix from the path
            let path = pq.path();
            let prefix = format!("/{}", app_name);
            let new_path = path.strip_prefix(&prefix).unwrap_or(path);

            // If path is empty after stripping, default to "/"
            let new_path = if new_path.is_empty() { "/" } else { new_path };

            if let Some(query) = pq.query() {
                format!("{}?{}", new_path, query)
            } else {
                new_path.to_string()
            }
        })
        .unwrap_or_else(|| "/".to_string());

    let target_url = format!("{}{}", arr_app.url, path_and_query);

    tracing::debug!("Proxying {} to {}", req.method(), target_url);

    // Forward the request
    forward_request(&state, target_url, req).await
}

async fn forward_request(
    state: &AppState,
    target_url: String,
    req: Request<Body>,
) -> Result<Response> {
    let method = req.method().clone();
    let headers = req.headers().clone();

    // Collect the body
    let body_bytes = req
        .into_body()
        .collect()
        .await
        .map_err(|e| {
            tracing::error!("Failed to read request body: {}", e);
            AppError::ProxyError(format!("Failed to read request body: {}", e))
        })?
        .to_bytes();

    // Build the proxied request
    let mut proxy_req = state
        .http_client
        .request(method.clone(), &target_url)
        .body(body_bytes.to_vec());

    // Forward relevant headers (skip host, connection, etc.)
    for (name, value) in headers.iter() {
        let name_str = name.as_str().to_lowercase();
        if !should_skip_header(&name_str) {
            proxy_req = proxy_req.header(name, value);
        }
    }

    // Send the request
    let response = proxy_req.send().await.map_err(|e| {
        tracing::error!("Failed to proxy {} to {}: {}", method, target_url, e);
        AppError::ProxyError(format!("Failed to proxy request to {}: {}", target_url, e))
    })?;

    let status = response.status();
    tracing::debug!("Upstream response status: {}", status);

    // Convert reqwest::Response to axum::Response
    let mut builder = Response::builder().status(status);

    // Copy headers from the response
    for (name, value) in response.headers().iter() {
        let name_str = name.as_str().to_lowercase();
        if !should_skip_header(&name_str) {
            builder = builder.header(name, value);
        }
    }

    let body_bytes = response.bytes().await.map_err(|e| {
        tracing::error!("Failed to read response body: {}", e);
        AppError::ProxyError(format!("Failed to read response body: {}", e))
    })?;

    builder.body(Body::from(body_bytes)).map_err(|e| {
        tracing::error!("Failed to build response: {}", e);
        AppError::ProxyError(format!("Failed to build response: {}", e))
    })
}

fn should_skip_header(name: &str) -> bool {
    matches!(
        name,
        "host" | "connection" | "transfer-encoding" | "content-length"
    )
}

async fn handle_websocket_upgrade_raw(
    state: Arc<AppState>,
    app_name: String,
    req: Request<Body>,
) -> Result<Response> {
    use crate::proxy::websocket::proxy_websocket_connection;

    // Find the arr app configuration
    let arr_app = state
        .config
        .arr_apps
        .iter()
        .find(|app| app.name == app_name)
        .ok_or_else(|| {
            let available_apps: Vec<_> = state.config.arr_apps.iter().map(|a| &a.name).collect();
            AppError::AppNotFound(format!(
                "App '{}' not found for WebSocket connection. Available apps: {:?}",
                app_name, available_apps
            ))
        })?;

    // Build the WebSocket URL
    // Keep the full path including the app name prefix, since the *arr app
    // is configured with URL Base matching our prefix
    let path = req.uri().path();
    let query = req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();

    // Convert HTTP URL to WebSocket URL
    let target_ws_url = arr_app
        .url
        .replace("http://", "ws://")
        .replace("https://", "wss://");
    let full_ws_url = format!("{}{}{}", target_ws_url, path, query);

    tracing::info!("Proxying WebSocket connection to: {}", full_ws_url);

    proxy_websocket_connection(req, full_ws_url).await
}
