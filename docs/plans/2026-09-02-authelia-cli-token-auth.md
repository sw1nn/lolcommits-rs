# Authelia CLI token auth — implementation plan

Replaces the static shared bearer tokens on `POST /api/upload` with Authelia
OIDC device-flow tokens. Source of truth for the contract:
`authelia-cli-token-auth-integration.md` (infrastructure repo).

## Contract summary

| Item | Value |
|---|---|
| Issuer | `https://auth.sw1nn.net` |
| JWKS | `{issuer}/jwks.json` (RS256, `kid: main`) |
| Device authorization | `{issuer}/api/oidc/device-authorization` |
| Token | `{issuer}/api/oidc/token` |
| `client_id` | `lolcommits-cli` (public client, no secret) |
| Scopes | `openid profile groups offline_access` |
| Required group | `lolcommits` |
| Lifespans | access 1h, refresh 720h, device code 10m |

Verification order on the server: signature (RS256 via JWKS `kid`) → `iss` →
`client_id` → `exp`/`nbf` (60s leeway) → `groups`.

> [!IMPORTANT]
> `aud` is empty on every Authelia device-grant token, so the replay boundary is
> `client_id`, not `aud`. `scp` is never an authorization boundary, and `admins`
> is not a blanket grant.

## Configuration

A new shared `[auth]` section, read by both the daemon and the CLI. Defaults
match the live deployment, so existing hosts need no new config beyond running
`lolcommits-ctl login`.

```toml
[auth]
issuer = "https://auth.sw1nn.net"
client_id = "lolcommits-cli"
required_group = "lolcommits"           # server only
# optional endpoint overrides, derived from `issuer` when unset
# jwks_url = "https://auth.sw1nn.net/jwks.json"
# device_authorization_url = "https://auth.sw1nn.net/api/oidc/device-authorization"
# token_url = "https://auth.sw1nn.net/api/oidc/token"
# scopes = ["openid", "profile", "groups", "offline_access"]
```

## Work items

### 1. Foundation (shared)

- `Cargo.toml`: add `jsonwebtoken` (`rust_crypto`), `keyring`, `reqwest/json`;
  drop `subtle`.
- `config.rs`: add `AuthConfig` with the effective-URL accessors. Delete
  `ClientConfig.upload_token` and `ServerConfig.upload_tokens`.
- `error.rs`: add the JWT, JWKS, device-flow, token-store and authorization
  variants. Delete `ServerBindWithoutAuth`.
- `lib.rs`: register the new modules.

### 2. Server

- `auth.rs`: `Authenticator` holding a cached `JwkSet`. Refetch only on an
  unknown `kid`, rate-limited to one attempt per 60s so a caller sending
  garbage `kid` values cannot make the daemon hammer Authelia. Serving stale
  keys is safe — rotation is manual and rare.
- Axum `FromRequestParts` extractor yielding `AuthenticatedUser { sub,
  preferred_username }`, applied to `POST /api/upload` only. The gallery and
  static assets stay unauthenticated.
- `401` for missing/malformed/invalid bearer, `403` for a valid token without
  the required group. Log `sub` and `preferred_username`; never token contents.
- Delete `upload_authorized`, `ensure_bind_is_authorized`, `is_loopback_bind`,
  `load_credential_tokens` and their tests.

### 3. Client

- `oidc.rs`: RFC 8628 device flow (blocking), honouring `interval`,
  `authorization_pending` and `slow_down`; plus the refresh-token grant.
- `token_store.rs`: Secret Service via `keyring`, with a `0600` file fallback
  under `$XDG_STATE_HOME/lolcommits/tokens.json` for headless hosts.
- `lolcommits-ctl login` / `logout`.
- `capture.rs`: attach the access token, refresh when expired or on a `401`,
  and retry the upload exactly once. No background refresh daemons.
- On refresh failure, exit non-zero with:

  ```
  error: not logged in or session expired
         run: lolcommits-ctl login
  ```

### 4. Documentation

- README: replace the upload-token section; document `login`/`logout` and the
  `[auth]` section; drop the non-loopback-bind warning and the forward-auth
  guidance (bearer validation belongs in the app, and the gallery stays open).

## Out of scope

- Gating the gallery, or splitting its vhost.
- Caddy `forward_auth` in front of these vhosts.
- Per-request introspection or userinfo calls.
