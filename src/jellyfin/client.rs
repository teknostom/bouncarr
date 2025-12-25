use crate::config::JellyfinConfig;
use crate::error::{AppError, Result};
use crate::jellyfin::types::{AuthenticateRequest, AuthenticateResponse, User, UserInfo};

#[derive(Clone)]
pub struct JellyfinClient {
    config: JellyfinConfig,
    client: reqwest::Client,
}

impl JellyfinClient {
    pub fn new(config: JellyfinConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

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
        r#"MediaBrowser Client="Bouncarr", Device="Bouncarr", DeviceId="bouncarr-1", Version="0.1.0""#.to_string()
    }
}
