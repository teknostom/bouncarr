use crate::AppState;
use crate::auth::jwt::TokenType;
use crate::error::{AppError, Result};
use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tower_cookies::{Cookie, Cookies};

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Serialize)]
pub struct LoginResponse {
    pub success: bool,
    pub username: String,
    pub is_admin: bool,
}

pub async fn login(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
    Json(req): Json<LoginRequest>,
) -> Result<Json<LoginResponse>> {
    // Authenticate with Jellyfin
    let (user_info, _jellyfin_token) = state
        .jellyfin_client
        .authenticate(&req.username, &req.password)
        .await?;

    // Check if user is an administrator
    if !user_info.is_administrator {
        return Err(AppError::Forbidden);
    }

    // Create JWT tokens
    let access_token = state.jwt_manager.create_access_token(&user_info)?;
    let refresh_token = state.jwt_manager.create_refresh_token(&user_info)?;

    // Set cookies
    let mut access_cookie = Cookie::new(state.config.security.cookie_name.clone(), access_token);
    access_cookie.set_http_only(true);
    access_cookie.set_secure(state.config.security.secure_cookies);
    access_cookie.set_path("/");
    cookies.add(access_cookie);

    let mut refresh_cookie = Cookie::new(
        state.config.security.refresh_cookie_name.clone(),
        refresh_token,
    );
    refresh_cookie.set_http_only(true);
    refresh_cookie.set_secure(state.config.security.secure_cookies);
    refresh_cookie.set_path("/");
    cookies.add(refresh_cookie);

    Ok(Json(LoginResponse {
        success: true,
        username: user_info.username,
        is_admin: user_info.is_administrator,
    }))
}

pub async fn refresh(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
) -> Result<Json<LoginResponse>> {
    // Get refresh token from cookie
    let refresh_token = cookies
        .get(&state.config.security.refresh_cookie_name)
        .ok_or(AppError::Unauthorized)?
        .value()
        .to_string();

    // Validate refresh token
    let claims = state
        .jwt_manager
        .validate_token(&refresh_token, TokenType::Refresh)?;

    // Fetch fresh user data from Jellyfin
    let user_info = state.jellyfin_client.get_user(&claims.sub).await?;

    // Check if still an administrator
    if !user_info.is_administrator {
        return Err(AppError::Forbidden);
    }

    // Create new access token
    let access_token = state.jwt_manager.create_access_token(&user_info)?;

    // Set new access token cookie
    let mut access_cookie = Cookie::new(state.config.security.cookie_name.clone(), access_token);
    access_cookie.set_http_only(true);
    access_cookie.set_secure(state.config.security.secure_cookies);
    access_cookie.set_path("/");
    cookies.add(access_cookie);

    Ok(Json(LoginResponse {
        success: true,
        username: user_info.username,
        is_admin: user_info.is_administrator,
    }))
}

pub async fn logout(
    State(state): State<Arc<AppState>>,
    cookies: Cookies,
) -> Result<Json<serde_json::Value>> {
    // Remove cookies
    cookies.remove(Cookie::from(state.config.security.cookie_name.clone()));
    cookies.remove(Cookie::from(
        state.config.security.refresh_cookie_name.clone(),
    ));

    Ok(Json(serde_json::json!({ "success": true })))
}
