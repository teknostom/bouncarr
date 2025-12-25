use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::json;

#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("Configuration error: {0}")]
    Config(#[from] config::ConfigError),

    #[error("Authentication failed: {0}")]
    AuthenticationFailed(String),

    #[error("Unauthorized")]
    Unauthorized,

    #[error("Forbidden: Admin access required")]
    Forbidden,

    #[error("JWT error: {0}")]
    JwtError(#[from] jsonwebtoken::errors::Error),

    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Invalid token")]
    InvalidToken,

    #[error("Proxy error: {0}")]
    ProxyError(String),

    #[error("App not found: {0}")]
    AppNotFound(String),

    #[error("Internal server error: {0}")]
    Internal(#[from] anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, message) = match self {
            AppError::AuthenticationFailed(msg) => (StatusCode::UNAUTHORIZED, msg),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            AppError::Forbidden => (StatusCode::FORBIDDEN, "Admin access required".to_string()),
            AppError::InvalidToken => (StatusCode::UNAUTHORIZED, "Invalid token".to_string()),
            AppError::JwtError(e) => (StatusCode::UNAUTHORIZED, e.to_string()),
            AppError::ProxyError(msg) => (StatusCode::BAD_GATEWAY, msg),
            AppError::AppNotFound(msg) => (StatusCode::NOT_FOUND, msg),
            AppError::Config(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            AppError::RequestFailed(e) => (StatusCode::BAD_GATEWAY, e.to_string()),
            AppError::Internal(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };

        let body = Json(json!({
            "error": message,
        }));

        (status, body).into_response()
    }
}

pub type Result<T> = std::result::Result<T, AppError>;
