//! Persistence for the CLI's OIDC credentials.
//!
//! The Secret Service (KeepassXC, GNOME Keyring, ...) is the primary store. It
//! is unavailable on headless hosts and over plain SSH, where there is no D-Bus
//! session, so a `0600` file under `$XDG_STATE_HOME` is the fallback.

use crate::config::AuthConfig;
use crate::error::{Error, Result};
use crate::oidc::TokenSet;
use std::fs::{File, Permissions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use xdg::BaseDirectories;

const KEYRING_SERVICE: &str = "lolcommits";
const XDG_PREFIX: &str = "lolcommits";
const TOKEN_FILE_NAME: &str = "tokens.json";
const TOKEN_FILE_MODE: u32 = 0o600;

/// Where a [`TokenSet`] ended up being stored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreBackend {
    SecretService,
    File(PathBuf),
}

impl std::fmt::Display for StoreBackend {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SecretService => fmt.write_str("the Secret Service"),
            Self::File(path) => write!(fmt, "{}", path.display()),
        }
    }
}

/// Load stored credentials, or `Ok(None)` when there are none to load.
///
/// Credentials that will not parse are treated as absent rather than fatal:
/// the caller's next step is a login, which overwrites them anyway.
pub fn load(auth: &AuthConfig) -> Result<Option<TokenSet>> {
    match entry(auth).and_then(|entry| entry.get_password()) {
        Ok(json) => return Ok(parse(&json)),
        Err(keyring::Error::NoEntry) => {
            tracing::debug!("No credentials in the Secret Service, trying the file store");
        }
        Err(error) => {
            tracing::debug!(%error, "Secret Service unavailable, trying the file store");
        }
    }

    match token_file_path() {
        Some(path) => load_file(&path),
        None => Ok(None),
    }
}

/// Store credentials, returning the backend that accepted them.
pub fn save(auth: &AuthConfig, tokens: &TokenSet) -> Result<StoreBackend> {
    let json = serde_json::to_string(tokens)?;

    match entry(auth).and_then(|entry| entry.set_password(&json)) {
        Ok(()) => {
            // A file written on an earlier run, when the Secret Service was
            // unavailable, would otherwise sit on disk holding a refresh token
            // that nothing reads any more.
            if let Some(path) = token_file_path() {
                let _ = remove_file(&path);
            }
            Ok(StoreBackend::SecretService)
        }
        Err(error) => {
            tracing::warn!(%error, "Secret Service unavailable, falling back to file");
            let path = token_file_path().ok_or(Error::NoHomeDirectory)?;
            save_file(&path, &json)?;
            Ok(StoreBackend::File(path))
        }
    }
}

/// Forget stored credentials in both backends.
///
/// Nothing to delete is success. A store that refused the delete is not: the
/// caller asked for the credentials to be gone, and reporting success while a
/// refresh token is still live in a locked keyring is the wrong answer to
/// "I am handing this laptop back".
pub fn clear(auth: &AuthConfig) -> Result<()> {
    match entry(auth).and_then(|entry| entry.delete_credential()) {
        Ok(()) => {}
        Err(error) if nothing_to_delete(&error) => {
            tracing::debug!(%error, "No Secret Service credentials to clear");
        }
        Err(error) => return Err(error.into()),
    }

    if let Some(path) = token_file_path() {
        remove_file(&path)?;
    }

    Ok(())
}

/// Whether a keyring failure means "there was nothing stored here" rather than
/// "the delete failed". A host with no Secret Service at all never held the
/// credentials; a locked or broken one may still.
fn nothing_to_delete(error: &keyring::Error) -> bool {
    matches!(
        error,
        keyring::Error::NoEntry
            | keyring::Error::NoDefaultStore
            | keyring::Error::NotSupportedByStore(_)
    )
}

/// Absolute path of the fallback token file, or `None` without a home directory.
pub fn token_file_path() -> Option<PathBuf> {
    BaseDirectories::with_prefix(XDG_PREFIX).get_state_file(TOKEN_FILE_NAME)
}

fn entry(auth: &AuthConfig) -> keyring::Result<keyring::Entry> {
    keyring::Entry::new(KEYRING_SERVICE, &auth.client_id)
}

