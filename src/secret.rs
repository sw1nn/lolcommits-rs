use serde::{Deserialize, Serialize};

/// A string whose `Debug` output never reveals the value, so secrets held in
/// config are not leaked through `tracing::debug!(?config)` or error output.
///
/// (De)serialization is transparent, so a `Secret` reads and writes as a plain
/// string in config files.
#[derive(Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new<S>(value: S) -> Self
    where
        S: Into<String>,
    {
        Self(value.into())
    }

    /// Return the underlying secret. Call this only where the value is actually
    /// needed (e.g. a constant-time comparison), never for logging.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, fmt: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fmt.write_str("\"[redacted]\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_output_is_redacted() {
        let secret = Secret::new("hunter2");
        assert_eq!(format!("{secret:?}"), "\"[redacted]\"");
        assert!(!format!("{secret:?}").contains("hunter2"));
    }

    #[test]
    fn expose_returns_value() {
        assert_eq!(Secret::new("hunter2").expose(), "hunter2");
    }
}
