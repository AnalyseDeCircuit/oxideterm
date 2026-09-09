// Copyright (C) 2026 AnalyseDeCircuit
// SPDX-License-Identifier: GPL-3.0-only

//! OAuth and device authorization flows for cloud providers.

use super::*;

const MICROSOFT_AUTH_TENANT: &str = "common";
const MICROSOFT_ONEDRIVE_SCOPE: &str = "offline_access Files.ReadWrite.AppFolder";
const GOOGLE_OAUTH_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_OAUTH_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_DRIVE_APPDATA_SCOPE: &str = "https://www.googleapis.com/auth/drive.appdata";

impl CloudSyncBackend {
    pub async fn start_github_device_flow(&self, client_id: &str) -> Result<GithubDeviceCode> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            bail!("missing_github_oauth_client_id: GitHub OAuth client ID is not configured");
        }
        let response = execute_cloud_request(
            self.client
                .post("https://github.com/login/device/code")
                .header(ACCEPT, "application/json")
                .form(&[("client_id", client_id), ("scope", "gist")]),
        )
        .await?;
        let status = response.status();
        let value = response
            .json::<GithubDeviceCodeResponse>()
            .await
            .map_err(anyhow::Error::new)?;
        if !status.is_success() {
            bail!("github_oauth_start_failed: GitHub rejected the device authorization request");
        }
        Ok(GithubDeviceCode {
            device_code: value.device_code,
            user_code: value.user_code,
            verification_uri: value.verification_uri,
            expires_in: value.expires_in,
            interval: value.interval,
        })
    }

    pub async fn poll_github_device_flow(
        &self,
        client_id: &str,
        device_code: &str,
        interval: u64,
    ) -> Result<GithubDeviceTokenPoll> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            bail!("missing_github_oauth_client_id: GitHub OAuth client ID is not configured");
        }
        let response = execute_cloud_request(
            self.client
                .post("https://github.com/login/oauth/access_token")
                .header(ACCEPT, "application/json")
                .form(&[
                    ("client_id", client_id),
                    ("device_code", device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ]),
        )
        .await?;
        let status = response.status();
        let value = response
            .json::<GithubDeviceTokenResponse>()
            .await
            .map_err(anyhow::Error::new)?;
        if !status.is_success() {
            bail!("github_oauth_poll_failed: GitHub rejected the device token request");
        }
        if let Some(access_token) = value.access_token {
            // The OAuth access token leaves the HTTP response as a String only
            // long enough to move into a zeroizing owner for keychain storage.
            return Ok(GithubDeviceTokenPoll::Token {
                access_token: Zeroizing::new(access_token),
            });
        }
        match value.error.as_deref() {
            Some("authorization_pending") => Ok(GithubDeviceTokenPoll::Pending {
                interval: value.interval.unwrap_or(interval),
            }),
            Some("slow_down") => Ok(GithubDeviceTokenPoll::SlowDown {
                interval: value.interval.unwrap_or(interval + 5),
            }),
            Some("expired_token") => bail!("github_oauth_expired: GitHub device code expired"),
            Some("access_denied") => bail!("github_oauth_denied: GitHub authorization was denied"),
            Some("incorrect_client_credentials") => {
                bail!("github_oauth_bad_client: GitHub OAuth client ID is invalid")
            }
            Some(error) => {
                let description = value
                    .error_description
                    .as_deref()
                    .unwrap_or("GitHub OAuth device flow failed");
                bail!("github_oauth_{error}: {description}")
            }
            None => bail!("github_oauth_empty_response: GitHub did not return an access token"),
        }
    }

    pub async fn start_microsoft_device_flow(
        &self,
        client_id: &str,
    ) -> Result<MicrosoftDeviceCode> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            bail!("missing_microsoft_oauth_client_id: Microsoft OAuth client ID is not configured");
        }
        let response = execute_cloud_request(
            self.client
                .post(microsoft_device_code_url())
                .header(ACCEPT, "application/json")
                .form(&[
                    ("client_id", client_id),
                    ("scope", MICROSOFT_ONEDRIVE_SCOPE),
                ]),
        )
        .await?;
        let status = response.status();
        let value = response.json::<Value>().await.map_err(anyhow::Error::new)?;
        if !status.is_success() {
            return Err(microsoft_oauth_value_error(
                &value,
                "microsoft_oauth_start_failed",
            ));
        }
        let value = serde_json::from_value::<MicrosoftDeviceCodeResponse>(value)
            .map_err(anyhow::Error::new)?;
        Ok(MicrosoftDeviceCode {
            device_code: value.device_code,
            user_code: value.user_code,
            verification_uri: value.verification_uri,
            expires_in: value.expires_in,
            interval: value.interval,
        })
    }

    pub async fn poll_microsoft_device_flow(
        &self,
        client_id: &str,
        device_code: &str,
        interval: u64,
    ) -> Result<MicrosoftDeviceTokenPoll> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            bail!("missing_microsoft_oauth_client_id: Microsoft OAuth client ID is not configured");
        }
        let response = execute_cloud_request(
            self.client
                .post(microsoft_token_url())
                .header(ACCEPT, "application/json")
                .form(&[
                    ("client_id", client_id),
                    ("device_code", device_code),
                    ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
                ]),
        )
        .await?;
        let status = response.status();
        let value = response
            .json::<MicrosoftTokenResponse>()
            .await
            .map_err(anyhow::Error::new)?;
        if !status.is_success()
            && !matches!(
                value.error.as_deref(),
                Some("authorization_pending" | "slow_down")
            )
        {
            return Err(microsoft_oauth_error(&value, "microsoft_oauth_poll_failed"));
        }
        if let Some(access_token) = value.access_token {
            let refresh_token = value.refresh_token.context(
                "microsoft_oauth_empty_response: Microsoft did not return a refresh token",
            )?;
            // Microsoft returns opaque tokens; move both into zeroizing owners
            // immediately and never derive behavior from token contents.
            return Ok(MicrosoftDeviceTokenPoll::Token {
                access_token: Zeroizing::new(access_token),
                refresh_token: Zeroizing::new(refresh_token),
            });
        }
        match value.error.as_deref() {
            Some("authorization_pending") => Ok(MicrosoftDeviceTokenPoll::Pending {
                interval: value.interval.unwrap_or(interval),
            }),
            Some("slow_down") => Ok(MicrosoftDeviceTokenPoll::SlowDown {
                interval: value.interval.unwrap_or(interval + 5),
            }),
            _ => Err(microsoft_oauth_error(
                &value,
                "microsoft_oauth_empty_response",
            )),
        }
    }

    pub async fn refresh_microsoft_access_token(
        &self,
        client_id: &str,
        refresh_token: &str,
    ) -> Result<MicrosoftTokenRefresh> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            bail!("missing_microsoft_oauth_client_id: Microsoft OAuth client ID is not configured");
        }
        if refresh_token.trim().is_empty() {
            bail!("missing_microsoft_refresh_token: Microsoft refresh token is not configured");
        }
        let response = execute_cloud_request(
            self.client
                .post(microsoft_token_url())
                .header(ACCEPT, "application/json")
                .form(&[
                    ("client_id", client_id),
                    ("scope", MICROSOFT_ONEDRIVE_SCOPE),
                    ("refresh_token", refresh_token),
                    ("grant_type", "refresh_token"),
                ]),
        )
        .await?;
        let status = response.status();
        let value = response
            .json::<MicrosoftTokenResponse>()
            .await
            .map_err(anyhow::Error::new)?;
        if !status.is_success() {
            return Err(microsoft_oauth_error(
                &value,
                "microsoft_oauth_refresh_failed",
            ));
        }
        let access_token = value
            .access_token
            .context("microsoft_oauth_empty_response: Microsoft did not return an access token")?;
        Ok(MicrosoftTokenRefresh {
            access_token: Zeroizing::new(access_token),
            refresh_token: value.refresh_token.map(Zeroizing::new),
        })
    }

    pub fn start_google_oauth_flow(
        &self,
        client_id: &str,
        redirect_uri: &str,
    ) -> Result<GoogleOauthStart> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            bail!("missing_google_oauth_client_id: Google OAuth client ID is not configured");
        }
        let redirect_uri = redirect_uri.trim();
        if redirect_uri.is_empty() {
            bail!("google_oauth_redirect_failed: Google OAuth redirect URI is not configured");
        }
        let code_verifier = generate_google_pkce_verifier();
        let code_challenge = google_pkce_challenge(code_verifier.as_str());
        let state = generate_google_oauth_state();
        let mut url = Url::parse(GOOGLE_OAUTH_AUTH_URL)
            .context("google_oauth_start_failed: invalid Google OAuth authorization URL")?;
        url.query_pairs_mut()
            .append_pair("client_id", client_id)
            .append_pair("redirect_uri", redirect_uri)
            .append_pair("response_type", "code")
            .append_pair("scope", GOOGLE_DRIVE_APPDATA_SCOPE)
            .append_pair("access_type", "offline")
            .append_pair("prompt", "consent")
            .append_pair("code_challenge", &code_challenge)
            .append_pair("code_challenge_method", "S256")
            .append_pair("state", &state);
        Ok(GoogleOauthStart {
            authorization_url: url.to_string(),
            state,
            code_verifier: Zeroizing::new(code_verifier),
        })
    }

    pub async fn exchange_google_authorization_code(
        &self,
        client_id: &str,
        code: &str,
        code_verifier: &str,
        redirect_uri: &str,
    ) -> Result<GoogleTokenRefresh> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            bail!("missing_google_oauth_client_id: Google OAuth client ID is not configured");
        }
        if code.trim().is_empty() {
            bail!("google_oauth_empty_response: Google did not return an authorization code");
        }
        let response = execute_cloud_request(
            self.client
                .post(GOOGLE_OAUTH_TOKEN_URL)
                .header(ACCEPT, "application/json")
                .form(&[
                    ("client_id", client_id),
                    ("code", code),
                    ("code_verifier", code_verifier),
                    ("redirect_uri", redirect_uri),
                    ("grant_type", "authorization_code"),
                ]),
        )
        .await?;
        let status = response.status();
        let value = response
            .json::<GoogleTokenResponse>()
            .await
            .map_err(anyhow::Error::new)?;
        if !status.is_success() {
            return Err(google_oauth_error(&value, "google_oauth_exchange_failed"));
        }
        let access_token = value
            .access_token
            .context("google_oauth_empty_response: Google did not return an access token")?;
        let refresh_token = value
            .refresh_token
            .context("google_oauth_empty_response: Google did not return a refresh token")?;
        Ok(GoogleTokenRefresh {
            access_token: Zeroizing::new(access_token),
            refresh_token: Some(Zeroizing::new(refresh_token)),
        })
    }

    pub async fn refresh_google_access_token(
        &self,
        client_id: &str,
        refresh_token: &str,
    ) -> Result<GoogleTokenRefresh> {
        let client_id = client_id.trim();
        if client_id.is_empty() {
            bail!("missing_google_oauth_client_id: Google OAuth client ID is not configured");
        }
        if refresh_token.trim().is_empty() {
            bail!("missing_google_refresh_token: Google refresh token is not configured");
        }
        let response = execute_cloud_request(
            self.client
                .post(GOOGLE_OAUTH_TOKEN_URL)
                .header(ACCEPT, "application/json")
                .form(&[
                    ("client_id", client_id),
                    ("refresh_token", refresh_token),
                    ("grant_type", "refresh_token"),
                ]),
        )
        .await?;
        let status = response.status();
        let value = response
            .json::<GoogleTokenResponse>()
            .await
            .map_err(anyhow::Error::new)?;
        if !status.is_success() {
            return Err(google_oauth_error(&value, "google_oauth_refresh_failed"));
        }
        let access_token = value
            .access_token
            .context("google_oauth_empty_response: Google did not return an access token")?;
        Ok(GoogleTokenRefresh {
            access_token: Zeroizing::new(access_token),
            refresh_token: value.refresh_token.map(Zeroizing::new),
        })
    }
}

