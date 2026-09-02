//! Offline verification of Authelia access tokens.
//!
//! Uploads present an RS256 JWT (RFC 9068, `typ: at+jwt`) obtained by the CLI
//! through the OIDC device authorization grant. The daemon verifies it against
//! a cached JWKS rather than calling introspection or userinfo, so a token
//! check never depends on the identity provider being reachable.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::FromRequestParts;
use axum::http::{StatusCode, request::Parts};
use axum::response::{IntoResponse, Response};
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::config::AuthConfig;
use crate::error::{Error, Result};

/// Accepted clock skew, in seconds, when checking `exp` and `nbf`.
const CLOCK_LEEWAY_SECS: u64 = 60;

/// Shortest interval between JWKS refetches. Refetching is triggered by a
/// token carrying an unknown `kid`, which is caller-controlled, so without
/// this a caller sending garbage `kid` values could make the daemon hammer
/// the identity provider.
const MIN_JWKS_REFETCH_INTERVAL: Duration = Duration::from_secs(60);

/// Cap on a JWKS fetch. `reqwest` has no default timeout, and a fetch happens
/// both at startup and inside a request, so an unresponsive provider would
/// otherwise hang the daemon rather than being retried later.
const JWKS_FETCH_TIMEOUT: Duration = Duration::from_secs(10);

/// Claims taken from the access token. `aud` is deliberately absent: Authelia's
/// device grant returns `aud: []` whatever the client configuration, so the
/// replay boundary is `client_id` instead (see [`Authenticator::verify`]).
#[derive(Debug, Deserialize)]
struct Claims {
    sub: String,
    client_id: String,
    #[serde(default)]
    groups: Vec<String>,
    #[serde(default)]
    preferred_username: Option<String>,
}

/// The caller behind an authenticated request, for attribution in the logs.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    /// Stable subject identifier from the identity provider.
    pub subject: String,
    /// Human-readable username, when the token carries one.
    pub username: Option<String>,
}

impl AuthenticatedUser {
    /// Name to attribute an upload to, falling back to the subject.
    pub fn display_name(&self) -> &str {
        self.username.as_deref().unwrap_or(&self.subject)
    }
}

/// Verifies access tokens against a cached JSON Web Key Set.
pub struct Authenticator {
    config: AuthConfig,
    http: reqwest::Client,
    jwks: RwLock<Arc<JwkSet>>,
    last_fetch: Mutex<Option<Instant>>,
}

impl Authenticator {
    /// Build an authenticator and prime the key cache.
    ///
    /// A failed initial fetch is a warning rather than a fatal error: the
    /// daemon still starts, and the first upload triggers a refetch. This keeps
    /// the identity provider off the daemon's startup critical path.
    pub async fn new(config: AuthConfig) -> Self {
        let authenticator = Self {
            config,
            http: jwks_client(),
            jwks: RwLock::new(Arc::new(JwkSet { keys: Vec::new() })),
            last_fetch: Mutex::new(None),
        };

        if let Err(error) = authenticator.refetch_jwks().await {
            tracing::warn!(
                error = %error,
                url = %authenticator.config.jwks_url(),
                "Could not load JWKS at startup; will retry on the first upload"
            );
        }

        authenticator
    }

    /// Build an authenticator over a fixed key set, with the refetch window
    /// already consumed so verification never reaches the network.
    #[cfg(test)]
    fn with_keys(config: AuthConfig, jwks: JwkSet) -> Self {
        Self {
            config,
            http: jwks_client(),
            jwks: RwLock::new(Arc::new(jwks)),
            last_fetch: Mutex::new(Some(Instant::now())),
        }
    }

