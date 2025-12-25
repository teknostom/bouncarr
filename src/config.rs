use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub jellyfin: JellyfinConfig,
    pub arr_apps: Vec<ArrApp>,
    pub server: ServerConfig,
    pub security: SecurityConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JellyfinConfig {
    pub url: String,
    pub api_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrApp {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub access_token_expiry_hours: u64,
    pub refresh_token_expiry_days: u64,
    pub cookie_name: String,
    pub refresh_cookie_name: String,
    pub secure_cookies: bool,
}

impl Config {
    pub fn load() -> Result<Self, config::ConfigError> {
        let config = config::Config::builder()
            // Start with default values
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 3000)?
            .set_default("security.access_token_expiry_hours", 24)?
            .set_default("security.refresh_token_expiry_days", 30)?
            .set_default("security.cookie_name", "bouncarr_token")?
            .set_default("security.refresh_cookie_name", "bouncarr_refresh")?
            .set_default("security.secure_cookies", true)?
            // Load from config.yaml (required)
            .add_source(
                config::File::from(Path::new("config.yaml"))
                    .required(true)
                    .format(config::FileFormat::Yaml),
            )
            .build()?;

        config.try_deserialize()
    }
}