#[derive(Clone, Eq, PartialEq)]
pub struct GithubDeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

#[derive(Clone, Eq, PartialEq)]
pub struct MicrosoftDeviceCode {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

impl std::fmt::Debug for MicrosoftDeviceCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MicrosoftDeviceCode")
            .field("device_code", &"[redacted device code]")
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish()
    }
}

impl std::fmt::Debug for GithubDeviceCode {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GithubDeviceCode")
            .field("device_code", &"[redacted device code]")
            .field("user_code", &self.user_code)
            .field("verification_uri", &self.verification_uri)
            .field("expires_in", &self.expires_in)
            .field("interval", &self.interval)
            .finish()
    }
}

#[derive(Eq, PartialEq)]
pub enum GithubDeviceTokenPoll {
    Pending { interval: u64 },
    SlowDown { interval: u64 },
    Token { access_token: Zeroizing<String> },
}

impl std::fmt::Debug for GithubDeviceTokenPoll {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending { interval } => formatter
                .debug_struct("Pending")
                .field("interval", interval)
                .finish(),
            Self::SlowDown { interval } => formatter
                .debug_struct("SlowDown")
                .field("interval", interval)
                .finish(),
            Self::Token { .. } => formatter
                .debug_struct("Token")
                .field("access_token", &"[redacted token]")
                .finish(),
        }
    }
}