    /// Verify a bearer token and return the caller it identifies.
    ///
    /// Checks, all required: RS256 signature against the JWKS key matching the
    /// token's `kid`, then `iss`, then `client_id`, then `exp`/`nbf`, then
    /// `groups`.
    pub async fn verify(&self, token: &str) -> Result<AuthenticatedUser> {
        let header = decode_header(token)?;
        let Some(kid) = header.kid else {
            return Err(Error::TokenMissingKeyId);
        };

        let key = self.decoding_key(&kid).await?;
        let claims = decode::<Claims>(token, &key, &self.validation())?.claims;

        // `aud` is empty on every Authelia device-grant token, so `client_id`
        // is what stops a token minted for another service being replayed
        // here. Do not "improve" this to an `aud` check without first
        // confirming against a live token that `aud` is populated.
        if claims.client_id != self.config.client_id {
            return Err(Error::WrongClientId);
        }

        // Groups are set by the identity provider and cannot be forged by the
        // caller, unlike `scp`, which a public client asks for freely. Match
        // the required group exactly — `admins` is not a blanket grant.
        if !claims
            .groups
            .iter()
            .any(|g| g == &self.config.required_group)
        {
            return Err(Error::MissingRequiredGroup {
                group: self.config.required_group.clone(),
            });
        }

        Ok(AuthenticatedUser {
            subject: claims.sub,
            username: claims.preferred_username,
        })
    }

    fn validation(&self) -> Validation {
        let mut validation = Validation::new(Algorithm::RS256);
        validation.leeway = CLOCK_LEEWAY_SECS;
        validation.validate_exp = true;
        validation.validate_nbf = true;
        // Authorization is by `client_id`, not audience, because Authelia's
        // device grant always returns `aud: []`.
        validation.validate_aud = false;
        validation.set_issuer(&[&self.config.issuer]);
        validation.set_required_spec_claims(&["exp", "nbf", "iss", "sub"]);
        validation
    }

    /// Find the signing key for `kid`, refetching the key set once if it is
    /// unknown. Key rotation is manual and rare, so serving cached keys is
    /// safe and a genuinely new key costs exactly one refetch.
    async fn decoding_key(&self, kid: &str) -> Result<DecodingKey> {
        if let Some(key) = self.cached_key(kid).await {
            return Ok(key);
        }

        if !self.refetch_allowed() {
            tracing::warn!("Rejecting token with unknown key id; refetch rate-limited");
            return Err(Error::UnknownSigningKey);
        }

        self.refetch_jwks().await?;

        self.cached_key(kid).await.ok_or(Error::UnknownSigningKey)
    }

    async fn cached_key(&self, kid: &str) -> Option<DecodingKey> {
        let jwks = self.jwks.read().await.clone();
        jwks.find(kid).and_then(|jwk| {
            DecodingKey::from_jwk(jwk)
                .inspect_err(|error| {
                    tracing::warn!(kid, error = %error, "Unusable JWKS entry");
                })
                .ok()
        })
    }

    /// Consume the rate-limit window, returning whether a refetch may proceed.
    /// The window is taken before the request runs, so a failing provider does
    /// not turn every request into another outbound call.
    fn refetch_allowed(&self) -> bool {
        let mut last_fetch = self.last_fetch.lock().unwrap_or_else(|e| e.into_inner());
        let allowed = last_fetch.is_none_or(|at| at.elapsed() >= MIN_JWKS_REFETCH_INTERVAL);
        if allowed {
            *last_fetch = Some(Instant::now());
        }
        allowed
    }

    async fn refetch_jwks(&self) -> Result<()> {
        let url = self.config.jwks_url();
        tracing::debug!(url, "Fetching JWKS");

        let response = self
            .http
            .get(&url)
            .send()
            .await
            .map_err(|source| Error::JwksFetch {
                url: url.clone(),
                source,
            })?;

        let status = response.status();
        if !status.is_success() {
            return Err(Error::JwksUnavailable {
                url,
                status: status.as_u16(),
            });
        }

        let jwks: JwkSet = response.json().await.map_err(|source| Error::JwksFetch {
            url: url.clone(),
            source,
        })?;

        tracing::info!(url, keys = jwks.keys.len(), "Loaded JWKS");
        *self.jwks.write().await = Arc::new(jwks);

        // A successful fetch also opens the rate-limit window from now, so an
        // unknown `kid` seen straight after a rotation is not held off.
        *self.last_fetch.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());

        Ok(())
    }
}

