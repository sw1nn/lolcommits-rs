//! OAuth 2.0 device authorization grant (RFC 8628) client.
//!
//! The CLI runs inside a git hook in the user's terminal, so everything here is
//! blocking: there is no async runtime to borrow and no benefit to starting one.
//!
//! Token values are wrapped in [`Secret`] so they cannot reach the logs through
//! a stray `Debug` format.

use crate::config::AuthConfig;
use crate::error::{Error, Result};
use crate::secret::Secret;
use reqwest::blocking;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

const DEVICE_CODE_GRANT: &str = "urn:ietf:params:oauth:grant-type:device_code";
const REFRESH_TOKEN_GRANT: &str = "refresh_token";

/// Poll interval to use when the authorization server does not suggest one
/// (RFC 8628 section 3.2).
const DEFAULT_POLL_INTERVAL_SECS: u64 = 5;

/// Amount the poll interval grows on each `slow_down` (RFC 8628 section 3.5).
const SLOW_DOWN_INCREMENT_SECS: u64 = 5;

/// Floor on the poll interval. An issuer advertising `interval: 0` would
/// otherwise turn polling into a busy loop against the token endpoint.
const MIN_POLL_INTERVAL_SECS: u64 = 1;

/// Assumed access token lifetime when the token response omits `expires_in`.
const DEFAULT_TOKEN_LIFETIME_SECS: i64 = 3600;

/// A set of credentials obtained from the token endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenSet {
    pub access_token: Secret,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<Secret>,

    /// Absolute expiry of the access token, in unix seconds.
    pub expires_at: i64,
}

impl TokenSet {
    /// Whether the access token is expired, or will be within `leeway_secs`.
    pub fn is_expired(&self, leeway_secs: i64) -> bool {
        chrono::Utc::now().timestamp() + leeway_secs >= self.expires_at
    }
}

/// A pending device authorization, as returned by the device endpoint.
#[derive(Debug, Deserialize)]
pub struct DeviceAuthorization {
    /// The client's half of the grant. Secret: whoever holds it can collect the
    /// tokens once the user approves.
    pub device_code: Secret,

    /// Short code the user types at the verification URI.
    pub user_code: String,

    pub verification_uri: String,

    #[serde(default)]
    pub verification_uri_complete: Option<String>,

    /// Lifetime of the device code, in seconds.
    pub expires_in: i64,

    #[serde(default)]
    pub interval: Option<u64>,
}

impl DeviceAuthorization {
    /// URL to send the user to, preferring the one that pre-fills the code.
    pub fn verification_url(&self) -> &str {
        self.verification_uri_complete
            .as_deref()
            .unwrap_or(&self.verification_uri)
    }

    /// Server-suggested poll interval, or the RFC default, never below the floor.
    pub fn poll_interval(&self) -> Duration {
        Duration::from_secs(
            self.interval
                .unwrap_or(DEFAULT_POLL_INTERVAL_SECS)
                .max(MIN_POLL_INTERVAL_SECS),
        )
    }
}

/// Start a device authorization: ask the issuer for a user code and a URL.
pub fn device_authorization(
    client: &blocking::Client,
    auth: &AuthConfig,
) -> Result<DeviceAuthorization> {
    let url = auth.device_authorization_url();
    let scope = auth.scope();
    tracing::debug!(url = %url, client_id = %auth.client_id, scope = %scope, "Requesting device authorization");

    let response = client
        .post(&url)
        .form(&[
            ("client_id", auth.client_id.as_str()),
            ("scope", scope.as_str()),
        ])
        .send()?;

    let status = response.status();
    let body = response.text()?;

    if !status.is_success() {
        return Err(Error::DeviceAuthorizationFailed {
            status: status.as_u16(),
            body,
        });
    }

    let authorization: DeviceAuthorization = serde_json::from_str(&body)?;
    tracing::debug!(
        expires_in = authorization.expires_in,
        interval_secs = authorization.poll_interval().as_secs(),
        "Device authorization issued"
    );

    Ok(authorization)
}