#[derive(Eq, PartialEq)]
pub enum MicrosoftDeviceTokenPoll {
    Pending {
        interval: u64,
    },
    SlowDown {
        interval: u64,
    },
    Token {
        access_token: Zeroizing<String>,
        refresh_token: Zeroizing<String>,
    },
}

impl std::fmt::Debug for MicrosoftDeviceTokenPoll {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending { interval } => formatter
                .debug_struct("Pending")
                .field("interval", interval)
                .finish(),
            Self::SlowDown { interval } => formatter
                .debug_struct("SlowDown")
                .field("interval", interval)
                .finish(),
            Self::Token { .. } => formatter
                .debug_struct("Token")
                .field("access_token", &"[redacted token]")
                .field("refresh_token", &"[redacted token]")
                .finish(),
        }
    }
}

#[derive(Eq, PartialEq)]
pub struct MicrosoftTokenRefresh {
    pub access_token: Zeroizing<String>,
    pub refresh_token: Option<Zeroizing<String>>,
}

impl std::fmt::Debug for MicrosoftTokenRefresh {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MicrosoftTokenRefresh")
            .field("access_token", &"[redacted token]")
            .field(
                "refresh_token",
                &self
                    .refresh_token
                    .as_ref()
                    .map(|_| "[redacted token]")
                    .unwrap_or("None"),
            )
            .finish()
    }
}

