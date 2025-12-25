use crate::config::SecurityConfig;
use crate::error::{AppError, Result};
use crate::jellyfin::types::UserInfo;
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

/// JWT token claims
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    /// User ID (subject)
    pub sub: String,
    /// Username
    pub username: String,
    /// Whether user is an administrator
    pub is_admin: bool,
    /// Token expiration timestamp
    pub exp: i64,
    /// Token issued at timestamp
    pub iat: i64,
    /// Type of token (access or refresh)
    pub token_type: TokenType,
}

/// Type of JWT token
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TokenType {
    /// Access token for API requests
    Access,
    /// Refresh token for obtaining new access tokens
    Refresh,
}

/// JWT token manager for creating and validating tokens
pub struct JwtManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    refresh_token_expiry: Duration,
}

impl JwtManager {
    /// Create a new JWT manager
    ///
    /// If a JWT secret is configured, it will be used. Otherwise, a random secret
    /// is generated on startup.
    ///
    /// # Note
    ///
    /// Using a random secret (when jwt_secret is not configured) will invalidate
    /// all tokens on server restart. In production, you should configure a
    /// persistent jwt_secret in config.yaml or via environment variable.
    pub fn new(config: &SecurityConfig) -> Self {
        let secret = match &config.jwt_secret {
            Some(s) if !s.is_empty() => {
                tracing::info!("Using configured JWT secret");
                s.clone()
            }
            _ => {
                tracing::warn!(
                    "No JWT secret configured - generating random secret. \
                    All tokens will be invalidated on server restart! \
                    Set 'security.jwt_secret' in config.yaml or JWT_SECRET env var for production."
                );
                Self::generate_secret()
            }
        };

        Self {
            encoding_key: EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: DecodingKey::from_secret(secret.as_bytes()),
            refresh_token_expiry: Duration::days(config.refresh_token_expiry_days as i64),
        }
    }

    fn generate_secret() -> String {
        use rand::Rng;

        // Generate 32 bytes (256 bits) of cryptographically secure random data
        let mut rng = rand::thread_rng();
        let random_bytes: [u8; 32] = rng.r#gen();

        // Convert to base64 for a nice string representation
        use base64::Engine;
        base64::engine::general_purpose::STANDARD.encode(random_bytes)
    }

    /// Create an access token for a user
    ///
    /// Access tokens expire at the end of the current day.
    pub fn create_access_token(&self, user_info: &UserInfo) -> Result<String> {
        let now = Utc::now();
        // Access token expires at end of day
        let end_of_day = now
            .date_naive()
            .and_hms_opt(23, 59, 59)
            .ok_or_else(|| {
                AppError::Internal(anyhow::anyhow!("Failed to create end of day timestamp"))
            })?
            .and_utc();

        let claims = Claims {
            sub: user_info.user_id.clone(),
            username: user_info.username.clone(),
            is_admin: user_info.is_administrator,
            exp: end_of_day.timestamp(),
            iat: now.timestamp(),
            token_type: TokenType::Access,
        };

        encode(&Header::default(), &claims, &self.encoding_key).map_err(AppError::JwtError)
    }

    /// Create a refresh token for a user
    ///
    /// Refresh tokens expire after the configured number of days.
    pub fn create_refresh_token(&self, user_info: &UserInfo) -> Result<String> {
        let now = Utc::now();
        let expiry = now + self.refresh_token_expiry;

        let claims = Claims {
            sub: user_info.user_id.clone(),
            username: user_info.username.clone(),
            is_admin: user_info.is_administrator,
            exp: expiry.timestamp(),
            iat: now.timestamp(),
            token_type: TokenType::Refresh,
        };

        encode(&Header::default(), &claims, &self.encoding_key).map_err(AppError::JwtError)
    }

    /// Validate a JWT token
    ///
    /// # Arguments
    ///
    /// * `token` - JWT token string
    /// * `expected_type` - Expected token type (access or refresh)
    ///
    /// # Errors
    ///
    /// Returns error if token is invalid, expired, or type mismatch
    pub fn validate_token(&self, token: &str, expected_type: TokenType) -> Result<Claims> {
        let token_data = decode::<Claims>(token, &self.decoding_key, &Validation::default())?;

        if token_data.claims.token_type != expected_type {
            return Err(AppError::InvalidToken);
        }

        Ok(token_data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> SecurityConfig {
        SecurityConfig {
            access_token_expiry_hours: 24,
            refresh_token_expiry_days: 30,
            cookie_name: "test_token".to_string(),
            refresh_cookie_name: "test_refresh".to_string(),
            secure_cookies: false,
            jwt_secret: Some("test-secret-key-for-testing".to_string()),
        }
    }

    fn test_user_info() -> UserInfo {
        UserInfo {
            user_id: "test-user-123".to_string(),
            username: "testuser".to_string(),
            is_administrator: true,
        }
    }

    #[test]
    fn test_create_and_validate_access_token() {
        let config = test_config();
        let manager = JwtManager::new(&config);
        let user_info = test_user_info();

        // Create access token
        let token = manager.create_access_token(&user_info).unwrap();
        assert!(!token.is_empty());

        // Validate token
        let claims = manager.validate_token(&token, TokenType::Access).unwrap();
        assert_eq!(claims.sub, user_info.user_id);
        assert_eq!(claims.username, user_info.username);
        assert_eq!(claims.is_admin, user_info.is_administrator);
        assert_eq!(claims.token_type, TokenType::Access);
    }

    #[test]
    fn test_create_and_validate_refresh_token() {
        let config = test_config();
        let manager = JwtManager::new(&config);
        let user_info = test_user_info();

        // Create refresh token
        let token = manager.create_refresh_token(&user_info).unwrap();
        assert!(!token.is_empty());

        // Validate token
        let claims = manager.validate_token(&token, TokenType::Refresh).unwrap();
        assert_eq!(claims.sub, user_info.user_id);
        assert_eq!(claims.username, user_info.username);
        assert_eq!(claims.token_type, TokenType::Refresh);
    }

    #[test]
    fn test_token_type_mismatch() {
        let config = test_config();
        let manager = JwtManager::new(&config);
        let user_info = test_user_info();

        // Create access token but try to validate as refresh
        let token = manager.create_access_token(&user_info).unwrap();
        let result = manager.validate_token(&token, TokenType::Refresh);
        assert!(result.is_err());

        // Create refresh token but try to validate as access
        let token = manager.create_refresh_token(&user_info).unwrap();
        let result = manager.validate_token(&token, TokenType::Access);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_token() {
        let config = test_config();
        let manager = JwtManager::new(&config);

        let result = manager.validate_token("invalid.token.here", TokenType::Access);
        assert!(result.is_err());
    }

    #[test]
    fn test_different_secrets_produce_different_tokens() {
        let user_info = test_user_info();

        let config1 = test_config();
        let manager1 = JwtManager::new(&config1);

        let mut config2 = test_config();
        config2.jwt_secret = Some("different-secret".to_string());
        let manager2 = JwtManager::new(&config2);

        let token1 = manager1.create_access_token(&user_info).unwrap();
        let token2 = manager2.create_access_token(&user_info).unwrap();

        // Tokens should be different
        assert_ne!(token1, token2);

        // manager1 cannot validate token from manager2
        let result = manager1.validate_token(&token2, TokenType::Access);
        assert!(result.is_err());
    }
}
