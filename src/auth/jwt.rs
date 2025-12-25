use crate::config::SecurityConfig;
use crate::error::{AppError, Result};
use crate::jellyfin::types::UserInfo;
use chrono::{Duration, Utc};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation, decode, encode};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String, // user_id
    pub username: String,
    pub is_admin: bool,
    pub exp: i64, // expiration time
    pub iat: i64, // issued at
    pub token_type: TokenType,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TokenType {
    Access,
    Refresh,
}

pub struct JwtManager {
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    refresh_token_expiry: Duration,
}

impl JwtManager {
    pub fn new(config: &SecurityConfig) -> Self {
        // Generate a random secret key on startup
        // In production, you might want to make this configurable or persisted
        let secret = Self::generate_secret();

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

    pub fn create_access_token(&self, user_info: &UserInfo) -> Result<String> {
        let now = Utc::now();
        // Access token expires at end of day
        let end_of_day = now.date_naive().and_hms_opt(23, 59, 59).unwrap().and_utc();

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

    pub fn validate_token(&self, token: &str, expected_type: TokenType) -> Result<Claims> {
        let token_data = decode::<Claims>(token, &self.decoding_key, &Validation::default())?;

        if token_data.claims.token_type != expected_type {
            return Err(AppError::InvalidToken);
        }

        Ok(token_data.claims)
    }
}