#[derive(Eq, PartialEq)]
pub struct GoogleOauthStart {
    pub authorization_url: String,
    pub state: String,
    pub code_verifier: Zeroizing<String>,
}

impl std::fmt::Debug for GoogleOauthStart {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GoogleOauthStart")
            .field("authorization_url", &self.authorization_url)
            .field("state", &"[redacted state]")
            .field("code_verifier", &"[redacted verifier]")
            .finish()
    }
}

#[derive(Eq, PartialEq)]
pub struct GoogleTokenRefresh {
    pub access_token: Zeroizing<String>,
    pub refresh_token: Option<Zeroizing<String>>,
}

impl std::fmt::Debug for GoogleTokenRefresh {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("GoogleTokenRefresh")
            .field("access_token", &"[redacted token]")
            .field(
                "refresh_token",
                &self
                    .refresh_token
                    .as_ref()
                    .map(|_| "[redacted token]")
                    .unwrap_or("None"),
            )
            .finish()
    }
}

#[derive(Debug, Deserialize)]
struct GithubDeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_github_device_interval")]
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct GithubDeviceTokenResponse {
    access_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    interval: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct MicrosoftDeviceCodeResponse {
    device_code: String,
    user_code: String,
    verification_uri: String,
    expires_in: u64,
    #[serde(default = "default_microsoft_device_interval")]
    interval: u64,
}

#[derive(Debug, Deserialize)]
struct MicrosoftTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    interval: Option<u64>,
}

#[derive(Deserialize)]
struct GoogleTokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    error: Option<String>,
    error_description: Option<String>,
    error_uri: Option<String>,
}

impl fmt::Debug for GoogleTokenResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // OAuth token responses can contain bearer and refresh tokens. Keep
        // diagnostics useful while guaranteeing token material never enters
        // logs, test failures, or UI-facing error formatting.
        f.debug_struct("GoogleTokenResponse")
            .field(
                "access_token",
                &self.access_token.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("error", &self.error)
            .field("error_description", &self.error_description)
            .field("error_uri", &self.error_uri)
            .finish()
    }
}

fn microsoft_device_code_url() -> String {
    format!("https://login.microsoftonline.com/{MICROSOFT_AUTH_TENANT}/oauth2/v2.0/devicecode")
}

fn microsoft_token_url() -> String {
    format!("https://login.microsoftonline.com/{MICROSOFT_AUTH_TENANT}/oauth2/v2.0/token")
}

