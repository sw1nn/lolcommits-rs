use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use xdg::BaseDirectories;

/// XDG prefix for lolcommits configuration.
const XDG_PREFIX: &str = "lolcommits";

/// Default configuration file name within the config directory.
const CONFIG_FILE_NAME: &str = "config.toml";

/// Where the packaged gallery assets are installed.
pub const DEFAULT_STATIC_DIR: &str = "/usr/share/lolcommits/static";

/// Overrides `server.static_dir`, so a development run can serve the in-tree
/// assets without editing the installed config.
pub const STATIC_ROOT_ENV: &str = "LOLCOMMITS_STATIC_ROOT";

/// Configuration for a single camera device.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraDeviceConfig {
    /// Device path or index (e.g., "/dev/video0", "0", "/dev/video-ugreen")
    pub device: String,

    /// Camera pixel format: "YUYV", "MJPEG", "NV12", "GRAY". If not set, auto-detects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,

    /// Camera capture width in pixels. If not set, auto-detects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub width: Option<u32>,

    /// Camera capture height in pixels. If not set, auto-detects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub height: Option<u32>,

    /// Camera frame rate. If not set, auto-detects.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fps: Option<u32>,
}

impl CameraDeviceConfig {
    /// Create a new camera device config with just the device path.
    pub fn new<S>(device: S) -> Self
    where
        S: Into<String>,
    {
        Self {
            device: device.into(),
            format: None,
            width: None,
            height: None,
            fps: None,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client: Option<ClientConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<ServerConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub burned_in_chyron: Option<BurnedInChyronConfig>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth: Option<AuthConfig>,
}

/// OpenID Connect settings shared by the daemon, which verifies access tokens,
/// and the CLI, which obtains them through the device authorization grant.
///
/// The client is public (no client secret), so nothing here is sensitive.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Token issuer. Must match the `iss` claim exactly, and is the base the
    /// endpoint URLs below are derived from when they are not set explicitly.
    #[serde(default = "default_auth_issuer")]
    pub issuer: String,

    /// This service's own OIDC client id. Tokens minted for any other client
    /// are rejected: `aud` is empty on Authelia's device grant, so `client_id`
    /// is what stops a token for another service being replayed here.
    #[serde(default = "default_auth_client_id")]
    pub client_id: String,

    /// Group a caller must hold in `groups` to upload. Used by the daemon only.
    #[serde(default = "default_auth_required_group")]
    pub required_group: String,

    /// Scopes requested at login. These are not an authorization boundary — a
    /// public client can ask for anything — so the daemon gates on groups.
    #[serde(default = "default_auth_scopes")]
    pub scopes: Vec<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jwks_url: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub device_authorization_url: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_url: Option<String>,
}

impl AuthConfig {
    /// URL of the issuer's JSON Web Key Set.
    pub fn jwks_url(&self) -> String {
        self.endpoint(self.jwks_url.as_deref(), "jwks.json")
    }

    /// RFC 8628 device authorization endpoint.
    pub fn device_authorization_url(&self) -> String {
        self.endpoint(
            self.device_authorization_url.as_deref(),
            "api/oidc/device-authorization",
        )
    }

    /// Token endpoint, used for both the device code and refresh grants.
    pub fn token_url(&self) -> String {
        self.endpoint(self.token_url.as_deref(), "api/oidc/token")
    }

    /// Requested scopes in the space-delimited form the token endpoint wants.
    pub fn scope(&self) -> String {
        self.scopes.join(" ")
    }

