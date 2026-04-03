use crate::github_copilot_storage::GitHubCopilotAuth;
use chrono::DateTime;
use chrono::TimeDelta;
use chrono::Utc;
use codex_client::BuildCustomCaTransportError;
use codex_client::build_reqwest_client_with_custom_ca;
use reqwest::Client;
use reqwest::StatusCode;
use serde::Deserialize;
use serde::Serialize;
use std::io;
use std::time::Duration;
use std::time::Instant;

const DEFAULT_GITHUB_DOMAIN: &str = "github.com";
pub const GITHUB_COPILOT_CLIENT_ID: &str = "Iv1.b507a08c87ecfe98";
pub const DEFAULT_GITHUB_COPILOT_API_BASE_URL: &str = "https://api.githubcopilot.com";
const COPILOT_USER_AGENT: &str = "GitHubCopilotChat/0.35.0";
const COPILOT_EDITOR_VERSION: &str = "vscode/1.107.0";
const COPILOT_EDITOR_PLUGIN_VERSION: &str = "copilot-chat/0.35.0";
const COPILOT_INTEGRATION_ID: &str = "vscode-chat";
const MAX_DEVICE_FLOW_WAIT: Duration = Duration::from_secs(15 * 60);
const GITHUB_COPILOT_TOKEN_REFRESH_BUFFER_MINUTES: i64 = 5;

pub fn build_github_copilot_client() -> Result<Client, BuildCustomCaTransportError> {
    build_reqwest_client_with_custom_ca(reqwest::Client::builder())
}

pub fn github_copilot_access_token_is_stale(auth: &GitHubCopilotAuth) -> bool {
    auth.copilot_token_expires_at
        <= Utc::now() + TimeDelta::minutes(GITHUB_COPILOT_TOKEN_REFRESH_BUFFER_MINUTES)
}