fn generate_google_pkce_verifier() -> String {
    random_urlsafe_string(32)
}

fn generate_google_oauth_state() -> String {
    random_urlsafe_string(24)
}

fn random_urlsafe_string(byte_len: usize) -> String {
    let mut bytes = vec![0u8; byte_len];
    rand::thread_rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn google_pkce_challenge(code_verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(code_verifier.as_bytes()))
}

fn google_oauth_error(value: &GoogleTokenResponse, fallback_code: &str) -> anyhow::Error {
    let code = match value.error.as_deref() {
        Some("access_denied") => "google_oauth_denied",
        Some("admin_policy_enforced") => "google_oauth_admin_policy",
        Some("invalid_client") | Some("unauthorized_client") => "google_oauth_bad_client",
        Some("invalid_scope") => "google_oauth_missing_scope",
        Some("invalid_grant") => "google_oauth_refresh_failed",
        Some("consent_required") | Some("interaction_required") => "google_oauth_consent_required",
        Some("invalid_request") => "google_oauth_invalid_request",
        _ => fallback_code,
    };
    let message = value
        .error_description
        .as_deref()
        .or(value.error_uri.as_deref())
        .unwrap_or("Google OAuth failed");
    anyhow::anyhow!("{code}: {message}")
}

fn microsoft_oauth_value_error(value: &Value, fallback_code: &str) -> anyhow::Error {
    // Device authorization failures share the token endpoint error shape,
    // but are received before any token exists, so normalize them here.
    let response = MicrosoftTokenResponse {
        access_token: None,
        refresh_token: None,
        error: value
            .get("error")
            .and_then(Value::as_str)
            .map(str::to_string),
        error_description: value
            .get("error_description")
            .and_then(Value::as_str)
            .or_else(|| {
                value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .and_then(Value::as_str)
            })
            .map(str::to_string),
        interval: None,
    };
    microsoft_oauth_error(&response, fallback_code)
}

fn microsoft_oauth_error(value: &MicrosoftTokenResponse, fallback_code: &str) -> anyhow::Error {
    let code = match value.error.as_deref() {
        Some("authorization_declined") | Some("access_denied") => "microsoft_oauth_denied",
        Some("expired_token") => "microsoft_oauth_expired",
        Some("bad_verification_code") => "microsoft_oauth_bad_code",
        Some("invalid_client") | Some("unauthorized_client") => "microsoft_oauth_bad_client",
        Some("invalid_grant") => "microsoft_oauth_refresh_failed",
        Some("invalid_scope") => "microsoft_oauth_missing_scope",
        Some("consent_required") | Some("interaction_required") => {
            "microsoft_oauth_consent_required"
        }
        Some("invalid_request") => "microsoft_oauth_invalid_request",
        _ => fallback_code,
    };
    let message = value
        .error_description
        .as_deref()
        .unwrap_or("Microsoft OAuth failed");
    anyhow::anyhow!("{code}: {message}")
}

fn default_github_device_interval() -> u64 {
    5
}