/// Poll the token endpoint until the user approves, denies, or the code expires.
pub fn poll_for_token(
    client: &blocking::Client,
    auth: &AuthConfig,
    authorization: &DeviceAuthorization,
) -> Result<TokenSet> {
    let url = auth.token_url();
    let deadline = Instant::now() + Duration::from_secs(authorization.expires_in.max(0) as u64);
    let mut interval = authorization.poll_interval();

    loop {
        if Instant::now() >= deadline {
            return Err(Error::DeviceCodeExpired);
        }
        std::thread::sleep(interval);

        tracing::debug!(
            interval_secs = interval.as_secs(),
            "Polling for device token"
        );
        let outcome = token_request(
            client,
            &url,
            &[
                ("client_id", auth.client_id.as_str()),
                ("grant_type", DEVICE_CODE_GRANT),
                ("device_code", authorization.device_code.expose()),
            ],
        )?;

        match outcome {
            TokenOutcome::Tokens(tokens) => return Ok(tokens),
            TokenOutcome::Error { error, description } => match error.as_str() {
                "authorization_pending" => {}
                "slow_down" => {
                    interval += Duration::from_secs(SLOW_DOWN_INCREMENT_SECS);
                    tracing::debug!(
                        interval_secs = interval.as_secs(),
                        "Server asked to slow down polling"
                    );
                }
                "expired_token" => return Err(Error::DeviceCodeExpired),
                "access_denied" => return Err(Error::DeviceAuthorizationDenied),
                _ => return Err(Error::TokenEndpointError { error, description }),
            },
        }
    }
}

/// Exchange a refresh token for a fresh access token.
pub fn refresh(
    client: &blocking::Client,
    auth: &AuthConfig,
    refresh_token: &Secret,
) -> Result<TokenSet> {
    let url = auth.token_url();
    tracing::debug!(url = %url, "Refreshing access token");

    let outcome = token_request(
        client,
        &url,
        &[
            ("client_id", auth.client_id.as_str()),
            ("grant_type", REFRESH_TOKEN_GRANT),
            ("refresh_token", refresh_token.expose()),
        ],
    )?;

    match outcome {
        TokenOutcome::Tokens(mut tokens) => {
            // Authelia rotates refresh tokens, but the RFC allows omitting a new
            // one; keep using the current one when that happens.
            if tokens.refresh_token.is_none() {
                tokens.refresh_token = Some(refresh_token.clone());
            }
            Ok(tokens)
        }
        TokenOutcome::Error { error, description } => {
            Err(Error::TokenEndpointError { error, description })
        }
    }
}

/// What the token endpoint said: credentials, or an RFC 6749 error code.
enum TokenOutcome {
    Tokens(TokenSet),
    Error {
        error: String,
        description: Option<String>,
    },
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: Secret,

    #[serde(default)]
    refresh_token: Option<Secret>,

    #[serde(default)]
    expires_in: Option<i64>,
}

impl TokenResponse {
    fn into_token_set(self, now: i64) -> TokenSet {
        TokenSet {
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            expires_at: now + self.expires_in.unwrap_or(DEFAULT_TOKEN_LIFETIME_SECS),
        }
    }
}

#[derive(Deserialize)]
struct TokenErrorResponse {
    error: String,

    #[serde(default)]
    error_description: Option<String>,
}