    fn endpoint(&self, configured: Option<&str>, default_path: &str) -> String {
        configured
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{}/{default_path}", self.issuer.trim_end_matches('/')))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BurnedInChyronConfig {
    #[serde(default = "default_font_name")]
    pub default_font_name: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_font_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub info_font_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub sha_font_name: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub stats_font_name: Option<String>,

    #[serde(default = "default_chyron_opacity")]
    pub chyron_opacity: f32,

    #[serde(default = "default_title_font_size")]
    pub title_font_size: f32,

    #[serde(default = "default_info_font_size")]
    pub info_font_size: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "ClientConfigRepr")]
pub struct ClientConfig {
    /// List of camera devices to try in order. First working camera is used.
    /// Each camera can have its own format/resolution settings.
    pub camera_devices: Vec<CameraDeviceConfig>,

    pub camera_warmup_frames: usize,

    pub server_url: String,

    pub server_upload_timeout_secs: u64,
}

/// Deserialization shim for [`ClientConfig`] that also accepts the legacy
/// singular `camera_device = "..."` key (documented in older configs) and
/// maps it to a single-element `camera_devices`.
#[derive(Deserialize)]
struct ClientConfigRepr {
    #[serde(default)]
    camera_devices: Option<Vec<CameraDeviceConfig>>,

    #[serde(default)]
    camera_device: Option<String>,

    #[serde(default = "default_camera_warmup_frames")]
    camera_warmup_frames: usize,

    #[serde(default = "default_server_url")]
    server_url: String,