fn default_microsoft_device_interval() -> u64 {
    5
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_authorization_debug_redacts_device_codes_but_keeps_user_codes() {
        let github = GithubDeviceCode {
            device_code: "secret-device-code".into(),
            user_code: "ABCD-EFGH".into(),
            verification_uri: "https://github.com/login/device".into(),
            expires_in: 900,
            interval: 5,
        };
        let microsoft = MicrosoftDeviceCode {
            device_code: "secret-microsoft-device-code".into(),
            user_code: "WXYZ-1234".into(),
            verification_uri: "https://microsoft.com/devicelogin".into(),
            expires_in: 900,
            interval: 5,
        };
        for (debug, secret, user_code) in [
            (format!("{github:?}"), "secret-device-code", "ABCD-EFGH"),
            (
                format!("{microsoft:?}"),
                "secret-microsoft-device-code",
                "WXYZ-1234",
            ),
        ] {
            assert!(debug.contains("redacted"));
            assert!(!debug.contains(secret));
            assert!(debug.contains(user_code));
        }
    }

    #[test]
    fn oauth_token_debug_redacts_token_values() {
        let github = GithubDeviceTokenPoll::Token {
            access_token: Zeroizing::new("github-secret-token".to_string()),
        };
        let microsoft = MicrosoftDeviceTokenPoll::Token {
            access_token: Zeroizing::new("microsoft-access-token".to_string()),
            refresh_token: Zeroizing::new("microsoft-refresh-token".to_string()),
        };
        let refreshed = MicrosoftTokenRefresh {
            access_token: Zeroizing::new("refreshed-access-token".to_string()),
            refresh_token: Some(Zeroizing::new("refreshed-refresh-token".to_string())),
        };
        let debug = format!("{github:?} {microsoft:?} {refreshed:?}");

        assert!(debug.contains("redacted"));
        assert!(!debug.contains("github-secret-token"));
        assert!(!debug.contains("microsoft-access-token"));
        assert!(!debug.contains("microsoft-refresh-token"));
        assert!(!debug.contains("refreshed-access-token"));
        assert!(!debug.contains("refreshed-refresh-token"));
    }

    #[test]
    fn google_oauth_token_debug_redacts_token_values() {
        let token_response = GoogleTokenResponse {
            access_token: Some("google-access-token".to_string()),
            refresh_token: Some("google-refresh-token".to_string()),
            error: None,
            error_description: None,
            error_uri: None,
        };
        let refreshed = GoogleTokenRefresh {
            access_token: Zeroizing::new("refreshed-google-access".to_string()),
            refresh_token: Some(Zeroizing::new("refreshed-google-refresh".to_string())),
        };
        let debug = format!("{token_response:?} {refreshed:?}");

        assert!(debug.contains("redacted"));
        assert!(!debug.contains("google-access-token"));
        assert!(!debug.contains("google-refresh-token"));
        assert!(!debug.contains("refreshed-google-access"));
        assert!(!debug.contains("refreshed-google-refresh"));
    }

    #[test]
    fn google_oauth_error_mapping_distinguishes_consent_admin_and_client_failures() {
        let admin = google_oauth_error(
            &GoogleTokenResponse {
                access_token: None,
                refresh_token: None,
                error: Some("admin_policy_enforced".to_string()),
                error_description: Some("Blocked by admin policy".to_string()),
                error_uri: None,
            },
            "google_oauth_exchange_failed",
        )
        .to_string();
        let bad_client = google_oauth_error(
            &GoogleTokenResponse {
                access_token: None,
                refresh_token: None,
                error: Some("unauthorized_client".to_string()),
                error_description: Some("Wrong OAuth client type".to_string()),
                error_uri: None,
            },
            "google_oauth_exchange_failed",
        )
        .to_string();
        let refresh_failed = google_oauth_error(
            &GoogleTokenResponse {
                access_token: None,
                refresh_token: None,
                error: Some("invalid_grant".to_string()),
                error_description: Some("Refresh token expired or revoked".to_string()),
                error_uri: None,
            },
            "google_oauth_exchange_failed",
        )
        .to_string();

        assert!(admin.starts_with("google_oauth_admin_policy:"));
        assert!(bad_client.starts_with("google_oauth_bad_client:"));
        assert!(refresh_failed.starts_with("google_oauth_refresh_failed:"));
    }

    #[test]
    fn microsoft_oauth_error_mapping_distinguishes_configuration_failures() {
        let scope_error = microsoft_oauth_value_error(
            &json!({
                "error": "invalid_scope",
                "error_description": "Files.ReadWrite.AppFolder is not configured"
            }),
            "microsoft_oauth_start_failed",
        )
        .to_string();
        let consent_error = microsoft_oauth_error(
            &MicrosoftTokenResponse {
                access_token: None,
                refresh_token: None,
                error: Some("consent_required".to_string()),
                error_description: Some("Admin consent is required".to_string()),
                interval: None,
            },
            "microsoft_oauth_poll_failed",
        )
        .to_string();
        let invalid_request_error = microsoft_oauth_error(
            &MicrosoftTokenResponse {
                access_token: None,
                refresh_token: None,
                error: Some("invalid_request".to_string()),
                error_description: Some("Device flow is not enabled".to_string()),
                interval: None,
            },
            "microsoft_oauth_poll_failed",
        )
        .to_string();

        assert!(scope_error.starts_with("microsoft_oauth_missing_scope:"));
        assert!(consent_error.starts_with("microsoft_oauth_consent_required:"));
        assert!(invalid_request_error.starts_with("microsoft_oauth_invalid_request:"));
    }
}
