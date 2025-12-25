use crate::AppState;
use crate::auth::jwt::TokenType;
use crate::error::{AppError, Result};
use crate::jellyfin::types::UserInfo;
use axum::{
    body::Body,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use std::sync::Arc;
use tower_cookies::Cookies;

pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
    mut req: Request<Body>,
    next: Next,
) -> Response {
    tracing::debug!("Auth middleware: checking authentication for {}", req.uri());

    // Check if this is a browser request (wants HTML)
    let is_browser = req
        .headers()
        .get(header::ACCEPT)
        .and_then(|v| v.to_str().ok())
        .map(|v| v.contains("text/html"))
        .unwrap_or(false);

    // Extract token from cookie or Authorization header
    let token = match extract_token(&req, cookies, &state.config.security.cookie_name) {
        Ok(t) => t,
        Err(e) => {
            tracing::debug!("No valid token found for {}", req.uri().path());
            if is_browser {
                let redirect_url = format!(
                    "/bouncarr/login?redirect={}",
                    urlencoding::encode(req.uri().path())
                );
                return Redirect::to(&redirect_url).into_response();
            }
            return e.into_response();
        }
    };

    // Validate the access token
    let claims = match state.jwt_manager.validate_token(&token, TokenType::Access) {
        Ok(c) => c,
        Err(e) => {
            // Only log validation failures at debug level to reduce noise
            // (common after server restart with old cookies)
            tracing::debug!("Token validation failed for {}: {:?}", req.uri().path(), e);
            if is_browser {
                let redirect_url = format!(
                    "/bouncarr/login?redirect={}",
                    urlencoding::encode(req.uri().path())
                );
                return Redirect::to(&redirect_url).into_response();
            }
            return AppError::Unauthorized.into_response();
        }
    };

    // Check if user is an administrator
    if !claims.is_admin {
        tracing::warn!("User {} is not an admin", claims.username);
        if is_browser {
            return (
                StatusCode::FORBIDDEN,
                "Admin access required. Please contact your administrator.",
            )
                .into_response();
        }
        return AppError::Forbidden.into_response();
    }

    tracing::debug!("Auth successful for user: {}", claims.username);

    // Create UserInfo from claims and attach to request
    let user_info = UserInfo {
        user_id: claims.sub,
        username: claims.username,
        is_administrator: claims.is_admin,
    };

    req.extensions_mut().insert(user_info);

    next.run(req).await
}

fn extract_token(req: &Request<Body>, cookies: Cookies, cookie_name: &str) -> Result<String> {
    // Try to get token from cookie first
    if let Some(cookie) = cookies.get(cookie_name) {
        // Note: Logging cookie NAME only (not the value/token itself) - safe for production
        tracing::debug!("Found token in cookie: {}", cookie_name);
        return Ok(cookie.value().to_string());
    }

    // Try to get token from Authorization header
    // Using let-chain syntax for clean sequential error handling
    if let Some(auth_header) = req.headers().get(header::AUTHORIZATION)
        && let Ok(auth_str) = auth_header.to_str()
        && let Some(token) = auth_str.strip_prefix("Bearer ")
    {
        // Note: Not logging the actual token value - safe for production
        tracing::debug!("Found token in Authorization header");
        return Ok(token.to_string());
    }

    tracing::debug!("No token found in cookies or headers");
    Err(AppError::Unauthorized)
}
