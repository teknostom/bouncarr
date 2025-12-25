use serde::{Deserialize, Serialize};
use std::path::Path;

/// Main application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Jellyfin server configuration
    pub jellyfin: JellyfinConfig,
    /// List of *arr applications to proxy
    pub arr_apps: Vec<ArrApp>,
    /// Server configuration
    pub server: ServerConfig,
    /// Security and authentication settings
    pub security: SecurityConfig,
}

/// Jellyfin server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JellyfinConfig {
    /// Jellyfin server URL (e.g., http://jellyfin:8096)
    pub url: String,
    /// Jellyfin API key for server authentication
    pub api_key: String,
}

/// Configuration for a single *arr application
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArrApp {
    /// Application name (used in URL path, e.g., "sonarr")
    pub name: String,
    /// Application URL (e.g., http://sonarr:8989)
    pub url: String,
}

/// Server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    /// Host to bind to (e.g., "0.0.0.0")
    pub host: String,
    /// Port to listen on
    pub port: u16,
    /// HTTP request timeout in seconds. Set to -1 to disable timeout.
    pub request_timeout_seconds: i64,
}

/// Security and authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    /// Access token expiry in hours
    pub access_token_expiry_hours: u64,
    /// Refresh token expiry in days
    pub refresh_token_expiry_days: u64,
    /// Cookie name for access token
    pub cookie_name: String,
    /// Cookie name for refresh token
    pub refresh_cookie_name: String,
    /// Whether to set Secure flag on cookies (requires HTTPS)
    pub secure_cookies: bool,
    /// JWT secret key. If not set, a random key will be generated on startup.
    /// WARNING: Random keys invalidate all tokens on server restart!
    #[serde(default)]
    pub jwt_secret: Option<String>,
}

impl Config {
    /// Load configuration from config.yaml file
    ///
    /// Also supports environment variable overrides:
    /// - `JWT_SECRET` - Override JWT secret key
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - config.yaml file is not found
    /// - Configuration is invalid (malformed YAML, missing fields)
    /// - URL validation fails
    pub fn load() -> Result<Self, config::ConfigError> {
        let config = config::Config::builder()
            // Start with default values
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 3000)?
            .set_default("server.request_timeout_seconds", -1)?
            .set_default("security.access_token_expiry_hours", 24)?
            .set_default("security.refresh_token_expiry_days", 30)?
            .set_default("security.cookie_name", "bouncarr_token")?
            .set_default("security.refresh_cookie_name", "bouncarr_refresh")?
            .set_default("security.secure_cookies", false)?
            // Load from config.yaml (required)
            .add_source(
                config::File::from(Path::new("config.yaml"))
                    .required(true)
                    .format(config::FileFormat::Yaml),
            )
            // Override with environment variables (optional)
            .set_override_option("security.jwt_secret", std::env::var("JWT_SECRET").ok())?
            .build()?;

        let cfg: Config = config.try_deserialize()?;

        // Validate configuration
        cfg.validate()?;

        Ok(cfg)
    }

    fn validate(&self) -> Result<(), config::ConfigError> {
        // Validate Jellyfin URL
        if let Err(e) = Self::validate_url(&self.jellyfin.url, "Jellyfin") {
            return Err(config::ConfigError::Message(e));
        }

        // Validate arr app URLs
        for app in &self.arr_apps {
            if let Err(e) = Self::validate_url(&app.url, &format!("Arr app '{}'", app.name)) {
                return Err(config::ConfigError::Message(e));
            }
        }

        Ok(())
    }

    fn validate_url(url: &str, context: &str) -> Result<(), String> {
        if url.is_empty() {
            return Err(format!("{} URL cannot be empty", context));
        }

        // Check if URL is valid
        match url.parse::<url::Url>() {
            Ok(parsed_url) => {
                // Ensure URL has a scheme
                if parsed_url.scheme().is_empty() {
                    return Err(format!(
                        "{} URL must have a scheme (http:// or https://)",
                        context
                    ));
                }

                // Ensure scheme is http or https
                if parsed_url.scheme() != "http" && parsed_url.scheme() != "https" {
                    return Err(format!(
                        "{} URL must use http:// or https:// scheme",
                        context
                    ));
                }

                // Ensure URL has a host
                if parsed_url.host_str().is_none() {
                    return Err(format!("{} URL must have a valid host", context));
                }

                Ok(())
            }
            Err(e) => Err(format!("{} URL is invalid: {}", context, e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_url_valid() {
        assert!(Config::validate_url("http://example.com", "Test").is_ok());
        assert!(Config::validate_url("https://example.com:8096", "Test").is_ok());
        assert!(Config::validate_url("http://localhost:8989", "Test").is_ok());
        assert!(Config::validate_url("http://192.168.1.1:7878", "Test").is_ok());
    }

    #[test]
    fn test_validate_url_invalid() {
        // Empty URL
        assert!(Config::validate_url("", "Test").is_err());

        // Missing scheme
        assert!(Config::validate_url("example.com", "Test").is_err());

        // Invalid scheme
        assert!(Config::validate_url("ftp://example.com", "Test").is_err());

        // Missing host
        assert!(Config::validate_url("http://", "Test").is_err());
    }
}
