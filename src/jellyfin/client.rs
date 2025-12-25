use crate::config::JellyfinConfig;
use crate::error::{AppError, Result};
use crate::jellyfin::types::{AuthenticateRequest, AuthenticateResponse, User, UserInfo};

/// Client for interacting with Jellyfin API
#[derive(Clone)]
pub struct JellyfinClient {
    config: JellyfinConfig,
    client: reqwest::Client,
}

impl JellyfinClient {
    /// Create a new Jellyfin client
    ///
    /// # Arguments
    ///
    /// * `config` - Jellyfin server configuration
    /// * `timeout_seconds` - HTTP request timeout in seconds (use -1 for no timeout)
    ///
    /// # Errors
    ///
    /// Returns error if HTTP client creation fails
    pub fn new(config: JellyfinConfig, timeout_seconds: i64) -> Result<Self> {
        let mut client_builder = reqwest::Client::builder();
        if timeout_seconds > 0 {
            client_builder =
                client_builder.timeout(std::time::Duration::from_secs(timeout_seconds as u64));
        }

        Ok(Self {
            config,
            client: client_builder.build().map_err(|e| {
                AppError::Internal(anyhow::anyhow!("Failed to build HTTP client: {}", e))
            })?,
        })
    }

    /// Authenticate a user with Jellyfin
    ///
    /// # Arguments
    ///
    /// * `username` - Jellyfin username
    /// * `password` - Jellyfin password
    ///
    /// # Returns
    ///
    /// Returns tuple of (UserInfo, Jellyfin access token)
    ///
    /// # Errors
    ///
    /// Returns error if authentication fails or network error occurs
    pub async fn authenticate(&self, username: &str, password: &str) -> Result<(UserInfo, String)> {
        let url = format!("{}/Users/AuthenticateByName", self.config.url);

        let request = AuthenticateRequest {
            username: username.to_string(),
            pw: password.to_string(),
        };

        let response = self
            .client
            .post(&url)
            .header("X-Emby-Authorization", self.build_auth_header())
            .json(&request)
            .send()
            .await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(AppError::AuthenticationFailed(format!(
                "Jellyfin authentication failed with status {}: {}",
                status, body
            )));
        }

        let auth_response: AuthenticateResponse = response.json().await?;
        let user_info: UserInfo = auth_response.user.into();

        Ok((user_info, auth_response.access_token))
    }

    /// Get user information from Jellyfin
    ///
    /// # Arguments
    ///
    /// * `user_id` - Jellyfin user ID
    ///
    /// # Errors
    ///
    /// Returns error if user not found or network error occurs
    pub async fn get_user(&self, user_id: &str) -> Result<UserInfo> {
        let url = format!("{}/Users/{}", self.config.url, user_id);

        let response = self
            .client
            .get(&url)
            .header("X-Emby-Authorization", self.build_auth_header())
            .header("X-MediaBrowser-Token", &self.config.api_key)
            .send()
            .await?;

        if !response.status().is_success() {
            return Err(AppError::AuthenticationFailed(
                "Failed to fetch user from Jellyfin".to_string(),
            ));
        }

        let user: User = response.json().await?;
        Ok(user.into())
    }

    fn build_auth_header(&self) -> String {
        format!(
            r#"MediaBrowser Client="Bouncarr", Device="Bouncarr", DeviceId="bouncarr-1", Version="{}""#,
            env!("CARGO_PKG_VERSION")
        )
    }
}