    #[serde(default = "default_server_upload_timeout_secs")]
    server_upload_timeout_secs: u64,
}

impl From<ClientConfigRepr> for ClientConfig {
    fn from(repr: ClientConfigRepr) -> Self {
        // Prefer the modern `camera_devices` list; fall back to the legacy
        // singular `camera_device`; otherwise use the default device.
        let camera_devices = repr.camera_devices.unwrap_or_else(|| {
            repr.camera_device
                .map(|device| vec![CameraDeviceConfig::new(device)])
                .unwrap_or_else(default_camera_devices)
        });

        Self {
            camera_devices,
            camera_warmup_frames: repr.camera_warmup_frames,
            server_url: repr.server_url,
            server_upload_timeout_secs: repr.server_upload_timeout_secs,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_background_path")]
    pub background_path: String,

    #[serde(default = "default_center_person")]
    pub center_person: bool,

    #[serde(default = "default_gallery_title")]
    pub gallery_title: String,

    #[serde(default = "default_images_dir")]
    pub images_dir: String,

    #[serde(default = "default_models_dir")]
    pub models_dir: String,

    /// Directory holding the gallery's `index.html` and its `/static/` assets.
    #[serde(default = "default_static_dir")]
    pub static_dir: String,

    #[serde(default = "default_bind_address")]
    pub bind_address: String,

    #[serde(default = "default_bind_port")]
    pub bind_port: u16,

    #[serde(default)]
    pub log_output: crate::LogOutput,

    #[serde(default = "default_burned_in_chyron")]
    pub burned_in_chyron: bool,

    /// Maximum number of uploads processed concurrently. Further uploads are
    /// shed with 429 while all slots are in use. Bounds the CPU/memory cost of
    /// image decode plus ONNX segmentation. Must be at least 1.
    #[serde(default = "default_max_concurrent_uploads")]
    pub max_concurrent_uploads: usize,
}

impl ServerConfig {
    /// Directory the gallery's static assets are read from.
    ///
    /// `LOLCOMMITS_STATIC_ROOT` wins over `static_dir`; an empty value counts
    /// as unset, so exporting it blank does not silently break the gallery.
    pub fn static_root(&self) -> PathBuf {
        std::env::var_os(STATIC_ROOT_ENV)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(&self.static_dir))
    }
}

fn default_font_name() -> String {
    "monospace".to_string()
}

fn default_auth_issuer() -> String {
    "https://auth.sw1nn.net".to_owned()
}

fn default_auth_client_id() -> String {
    "lolcommits-cli".to_owned()
}

fn default_auth_required_group() -> String {
    "lolcommits".to_owned()
}

fn default_auth_scopes() -> Vec<String> {
    ["openid", "profile", "groups", "offline_access"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

fn default_background_path() -> String {
    // get_data_home() is None when neither $XDG_DATA_HOME nor $HOME is set
    // (e.g. a HOME-less systemd/container daemon). Fall back to the system data
    // dir rather than panicking inside a serde default / Default impl.
    BaseDirectories::with_prefix(XDG_PREFIX)
        .get_data_home()
        .unwrap_or_else(|| PathBuf::from("/var/lib/lolcommits"))
        .join("background.png")
        .to_string_lossy()
        .to_string()
}

fn default_camera_devices() -> Vec<CameraDeviceConfig> {
    vec![CameraDeviceConfig::new("0")]
}

fn default_camera_warmup_frames() -> usize {
    3
}

fn default_chyron_opacity() -> f32 {
    0.75
}

fn default_title_font_size() -> f32 {
    28.0
}

fn default_info_font_size() -> f32 {
    18.0
}

fn default_center_person() -> bool {
    true
}

fn default_burned_in_chyron() -> bool {
    true
}

fn default_gallery_title() -> String {
    "Lolcommits Gallery".to_string()
}

fn default_server_url() -> String {
    "http://127.0.0.1:3000".to_string()
}

fn default_server_upload_timeout_secs() -> u64 {
    30
}

fn default_images_dir() -> String {
    "/var/lib/lolcommits/images".to_string()
}

fn default_models_dir() -> String {
    "/var/lib/lolcommits/models".to_string()
}

fn default_static_dir() -> String {
    DEFAULT_STATIC_DIR.to_owned()
}

fn default_bind_address() -> String {
    // Loopback by default: the daemon is expected to sit behind a reverse proxy.
    // Uploads are authenticated in the application, so a non-loopback bind is
    // not itself a risk.
    "127.0.0.1".to_string()
}

fn default_bind_port() -> u16 {
    3000
}

fn default_max_concurrent_uploads() -> usize {
    4
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            issuer: default_auth_issuer(),
            client_id: default_auth_client_id(),
            required_group: default_auth_required_group(),
            scopes: default_auth_scopes(),
            jwks_url: None,
            device_authorization_url: None,
            token_url: None,
        }
    }
}

impl Default for BurnedInChyronConfig {
    fn default() -> Self {
        Self {
            default_font_name: default_font_name(),
            message_font_name: None,
            info_font_name: None,
            sha_font_name: None,
            stats_font_name: None,
            chyron_opacity: default_chyron_opacity(),
            title_font_size: default_title_font_size(),
            info_font_size: default_info_font_size(),
        }
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            camera_devices: default_camera_devices(),
            camera_warmup_frames: default_camera_warmup_frames(),
            server_url: default_server_url(),
            server_upload_timeout_secs: default_server_upload_timeout_secs(),
        }
    }
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            background_path: default_background_path(),
            center_person: default_center_person(),
            gallery_title: default_gallery_title(),
            images_dir: default_images_dir(),
            models_dir: default_models_dir(),
            static_dir: default_static_dir(),
            bind_address: default_bind_address(),
            bind_port: default_bind_port(),
            log_output: crate::LogOutput::default(),
            burned_in_chyron: default_burned_in_chyron(),
            max_concurrent_uploads: default_max_concurrent_uploads(),
        }
    }
}

impl BurnedInChyronConfig {
    /// Get the font name for messages, falling back to default_font_name
    pub fn get_message_font_name(&self) -> &str {
        self.message_font_name
            .as_deref()
            .unwrap_or(&self.default_font_name)
    }

    /// Get the font name for info, falling back to default_font_name
    pub fn get_info_font_name(&self) -> &str {
        self.info_font_name
            .as_deref()
            .unwrap_or(&self.default_font_name)
    }

    /// Get the font name for SHA, falling back to default_font_name
    pub fn get_sha_font_name(&self) -> &str {
        self.sha_font_name
            .as_deref()
            .unwrap_or(&self.default_font_name)
    }