fn token_request(
    client: &blocking::Client,
    url: &str,
    form: &[(&str, &str)],
) -> Result<TokenOutcome> {
    let response = client.post(url).form(form).send()?;
    let status = response.status();
    let body = response.text()?;

    // The status alone is not enough: device-flow polling reports
    // `authorization_pending` as a 4xx, so the body decides.
    if let Ok(error) = serde_json::from_str::<TokenErrorResponse>(&body) {
        return Ok(TokenOutcome::Error {
            error: error.error,
            description: error.error_description,
        });
    }

    if !status.is_success() {
        return Err(Error::TokenRequestFailed {
            status: status.as_u16(),
            body,
        });
    }

    let parsed: TokenResponse = serde_json::from_str(&body)?;
    Ok(TokenOutcome::Tokens(
        parsed.into_token_set(chrono::Utc::now().timestamp()),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn device_authorization_defaults_interval_to_rfc_value() -> Result<()> {
        let authorization: DeviceAuthorization = serde_json::from_str(
            r#"{
                "device_code": "dc",
                "user_code": "ABCD-EFGH",
                "verification_uri": "https://auth.example/device",
                "expires_in": 600
            }"#,
        )?;

        assert_eq!(authorization.poll_interval(), Duration::from_secs(5));
        assert_eq!(
            authorization.verification_url(),
            "https://auth.example/device"
        );
        Ok(())
    }

    #[test]
    fn poll_interval_is_floored() -> Result<()> {
        let authorization: DeviceAuthorization = serde_json::from_str(
            r#"{
                "device_code": "dc",
                "user_code": "ABCD-EFGH",
                "verification_uri": "https://auth.example/device",
                "expires_in": 600,
                "interval": 0
            }"#,
        )?;

        assert_eq!(
            authorization.poll_interval(),
            Duration::from_secs(MIN_POLL_INTERVAL_SECS)
        );
        Ok(())
    }

    #[test]
    fn device_authorization_prefers_complete_verification_uri() -> Result<()> {
        let authorization: DeviceAuthorization = serde_json::from_str(
            r#"{
                "device_code": "dc",
                "user_code": "ABCD-EFGH",
                "verification_uri": "https://auth.example/device",
                "verification_uri_complete": "https://auth.example/device?user_code=ABCD-EFGH",
                "expires_in": 600,
                "interval": 10
            }"#,
        )?;

        assert_eq!(authorization.poll_interval(), Duration::from_secs(10));
        assert_eq!(
            authorization.verification_url(),
            "https://auth.example/device?user_code=ABCD-EFGH"
        );
        Ok(())
    }

    #[test]
    fn device_authorization_debug_redacts_device_code() -> Result<()> {
        let authorization: DeviceAuthorization = serde_json::from_str(
            r#"{
                "device_code": "super-secret",
                "user_code": "ABCD-EFGH",
                "verification_uri": "https://auth.example/device",
                "expires_in": 600
            }"#,
        )?;

        assert!(!format!("{authorization:?}").contains("super-secret"));
        Ok(())
    }

    #[test]
    fn token_response_computes_expiry_from_expires_in() -> Result<()> {
        let response: TokenResponse = serde_json::from_str(
            r#"{"access_token": "at", "refresh_token": "rt", "expires_in": 3600}"#,
        )?;
        let tokens = response.into_token_set(1_000);

        assert_eq!(tokens.expires_at, 4_600);
        assert_eq!(tokens.access_token.expose(), "at");
        assert_eq!(
            tokens.refresh_token.as_ref().map(Secret::expose),
            Some("rt")
        );
        Ok(())
    }

    #[test]
    fn token_response_without_expires_in_assumes_one_hour() -> Result<()> {
        let response: TokenResponse = serde_json::from_str(r#"{"access_token": "at"}"#)?;
        let tokens = response.into_token_set(0);

        assert_eq!(tokens.expires_at, DEFAULT_TOKEN_LIFETIME_SECS);
        assert!(tokens.refresh_token.is_none());
        Ok(())
    }

    #[test]
    fn token_error_response_parses_rfc_6749_shape() -> Result<()> {
        let error: TokenErrorResponse = serde_json::from_str(
            r#"{"error": "authorization_pending", "error_description": "pending"}"#,
        )?;

        assert_eq!(error.error, "authorization_pending");
        assert_eq!(error.error_description.as_deref(), Some("pending"));
        Ok(())
    }

    #[test]
    fn token_error_response_does_not_match_a_success_body() {
        let parsed = serde_json::from_str::<TokenErrorResponse>(
            r#"{"access_token": "at", "expires_in": 3600}"#,
        );
        assert!(parsed.is_err());
    }

    #[test]
    fn is_expired_honours_leeway() -> Result<()> {
        let now = chrono::Utc::now().timestamp();
        let tokens = TokenSet {
            access_token: Secret::new("at"),
            refresh_token: None,
            expires_at: now + 30,
        };

        assert!(!tokens.is_expired(0));
        assert!(tokens.is_expired(60));
        Ok(())
    }

    #[test]
    fn token_set_round_trips_through_json() -> Result<()> {
        let tokens = TokenSet {
            access_token: Secret::new("at"),
            refresh_token: Some(Secret::new("rt")),
            expires_at: 42,
        };

        let restored: TokenSet = serde_json::from_str(&serde_json::to_string(&tokens)?)?;

        assert_eq!(restored.access_token.expose(), "at");
        assert_eq!(
            restored.refresh_token.as_ref().map(Secret::expose),
            Some("rt")
        );
        assert_eq!(restored.expires_at, 42);
        assert!(format!("{restored:?}").contains("[redacted]"));
        Ok(())
    }
}