fn load_file(path: &Path) -> Result<Option<TokenSet>> {
    match std::fs::read_to_string(path) {
        Ok(contents) => Ok(parse(&contents)),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn parse(json: &str) -> Option<TokenSet> {
    serde_json::from_str(json)
        .inspect_err(|error| {
            tracing::warn!(%error, "Discarding unreadable stored credentials");
        })
        .ok()
}

fn save_file(path: &Path, json: &str) -> Result<()> {
    write_atomically(path, json).map_err(|source| Error::TokenStoreWrite {
        path: path.to_path_buf(),
        source,
    })
}

fn remove_file(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::NotFound => Ok(()),
        Err(source) => Err(Error::TokenStoreWrite {
            path: path.to_path_buf(),
            source,
        }),
    }
}

/// Write via a same-directory temp file so a crash cannot leave a truncated
/// token file, and so the content is never briefly world-readable.
///
/// `NamedTempFile` picks an unpredictable name and opens it exclusively, so
/// nothing can pre-create a symlink at the path we are about to write a
/// refresh token to.
fn write_atomically(path: &Path, contents: &str) -> std::io::Result<()> {
    let directory = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(directory)?;

    let mut temp_file = tempfile::NamedTempFile::new_in(directory)?;

    // NamedTempFile already creates at 0600; set it again so the guarantee this
    // file depends on is stated here rather than inherited silently.
    temp_file
        .as_file()
        .set_permissions(Permissions::from_mode(TOKEN_FILE_MODE))?;
    temp_file.write_all(contents.as_bytes())?;
    temp_file.as_file().sync_all()?;

    temp_file
        .persist(path)
        .map_err(|error| std::io::Error::other(error.to_string()))?;

    sync_directory(directory)
}

fn sync_directory(directory: &Path) -> std::io::Result<()> {
    File::open(directory)?.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secret::Secret;

    fn sample_tokens() -> TokenSet {
        TokenSet {
            access_token: Secret::new("access"),
            refresh_token: Some(Secret::new("refresh")),
            expires_at: 1_000,
        }
    }

    #[test]
    fn load_file_returns_none_when_absent() -> Result<()> {
        let directory = tempfile::tempdir()?;
        assert!(load_file(&directory.path().join("tokens.json"))?.is_none());
        Ok(())
    }

    #[test]
    fn save_then_load_round_trips() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join("nested").join(TOKEN_FILE_NAME);

        save_file(&path, &serde_json::to_string(&sample_tokens())?)?;
        let loaded = load_file(&path)?.ok_or(Error::NotLoggedIn)?;

        assert_eq!(loaded.access_token.expose(), "access");
        assert_eq!(
            loaded.refresh_token.as_ref().map(Secret::expose),
            Some("refresh")
        );
        assert_eq!(loaded.expires_at, 1_000);
        Ok(())
    }

    #[test]
    fn saved_file_is_owner_only() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join(TOKEN_FILE_NAME);

        save_file(&path, "{}")?;

        let mode = std::fs::metadata(&path)?.permissions().mode() & 0o777;
        assert_eq!(mode, TOKEN_FILE_MODE);
        Ok(())
    }

    #[test]
    fn save_overwrites_an_existing_file() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join(TOKEN_FILE_NAME);

        save_file(&path, &serde_json::to_string(&sample_tokens())?)?;
        let replacement = TokenSet {
            expires_at: 2_000,
            ..sample_tokens()
        };
        save_file(&path, &serde_json::to_string(&replacement)?)?;

        let loaded = load_file(&path)?.ok_or(Error::NotLoggedIn)?;
        assert_eq!(loaded.expires_at, 2_000);
        Ok(())
    }

    #[test]
    fn remove_file_is_idempotent() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join(TOKEN_FILE_NAME);

        save_file(&path, "{}")?;
        remove_file(&path)?;
        remove_file(&path)?;

        assert!(!path.exists());
        Ok(())
    }

    #[test]
    fn unreadable_credentials_are_treated_as_absent() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let path = directory.path().join(TOKEN_FILE_NAME);

        save_file(&path, "not json at all")?;

        assert!(load_file(&path)?.is_none());
        Ok(())
    }

    #[test]
    fn backend_display_names_the_location() {
        assert_eq!(
            StoreBackend::SecretService.to_string(),
            "the Secret Service"
        );
        assert_eq!(
            StoreBackend::File(PathBuf::from("/tmp/tokens.json")).to_string(),
            "/tmp/tokens.json"
        );
    }
}