    /// Get the font name for stats, falling back to default_font_name
    pub fn get_stats_font_name(&self) -> &str {
        self.stats_font_name
            .as_deref()
            .unwrap_or(&self.default_font_name)
    }
}

impl Config {
    /// Load configuration from the specified path, or search in hierarchical order:
    /// 1. /etc/sw1nn/lolcommits/config.toml (system-wide)
    /// 2. XDG_CONFIG_HOME/lolcommits/config.toml (user-specific)
    pub fn load_from(config_path: Option<PathBuf>) -> Result<Self> {
        let explicit = config_path.is_some();
        let config_path = if let Some(path) = config_path {
            // Use explicit path if provided
            path
        } else {
            // Search in hierarchical order
            let system_config = PathBuf::from("/etc/sw1nn/lolcommits/config.toml");

            if system_config.exists() {
                tracing::debug!(path = %system_config.display(), "Using system config");
                system_config
            } else {
                // Fall back to user config
                let user_config =
                    BaseDirectories::with_prefix(XDG_PREFIX).place_config_file(CONFIG_FILE_NAME)?;
                tracing::debug!(path = %user_config.display(), "Using user config");
                user_config
            }
        };

        if !config_path.exists() {
            // An explicitly requested path that does not exist is a user error:
            // fail instead of silently running defaults and clobbering the XDG
            // config on the next save().
            if explicit {
                return Err(Error::ConfigFileNotFound { path: config_path });
            }
            tracing::info!(path = %config_path.display(), "Config file not found, creating default");
            let default_config = Config::default();
            default_config.save()?;
            return Ok(default_config);
        }

        tracing::debug!(path = %config_path.display(), "Loading config");
        let contents =
            std::fs::read_to_string(&config_path).map_err(|source| Error::ConfigFileRead {
                path: config_path.clone(),
                source,
            })?;

        let config: Config = toml::from_str(&contents)?;

        tracing::debug!(?config, "Config loaded successfully");
        Ok(config)
    }

    /// Load configuration using hierarchical search
    pub fn load() -> Result<Self> {
        Self::load_from(None)
    }

    /// Save configuration to XDG_CONFIG_HOME/lolcommits/config.toml
    pub fn save(&self) -> Result {
        let config_path =
            BaseDirectories::with_prefix(XDG_PREFIX).place_config_file(CONFIG_FILE_NAME)?;

        let contents = toml::to_string_pretty(self)?;

        std::fs::write(&config_path, contents).map_err(|source| Error::ConfigFileWrite {
            path: config_path.clone(),
            source,
        })?;

        tracing::info!(path = %config_path.display(), "Config saved successfully");
        Ok(())
    }

