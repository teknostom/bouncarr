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
    // Validate input
    validate_login_request(&req)?;

    // Authenticate with Jellyfin
    let (user_info, _jellyfin_token) = match state
        .jellyfin_client
        .authenticate(&req.username, &req.password)
        .await
    {
        Ok(result) => result,
        Err(e) => {
            tracing::warn!("Failed login attempt for user '{}': {}", req.username, e);
            return Err(e);
        }
    };

    // Check if user is an administrator
    if !user_info.is_administrator {
        tracing::warn!("Non-admin user '{}' attempted to login", user_info.username);
        return Err(AppError::Forbidden);
    }

    tracing::info!("User '{}' logged in successfully", user_info.username);

    // Create JWT tokens
    let access_token = state.jwt_manager.create_access_token(&user_info)?;
    let refresh_token = state.jwt_manager.create_refresh_token(&user_info)?;

    // Set cookies
    // Note: Cookie::new requires ownership, so cloning cookie names is necessary
    let mut access_cookie = Cookie::new(state.config.security.cookie_name.clone(), access_token);
    access_cookie.set_http_only(true);
    access_cookie.set_secure(state.config.security.secure_cookies);
    access_cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);
    access_cookie.set_path("/");
    // Set max age to end of day to match JWT expiration
    let now = chrono::Utc::now();
    let end_of_day = now
        .date_naive()
        .and_hms_opt(23, 59, 59)
        .ok_or_else(|| {
            crate::error::AppError::Internal(anyhow::anyhow!(
                "Failed to create end of day timestamp"
            ))
        })?
        .and_utc();
    let seconds_until_eod = (end_of_day - now).num_seconds();
    access_cookie.set_max_age(tower_cookies::cookie::time::Duration::seconds(
        seconds_until_eod,
    ));
    cookies.add(access_cookie);

    let mut refresh_cookie = Cookie::new(
        state.config.security.refresh_cookie_name.clone(),
        refresh_token,
    );
    refresh_cookie.set_http_only(true);
    refresh_cookie.set_secure(state.config.security.secure_cookies);
    refresh_cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);
    refresh_cookie.set_path("/");
    let refresh_max_age = state.config.security.refresh_token_expiry_days as i64 * 86400;
    refresh_cookie.set_max_age(tower_cookies::cookie::time::Duration::seconds(
        refresh_max_age,
    ));
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
    access_cookie.set_same_site(tower_cookies::cookie::SameSite::Lax);
    access_cookie.set_path("/");
    // Set max age to end of day to match JWT expiration
    let now = chrono::Utc::now();
    let end_of_day = now
        .date_naive()
        .and_hms_opt(23, 59, 59)
        .ok_or_else(|| {
            crate::error::AppError::Internal(anyhow::anyhow!(
                "Failed to create end of day timestamp"
            ))
        })?
        .and_utc();
    let seconds_until_eod = (end_of_day - now).num_seconds();
    access_cookie.set_max_age(tower_cookies::cookie::time::Duration::seconds(
        seconds_until_eod,
    ));
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
    // Note: Clones are necessary as Cookie::new/from require ownership of strings
    cookies.remove(Cookie::from(state.config.security.cookie_name.clone()));
    cookies.remove(Cookie::from(
        state.config.security.refresh_cookie_name.clone(),
    ));

    Ok(Json(serde_json::json!({ "success": true })))
}

/// Validate login request input
fn validate_login_request(req: &LoginRequest) -> Result<()> {
    // Username validation
    if req.username.is_empty() {
        return Err(AppError::AuthenticationFailed(
            "Username cannot be empty".to_string(),
        ));
    }
    if req.username.len() > 255 {
        return Err(AppError::AuthenticationFailed(
            "Username too long (max 255 characters)".to_string(),
        ));
    }
    if req.username.chars().any(|c| c.is_control()) {
        return Err(AppError::AuthenticationFailed(
            "Username contains invalid characters".to_string(),
        ));
    }

    // Password validation
    if req.password.is_empty() {
        return Err(AppError::AuthenticationFailed(
            "Password cannot be empty".to_string(),
        ));
    }
    if req.password.len() > 1024 {
        return Err(AppError::AuthenticationFailed(
            "Password too long (max 1024 characters)".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_login_request(username: &str, password: &str) -> LoginRequest {
        LoginRequest {
            username: username.to_string(),
            password: password.to_string(),
        }
    }

    #[test]
    fn test_validate_login_valid() {
        let req = test_login_request("testuser", "testpass");
        assert!(validate_login_request(&req).is_ok());
    }

    #[test]
    fn test_validate_login_empty_username() {
        let req = test_login_request("", "testpass");
        assert!(validate_login_request(&req).is_err());
    }

    #[test]
    fn test_validate_login_empty_password() {
        let req = test_login_request("testuser", "");
        assert!(validate_login_request(&req).is_err());
    }

    #[test]
    fn test_validate_login_username_too_long() {
        let long_username = "a".repeat(256);
        let req = test_login_request(&long_username, "testpass");
        assert!(validate_login_request(&req).is_err());
    }

    #[test]
    fn test_validate_login_password_too_long() {
        let long_password = "a".repeat(1025);
        let req = test_login_request("testuser", &long_password);
        assert!(validate_login_request(&req).is_err());
    }

    #[test]
    fn test_validate_login_username_with_control_chars() {
        let req = test_login_request("test\nuser", "testpass");
        assert!(validate_login_request(&req).is_err());
    }

    #[test]
    fn test_validate_login_max_lengths() {
        // Max valid username (255 chars)
        let username = "a".repeat(255);
        let req = test_login_request(&username, "testpass");
        assert!(validate_login_request(&req).is_ok());

        // Max valid password (1024 chars)
        let password = "a".repeat(1024);
        let req = test_login_request("testuser", &password);
        assert!(validate_login_request(&req).is_ok());
    }
}