fn jwks_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(JWKS_FETCH_TIMEOUT)
        .build()
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "Falling back to an untimed HTTP client for JWKS");
            reqwest::Client::new()
        })
}

/// Extract the bearer token from an `Authorization` header.
fn bearer_token(parts: &Parts) -> Result<&str> {
    parts
        .headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .filter(|token| !token.is_empty())
        .ok_or(Error::MissingBearerToken)
}

/// Map a verification failure to a status. A valid token whose holder lacks the
/// required group is forbidden; everything else is unauthenticated.
fn rejection_status(error: &Error) -> StatusCode {
    match error {
        Error::MissingRequiredGroup { .. } => StatusCode::FORBIDDEN,
        _ => StatusCode::UNAUTHORIZED,
    }
}

impl<S> FromRequestParts<S> for AuthenticatedUser
where
    S: Send + Sync,
    Arc<Authenticator>: axum::extract::FromRef<S>,
{
    type Rejection = Response;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> std::result::Result<Self, Response> {
        use axum::extract::FromRef;

        let authenticator = Arc::<Authenticator>::from_ref(state);

        let result = match bearer_token(parts) {
            Ok(token) => authenticator.verify(token).await,
            Err(error) => Err(error),
        };

        result.map_err(|error| {
            let status = rejection_status(&error);
            // The token itself is never logged, only why it was refused.
            tracing::warn!(error = %error, %status, "Rejecting unauthenticated upload");
            crate::metrics::record_upload(if status == StatusCode::FORBIDDEN {
                "forbidden"
            } else {
                "unauthorized"
            });
            (status, status.canonical_reason().unwrap_or("Unauthorized")).into_response()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::{HeaderValue, Request, header::AUTHORIZATION};
    use jsonwebtoken::jwk::Jwk;
    use jsonwebtoken::{EncodingKey, Header, encode};
    use serde_json::json;

    /// Throwaway 2048-bit RSA key, generated for these tests only. It is not a
    /// credential: it signs nothing outside this file and verifies nothing
    /// outside it either.
    const SIGNING_KEY_PEM: &str = concat!(
        "-----BEGIN PRIVATE KEY-----\n",
        "MIIEvgIBADANBgkqhkiG9w0BAQEFAASCBKgwggSkAgEAAoIBAQC4u00y6kgOG8GZ\n",
        "eQDgt1YHmbZToZ4B42Sy5LoH87GxbXc6BqsBl7n2v3AOCHBM+9zKpCzxuE6NqZlj\n",
        "KRsJZxa6zoOrfnvBDt9kQ/tWf2kMZxpaU3ys1POGPWMZ2CMmYU2dK0f6zSO0/fOJ\n",
        "QNhzBLlzRPNkRGaEmEtDdbp/d96/LiVzpEatIVfgDXu9NJU1u43FXT9MbYVxOOsp\n",
        "1tF9B3xUMVYnpHzjHYMHotNH4S9wjiknTU7seGx2Mo0sN8CAIxW8G2FgAlNycKzd\n",
        "cJqu6pLlMuPOw+37pD98JmuC5zss396JVSePl24YpiZUxwLLiPbxWn1zJ7fFjRQo\n",
        "ZCgnBIbtAgMBAAECggEAEdUAv3q2kGgVAOHZmBeSfiLUIxANjtyatpsWKxjWzQwG\n",
        "T4tfvARfruYtZKljX8cLOeNttDqomIun0xbfdYGmQ8uWEbqgqxLqtQTL8P5VD12v\n",
        "gUsgVJWs2Uc5NwAyamzHn3WTWe4t9XVzKgtgqX+qACrGfOYOaFvEHiOx6EaTsawS\n",
        "aNsLNYjhyNQ2AnJxOYR32YnTk99U3ncLBtIRBeploUGLcG7zEfGyUfn6i/S+CllF\n",
        "agpvPGK8hS08/wtY1ZtEtdjg+p6bxf6yDl8BB9/LlQELsGdxjBwmLo9hIqvpvvSC\n",
        "+3TcHszK4lr2ATGoRlirLt8VQccg+UrEjJ3MJyrAcQKBgQDkB1+eR2DpbF0S/55j\n",
        "sjAT4ELloV+4PBZF69zQjAlyDSyVi1P5+uB8o2YH4AAeRZFNugdgsHTWwu68GZmv\n",
        "yQO2m2z9HsVLk07zukN+0vkx8AjMQ7ywGIzD5aFAKXIWax55Hym7vhTzQqg9yQas\n",
        "7Nk3Jk3qAoWpwLRVXI7Kh/owkQKBgQDPZExYAgn8fYCaz61VTD1kQ/Lmqjz3JiVo\n",
        "Wx5hDLHcSAC/mY6jGjlFaSwhsVWE5dvQ3NW00ewtxrLRo0+i6XAwsrCvXEGY//YW\n",
        "nKS9x6f0UujaOoFM24GnkP56u29HgKUVFyOeQNDYcuA4zffGib1qXf3kPyhP0gLd\n",
        "9cCtMIfenQKBgQDL9jx02uu4XpEx+Sq3ih6u6J1twFZZ+IUDreEpONkKBvamHKXU\n",
        "p648Tftpd9cjPJ6no4oN1kfsARiBb3SkY2zK3WMzVV6sJusr3qOYwSTcohN8geo4\n",
        "qPzgDHmbZncBznbHaDRwFamvnSPXgARUkNYKGlz+v5rHJ/Mll1Cxn8cNwQKBgQDO\n",
        "2PWADcCSEUa0oY/69EiC+XaJ859MzcIfnEnnd/bpgvMkJm7qZFxcy3IVxL5MB8o/\n",
        "PhLz/y/11ClECAOEtBmOqJqqvHQ8uoZitSdmlX0BpbPS/Ok7k+90BpyaItnxUfDU\n",
        "4ThIPdNPHvxeC6gmX/kI3ug8v3Vgb1EmulbLJg1NzQKBgAtfCf+8QtDESwbHgulQ\n",
        "WRbMnaVHdwfeh+/VNwUqKJ3LEF0LpQ5mjU18t2WLr/f2ZxRQoItFypcUqaB0bfLm\n",
        "pj+3uRcQTndFjvQ+P6fgS5MRmClbPMgiJ/dWm77T5hH7+/arjZhXrpAa9luK20EK\n",
        "ZT3/ktXKTlvdd01prCQjgDOs\n",
        "-----END PRIVATE KEY-----\n",
    );
    const TEST_KID: &str = "test-key";
    const ISSUER: &str = "https://auth.example.net";

    fn signing_key() -> Result<EncodingKey> {
        Ok(EncodingKey::from_rsa_pem(SIGNING_KEY_PEM.as_bytes())?)
    }

    fn jwks_for(kid: &str) -> Result<JwkSet> {
        let mut jwk = Jwk::from_encoding_key(&signing_key()?, Algorithm::RS256)?;
        jwk.common.key_id = Some(kid.to_owned());
        Ok(JwkSet { keys: vec![jwk] })
    }

    fn test_config() -> AuthConfig {
        AuthConfig {
            issuer: ISSUER.to_owned(),
            client_id: "lolcommits-cli".to_owned(),
            required_group: "lolcommits".to_owned(),
            ..Default::default()
        }
    }

    /// A token shaped like a real Authelia device-grant token: `aud` is an
    /// empty array and there is no `azp` claim.
    fn token_with(overrides: serde_json::Value) -> Result<String> {
        let now = jsonwebtoken::get_current_timestamp() as i64;
        let mut claims = json!({
            "aud": [],
            "client_id": "lolcommits-cli",
            "exp": now + 3600,
            "groups": ["admins", "lolcommits"],
            "iat": now,
            "iss": ISSUER,
            "jti": "0f0f0f",
            "nbf": now,
            "preferred_username": "neale",
            "scp": ["openid", "profile", "groups", "offline_access"],
            "sub": "07ca04a5-8538-4ecf-8fe6-a7c5e133a4df",
        });

        let Some(claims_map) = claims.as_object_mut() else {
            panic!("claims template must be an object");
        };
        for (key, value) in overrides.as_object().into_iter().flatten() {
            claims_map.insert(key.clone(), value.clone());
        }

        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(TEST_KID.to_owned());
        Ok(encode(&header, &claims, &signing_key()?)?)
    }

    async fn verify_token(token: &str) -> Result<AuthenticatedUser> {
        Authenticator::with_keys(test_config(), jwks_for(TEST_KID)?)
            .verify(token)
            .await
    }

    #[tokio::test]
    async fn accepts_a_well_formed_token() -> Result<()> {
        let user = verify_token(&token_with(json!({}))?).await?;
        assert_eq!(user.subject, "07ca04a5-8538-4ecf-8fe6-a7c5e133a4df");
        assert_eq!(user.username.as_deref(), Some("neale"));
        Ok(())
    }

    #[tokio::test]
    async fn rejects_a_token_from_another_client() -> Result<()> {
        let token = token_with(json!({ "client_id": "sw1nn-pkg-cli" }))?;
        assert!(matches!(
            verify_token(&token).await,
            Err(Error::WrongClientId)
        ));
        Ok(())
    }

    #[tokio::test]
    async fn rejects_a_token_from_another_issuer() -> Result<()> {
        let token = token_with(json!({ "iss": "https://evil.example.net" }))?;
        assert!(verify_token(&token).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn rejects_an_expired_token() -> Result<()> {
        let now = jsonwebtoken::get_current_timestamp() as i64;
        let token = token_with(json!({ "exp": now - CLOCK_LEEWAY_SECS as i64 - 10 }))?;
        assert!(verify_token(&token).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn rejects_a_token_that_is_not_yet_valid() -> Result<()> {
        let now = jsonwebtoken::get_current_timestamp() as i64;
        let token = token_with(json!({ "nbf": now + CLOCK_LEEWAY_SECS as i64 + 60 }))?;
        assert!(verify_token(&token).await.is_err());
        Ok(())
    }

    #[tokio::test]
    async fn rejects_a_caller_without_the_required_group() -> Result<()> {
        let token = token_with(json!({ "groups": ["pkg-publish"] }))?;
        assert!(matches!(
            verify_token(&token).await,
            Err(Error::MissingRequiredGroup { .. })
        ));
        Ok(())
    }

    /// `admins` must not act as a blanket grant.
    #[tokio::test]
    async fn rejects_an_admin_without_the_required_group() -> Result<()> {
        let token = token_with(json!({ "groups": ["admins", "pkg-admin"] }))?;
        assert!(matches!(
            verify_token(&token).await,
            Err(Error::MissingRequiredGroup { .. })
        ));
        Ok(())
    }

    #[tokio::test]
    async fn rejects_a_token_signed_by_an_unknown_key() -> Result<()> {
        let authenticator = Authenticator::with_keys(test_config(), jwks_for("some-other-kid")?);
        let result = authenticator.verify(&token_with(json!({}))?).await;
        assert!(matches!(result, Err(Error::UnknownSigningKey)));
        Ok(())
    }

    #[tokio::test]
    async fn rejects_a_tampered_token() -> Result<()> {
        let token = token_with(json!({}))?;
        let mut parts: Vec<&str> = token.split('.').collect();
        let forged = token_with(json!({ "groups": ["lolcommits"], "sub": "someone-else" }))?;
        let forged_payload = forged.split('.').nth(1).unwrap_or_default();
        parts[1] = forged_payload;
        assert!(verify_token(&parts.join(".")).await.is_err());
        Ok(())
    }

    /// An unsigned `alg: none` token must never be accepted.
    #[tokio::test]
    async fn rejects_an_unsigned_token() -> Result<()> {
        use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};

        let now = jsonwebtoken::get_current_timestamp() as i64;
        let header = URL_SAFE_NO_PAD
            .encode(json!({ "alg": "none", "typ": "at+jwt", "kid": TEST_KID }).to_string());
        let payload = URL_SAFE_NO_PAD.encode(
            json!({
                "client_id": "lolcommits-cli",
                "exp": now + 3600,
                "groups": ["lolcommits"],
                "iss": ISSUER,
                "nbf": now,
                "sub": "attacker",
            })
            .to_string(),
        );

        assert!(verify_token(&format!("{header}.{payload}.")).await.is_err());
        Ok(())
    }

    /// Signing with HS256 using the RSA public key as the HMAC secret is the
    /// classic algorithm-substitution attack. The RS256-only allow-list stops
    /// it before the key is ever consulted.
    #[tokio::test]
    async fn rejects_an_algorithm_substitution() -> Result<()> {
        let now = jsonwebtoken::get_current_timestamp() as i64;
        let claims = json!({
            "client_id": "lolcommits-cli",
            "exp": now + 3600,
            "groups": ["lolcommits"],
            "iss": ISSUER,
            "nbf": now,
            "sub": "attacker",
        });

        let mut header = Header::new(Algorithm::HS256);
        header.kid = Some(TEST_KID.to_owned());
        let token = encode(&header, &claims, &EncodingKey::from_secret(b"whatever"))?;

        assert!(verify_token(&token).await.is_err());
        Ok(())
    }

    /// The refetch window is taken before the network call, so concurrent
    /// unknown-`kid` requests cannot each trigger their own fetch.
    #[test]
    fn refetch_window_admits_one_attempt_at_a_time() -> Result<()> {
        let authenticator = Authenticator {
            config: test_config(),
            http: jwks_client(),
            jwks: RwLock::new(Arc::new(JwkSet { keys: Vec::new() })),
            last_fetch: Mutex::new(None),
        };

        assert!(
            authenticator.refetch_allowed(),
            "the first attempt proceeds"
        );
        assert!(
            !authenticator.refetch_allowed(),
            "a second attempt inside the window is refused"
        );

        Ok(())
    }

    #[tokio::test]
    async fn rejects_a_token_without_a_key_id() -> Result<()> {
        let claims = json!({ "sub": "x", "exp": jsonwebtoken::get_current_timestamp() + 60 });
        let token = encode(&Header::new(Algorithm::RS256), &claims, &signing_key()?)?;
        assert!(matches!(
            verify_token(&token).await,
            Err(Error::TokenMissingKeyId)
        ));
        Ok(())
    }

    fn parts_with_auth(value: Option<&'static str>) -> Parts {
        let mut request = Request::new(());
        if let Some(value) = value {
            request
                .headers_mut()
                .insert(AUTHORIZATION, HeaderValue::from_static(value));
        }
        request.into_parts().0
    }

    #[test]
    fn bearer_token_extracts_the_credential() -> Result<()> {
        assert_eq!(
            bearer_token(&parts_with_auth(Some("Bearer abc.def.ghi")))?,
            "abc.def.ghi"
        );
        Ok(())
    }

    #[test]
    fn bearer_token_rejects_missing_or_malformed_headers() {
        for header in [
            None,
            Some("abc.def.ghi"),
            Some("Basic abc"),
            Some("Bearer "),
        ] {
            assert!(
                matches!(
                    bearer_token(&parts_with_auth(header)),
                    Err(Error::MissingBearerToken)
                ),
                "expected {header:?} to be rejected"
            );
        }
    }

    #[test]
    fn missing_group_is_forbidden_and_everything_else_unauthorized() {
        assert_eq!(
            rejection_status(&Error::MissingRequiredGroup {
                group: "lolcommits".to_owned()
            }),
            StatusCode::FORBIDDEN
        );
        for error in [
            Error::MissingBearerToken,
            Error::WrongClientId,
            Error::UnknownSigningKey,
            Error::TokenMissingKeyId,
        ] {
            assert_eq!(rejection_status(&error), StatusCode::UNAUTHORIZED);
        }
    }

    #[test]
    fn display_name_prefers_the_username() {
        let named = AuthenticatedUser {
            subject: "07ca04a5".to_owned(),
            username: Some("neale".to_owned()),
        };
        assert_eq!(named.display_name(), "neale");

        let anonymous = AuthenticatedUser {
            subject: "07ca04a5".to_owned(),
            username: None,
        };
        assert_eq!(anonymous.display_name(), "07ca04a5");
    }
}