pub async fn refresh_github_copilot_auth(
    client: &Client,
    auth: &GitHubCopilotAuth,
) -> io::Result<GitHubCopilotAuth> {
    let token = exchange_github_copilot_access_token(
        client,
        &auth.github_access_token,
        auth.enterprise_domain.as_deref(),
    )
    .await?;

    Ok(GitHubCopilotAuth::new(
        auth.github_access_token.clone(),
        token.token,
        token.expires_at,
        token.api_base_url,
        auth.enterprise_domain.clone(),
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubCopilotEndpoints {
    pub device_code_url: String,
    pub access_token_url: String,
    pub copilot_token_url: String,
}

impl GitHubCopilotEndpoints {
    pub fn for_domain(domain: &str) -> Self {
        Self {
            device_code_url: format!("https://{domain}/login/device/code"),
            access_token_url: format!("https://{domain}/login/oauth/access_token"),
            copilot_token_url: format!("https://api.{domain}/copilot_internal/v2/token"),
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct GitHubDeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub interval: u64,
    pub expires_in: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitHubCopilotAccessToken {
    pub token: String,
    pub expires_at: DateTime<Utc>,
    pub api_base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubAccessTokenSuccess {
    access_token: String,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubAccessTokenError {
    error: String,
    error_description: Option<String>,
    interval: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct GitHubCopilotTokenResponse {
    token: String,
    expires_at: i64,
}

pub fn normalize_github_domain(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    let candidate = if trimmed.contains("://") {
        trimmed.to_string()
    } else {
        format!("https://{trimmed}")
    };

    let url = reqwest::Url::parse(&candidate).ok()?;
    let host = url.host_str()?.trim();
    if host.is_empty() {
        return None;
    }

    Some(host.to_string())
}

pub fn github_domain_or_default(domain: Option<&str>) -> &str {
    domain
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(DEFAULT_GITHUB_DOMAIN)
}

pub fn github_copilot_api_base_url(token: Option<&str>, enterprise_domain: Option<&str>) -> String {
    if let Some(token) = token
        && let Some(base_url) = github_copilot_api_base_url_from_token(token)
    {
        return base_url;
    }

    if let Some(domain) = enterprise_domain
        && !domain.trim().is_empty()
    {
        return format!("https://copilot-api.{domain}");
    }

    DEFAULT_GITHUB_COPILOT_API_BASE_URL.to_string()
}

pub fn github_copilot_api_base_url_from_token(token: &str) -> Option<String> {
    let proxy_ep = token
        .split(';')
        .find_map(|part| part.strip_prefix("proxy-ep="))?;
    let api_host = if let Some(host) = proxy_ep.strip_prefix("proxy.") {
        format!("api.{host}")
    } else if proxy_ep.starts_with("api.") {
        proxy_ep.to_string()
    } else {
        format!("api.{proxy_ep}")
    };
    Some(format!("https://{api_host}"))
}

pub async fn request_github_device_code(
    client: &Client,
    domain: Option<&str>,
    client_id: Option<&str>,
) -> io::Result<GitHubDeviceCode> {
    let endpoints = GitHubCopilotEndpoints::for_domain(github_domain_or_default(domain));
    let response = client
        .post(&endpoints.device_code_url)
        .header("accept", "application/json")
        .header("content-type", "application/x-www-form-urlencoded")
        .header("user-agent", COPILOT_USER_AGENT)
        .form(&[
            ("client_id", client_id.unwrap_or(GITHUB_COPILOT_CLIENT_ID)),
            ("scope", "read:user"),
        ])
        .send()
        .await
        .map_err(io::Error::other)?;

    decode_json_response(response, "GitHub device code request failed").await
}

pub async fn poll_github_device_access_token(
    client: &Client,
    device_code: &GitHubDeviceCode,
    domain: Option<&str>,
    client_id: Option<&str>,
) -> io::Result<String> {
    let endpoints = GitHubCopilotEndpoints::for_domain(github_domain_or_default(domain));
    let start = Instant::now();
    let mut interval = device_code.interval.max(1);

    while start.elapsed() < MAX_DEVICE_FLOW_WAIT {
        tokio::time::sleep(Duration::from_secs(interval)).await;

        let response = client
            .post(&endpoints.access_token_url)
            .header("accept", "application/json")
            .header("content-type", "application/x-www-form-urlencoded")
            .header("user-agent", COPILOT_USER_AGENT)
            .form(&[
                ("client_id", client_id.unwrap_or(GITHUB_COPILOT_CLIENT_ID)),
                ("device_code", device_code.device_code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await
            .map_err(io::Error::other)?;

        let status = response.status();
        let body = response.text().await.map_err(io::Error::other)?;
        if status != StatusCode::OK {
            return Err(io::Error::other(format!(
                "GitHub device token request failed with status {status}: {body}"
            )));
        }

        if let Ok(success) = serde_json::from_str::<GitHubAccessTokenSuccess>(&body) {
            return Ok(success.access_token);
        }

        let error: GitHubAccessTokenError = serde_json::from_str(&body).map_err(|err| {
            io::Error::other(format!(
                "failed to decode GitHub device token response: {err}; body: {body}"
            ))
        })?;

        match error.error.as_str() {
            "authorization_pending" => continue,
            "slow_down" => {
                interval = error.interval.unwrap_or(interval.saturating_add(5)).max(interval + 1);
            }
            _ => {
                let suffix = error
                    .error_description
                    .as_deref()
                    .map(|description| format!(": {description}"))
                    .unwrap_or_default();
                return Err(io::Error::other(format!(
                    "GitHub device flow failed: {}{suffix}",
                    error.error
                )));
            }
        }
    }

    Err(io::Error::other("GitHub device flow timed out"))
}

pub async fn exchange_github_copilot_access_token(
    client: &Client,
    github_access_token: &str,
    enterprise_domain: Option<&str>,
) -> io::Result<GitHubCopilotAccessToken> {
    let endpoints = GitHubCopilotEndpoints::for_domain(github_domain_or_default(enterprise_domain));
    let response = client
        .get(&endpoints.copilot_token_url)
        .header("accept", "application/json")
        .header("authorization", format!("Bearer {github_access_token}"))
        .header("user-agent", COPILOT_USER_AGENT)
        .header("editor-version", COPILOT_EDITOR_VERSION)
        .header("editor-plugin-version", COPILOT_EDITOR_PLUGIN_VERSION)
        .header("copilot-integration-id", COPILOT_INTEGRATION_ID)
        .send()
        .await
        .map_err(io::Error::other)?;

    let payload: GitHubCopilotTokenResponse =
        decode_json_response(response, "GitHub Copilot token exchange failed").await?;
    let expires_at = DateTime::<Utc>::from_timestamp(payload.expires_at, 0).ok_or_else(|| {
        io::Error::other(format!(
            "GitHub Copilot token exchange returned invalid expires_at: {}",
            payload.expires_at
        ))
    })?;
    let api_base_url = github_copilot_api_base_url(Some(&payload.token), enterprise_domain);

    Ok(GitHubCopilotAccessToken {
        token: payload.token,
        expires_at,
        api_base_url,
    })
}

async fn decode_json_response<T: for<'de> Deserialize<'de>>(
    response: reqwest::Response,
    context: &str,
) -> io::Result<T> {
    let status = response.status();
    let body = response.text().await.map_err(io::Error::other)?;
    if !status.is_success() {
        return Err(io::Error::other(format!("{context} with status {status}: {body}")));
    }

    serde_json::from_str(&body).map_err(|err| {
        io::Error::other(format!(
            "{context}: failed to decode response: {err}; body: {body}"
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn normalize_github_domain_accepts_host_and_url() {
        assert_eq!(normalize_github_domain("github.com"), Some("github.com".to_string()));
        assert_eq!(
            normalize_github_domain("https://octo.example.com/path"),
            Some("octo.example.com".to_string())
        );
    }

    #[test]
    fn normalize_github_domain_rejects_empty_input() {
        assert_eq!(normalize_github_domain("  "), None);
    }

    #[test]
    fn github_copilot_endpoints_use_expected_paths() {
        let endpoints = GitHubCopilotEndpoints::for_domain("github.com");
        assert_eq!(
            endpoints,
            GitHubCopilotEndpoints {
                device_code_url: "https://github.com/login/device/code".to_string(),
                access_token_url: "https://github.com/login/oauth/access_token".to_string(),
                copilot_token_url: "https://api.github.com/copilot_internal/v2/token".to_string(),
            }
        );
    }

    #[test]
    fn github_copilot_api_base_url_prefers_token_proxy_endpoint() {
        let token = "tid=x;proxy-ep=proxy.individual.githubcopilot.com;exp=1";
        assert_eq!(
            github_copilot_api_base_url(Some(token), None),
            "https://api.individual.githubcopilot.com"
        );
    }

    #[test]
    fn github_copilot_api_base_url_falls_back_to_enterprise_or_default() {
        assert_eq!(
            github_copilot_api_base_url(None, Some("ghe.example.com")),
            "https://copilot-api.ghe.example.com"
        );
        assert_eq!(
            github_copilot_api_base_url(None, None),
            DEFAULT_GITHUB_COPILOT_API_BASE_URL
        );
    }

    #[test]
    fn github_copilot_api_base_url_from_token_handles_prefixed_and_unprefixed_hosts() {
        assert_eq!(
            github_copilot_api_base_url_from_token("proxy-ep=proxy.individual.githubcopilot.com"),
            Some("https://api.individual.githubcopilot.com".to_string())
        );
        assert_eq!(
            github_copilot_api_base_url_from_token("proxy-ep=api.enterprise.githubcopilot.com"),
            Some("https://api.enterprise.githubcopilot.com".to_string())
        );
        assert_eq!(
            github_copilot_api_base_url_from_token("proxy-ep=enterprise.githubcopilot.com"),
            Some("https://api.enterprise.githubcopilot.com".to_string())
        );
    }

    #[test]
    fn github_copilot_access_token_is_stale_uses_buffer() {
        let fresh = GitHubCopilotAuth::new(
            "gho_test".to_string(),
            "copilot_test".to_string(),
            Utc::now() + TimeDelta::minutes(10),
            DEFAULT_GITHUB_COPILOT_API_BASE_URL.to_string(),
            None,
        );
        assert!(!github_copilot_access_token_is_stale(&fresh));

        let stale = GitHubCopilotAuth::new(
            "gho_test".to_string(),
            "copilot_test".to_string(),
            Utc::now() + TimeDelta::minutes(1),
            DEFAULT_GITHUB_COPILOT_API_BASE_URL.to_string(),
            None,
        );
        assert!(github_copilot_access_token_is_stale(&stale));
    }
}