    /// Get the path to the config file
    pub fn config_path() -> PathBuf {
        // Fall back to the system config dir when XDG dirs are unavailable
        // (no $XDG_CONFIG_HOME and no $HOME) instead of panicking.
        BaseDirectories::with_prefix(XDG_PREFIX)
            .get_config_home()
            .unwrap_or_else(|| PathBuf::from("/etc/sw1nn/lolcommits"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        // All sections are None by default
        assert!(config.client.is_none());
        assert!(config.server.is_none());
        assert!(config.burned_in_chyron.is_none());
    }

    #[test]
    fn test_default_burned_in_chyron_config() {
        let chyron = BurnedInChyronConfig::default();
        assert_eq!(chyron.chyron_opacity, 0.75);
    }

    #[test]
    fn test_default_client_config() {
        let client = ClientConfig::default();
        assert_eq!(client.camera_devices.len(), 1);
        assert_eq!(client.camera_devices[0].device, "0");
        assert_eq!(client.camera_warmup_frames, 3);
    }

    #[test]
    fn test_default_server_config() {
        let server = ServerConfig::default();
        assert!(server.center_person);
        assert!(server.burned_in_chyron);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config {
            client: Some(ClientConfig::default()),
            server: Some(ServerConfig::default()),
            ..Default::default()
        };
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(
            config.client.as_ref().unwrap().camera_devices.len(),
            parsed.client.as_ref().unwrap().camera_devices.len()
        );
        assert_eq!(
            config.client.as_ref().unwrap().camera_devices[0].device,
            parsed.client.as_ref().unwrap().camera_devices[0].device
        );
    }

    #[test]
    fn test_font_fallback_all_none() {
        let chyron = BurnedInChyronConfig {
            default_font_name: "DejaVu Sans".to_string(),
            message_font_name: None,
            info_font_name: None,
            sha_font_name: None,
            stats_font_name: None,
            ..Default::default()
        };

        assert_eq!(chyron.get_message_font_name(), "DejaVu Sans");
        assert_eq!(chyron.get_info_font_name(), "DejaVu Sans");
        assert_eq!(chyron.get_sha_font_name(), "DejaVu Sans");
        assert_eq!(chyron.get_stats_font_name(), "DejaVu Sans");
    }

    #[test]
    fn test_font_fallback_mixed() {
        let chyron = BurnedInChyronConfig {
            default_font_name: "monospace".to_string(),
            message_font_name: Some("Arial".to_string()),
            info_font_name: None,
            sha_font_name: Some("Courier New".to_string()),
            stats_font_name: None,
            ..Default::default()
        };

        assert_eq!(chyron.get_message_font_name(), "Arial");
        assert_eq!(chyron.get_info_font_name(), "monospace");
        assert_eq!(chyron.get_sha_font_name(), "Courier New");
        assert_eq!(chyron.get_stats_font_name(), "monospace");
    }

    #[test]
    fn test_default_font_name_is_monospace() {
        let chyron = BurnedInChyronConfig::default();
        assert_eq!(chyron.default_font_name, "monospace");
        assert_eq!(chyron.get_message_font_name(), "monospace");
        assert_eq!(chyron.get_info_font_name(), "monospace");
        assert_eq!(chyron.get_sha_font_name(), "monospace");
        assert_eq!(chyron.get_stats_font_name(), "monospace");
    }

    #[test]
    fn test_font_serialization_omits_none() {
        let config = Config {
            burned_in_chyron: Some(BurnedInChyronConfig {
                default_font_name: "monospace".to_string(),
                message_font_name: Some("Arial".to_string()),
                info_font_name: None,
                sha_font_name: None,
                stats_font_name: None,
                ..Default::default()
            }),
            ..Default::default()
        };

        let toml_str = toml::to_string(&config).unwrap();

        // Should contain message_font_name
        assert!(toml_str.contains("message_font_name"));

        // Should NOT contain the None fields
        assert!(!toml_str.contains("info_font_name"));
        assert!(!toml_str.contains("sha_font_name"));
        assert!(!toml_str.contains("stats_font_name"));
    }

    #[test]
    fn test_font_deserialization_missing_fields() {
        let toml_str = r#"
            [burned_in_chyron]
            default_font_name = "Liberation Sans"

            [client]
            camera_device = "0"

            [server]
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        let chyron = config.burned_in_chyron.unwrap();

        assert_eq!(chyron.default_font_name, "Liberation Sans");
        assert_eq!(chyron.message_font_name, None);
        assert_eq!(chyron.get_message_font_name(), "Liberation Sans");
    }

    #[test]
    fn test_default_bind_address_and_port() {
        let server = ServerConfig::default();
        assert_eq!(server.bind_address, "127.0.0.1");
        assert_eq!(server.bind_port, 3000);
        assert_eq!(server.max_concurrent_uploads, 4);
    }

    #[test]
    fn test_custom_bind_address_and_port() {
        let toml_str = r#"
            [server]
            bind_address = "0.0.0.0"
            bind_port = 8080
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        let server = config.server.unwrap();
        assert_eq!(server.bind_address, "0.0.0.0");
        assert_eq!(server.bind_port, 8080);
    }

    #[test]
    fn test_bind_config_serialization() {
        let config = Config {
            server: Some(ServerConfig {
                bind_address: "0.0.0.0".to_string(),
                bind_port: 8080,
                ..Default::default()
            }),
            ..Default::default()
        };

        let toml_str = toml::to_string(&config).unwrap();
        assert!(toml_str.contains("bind_address = \"0.0.0.0\""));
        assert!(toml_str.contains("bind_port = 8080"));

        let parsed: Config = toml::from_str(&toml_str).unwrap();
        let server = parsed.server.unwrap();
        assert_eq!(server.bind_address, "0.0.0.0");
        assert_eq!(server.bind_port, 8080);
    }

    #[test]
    fn test_server_burned_in_chyron_false() {
        let toml_str = r#"
            [server]
            burned_in_chyron = false
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        let server = config.server.unwrap();
        assert!(!server.burned_in_chyron);
    }

    #[test]
    fn test_server_burned_in_chyron_defaults_to_true() {
        let toml_str = r#"
            [server]
            bind_port = 8080
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();
        let server = config.server.unwrap();
        assert!(server.burned_in_chyron);
    }

    #[test]
    fn load_from_explicit_missing_path_errors() -> Result<()> {
        let missing = PathBuf::from("/nonexistent/definitely/not/here/config.toml");
        let result = Config::load_from(Some(missing.clone()));
        assert!(
            matches!(result, Err(Error::ConfigFileNotFound { path }) if path == missing),
            "expected ConfigFileNotFound for an explicit missing path"
        );
        Ok(())
    }

    #[test]
    fn legacy_camera_device_key_maps_to_camera_devices() -> Result<()> {
        let toml_str = r#"
            [client]
            camera_device = "/dev/video2"
        "#;
        let config: Config = toml::from_str(toml_str)?;
        let devices = config.client.map(|c| c.camera_devices).unwrap_or_default();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device, "/dev/video2");
        Ok(())
    }

    #[test]
    fn new_camera_devices_key_takes_precedence_over_legacy() -> Result<()> {
        let toml_str = r#"
            [client]
            camera_device = "0"

            [[client.camera_devices]]
            device = "/dev/video9"
        "#;
        let config: Config = toml::from_str(toml_str)?;
        let devices = config.client.map(|c| c.camera_devices).unwrap_or_default();
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].device, "/dev/video9");
        Ok(())
    }

    #[test]
    fn static_root_defaults_to_system_share() {
        let config = ServerConfig::default();

        // The development shell exports this, so it has to be cleared here.
        temp_env::with_var_unset(STATIC_ROOT_ENV, || {
            assert_eq!(config.static_root(), PathBuf::from(DEFAULT_STATIC_DIR));
        });
    }

    #[test]
    fn static_root_uses_configured_dir() {
        let config = ServerConfig {
            static_dir: "/srv/gallery".to_owned(),
            ..Default::default()
        };

        temp_env::with_var_unset(STATIC_ROOT_ENV, || {
            assert_eq!(config.static_root(), PathBuf::from("/srv/gallery"));
        });
    }

    #[test]
    fn static_root_env_var_overrides_configured_dir() {
        let config = ServerConfig {
            static_dir: "/srv/gallery".to_owned(),
            ..Default::default()
        };

        temp_env::with_var(STATIC_ROOT_ENV, Some("/home/dev/assets/static"), || {
            assert_eq!(
                config.static_root(),
                PathBuf::from("/home/dev/assets/static")
            );
        });
    }

    #[test]
    fn static_root_ignores_empty_env_var() {
        let config = ServerConfig {
            static_dir: "/srv/gallery".to_owned(),
            ..Default::default()
        };

        temp_env::with_var(STATIC_ROOT_ENV, Some(""), || {
            assert_eq!(config.static_root(), PathBuf::from("/srv/gallery"));
        });
    }
}
