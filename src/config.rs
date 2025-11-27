use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use xdg::BaseDirectories;

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

    #[serde(default = "default_burned_in_chyron")]
    pub burned_in_chyron: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    /// List of camera devices to try in order. First working camera is used.
    /// Each camera can have its own format/resolution settings.
    #[serde(default = "default_camera_devices")]
    pub camera_devices: Vec<CameraDeviceConfig>,

    #[serde(default = "default_camera_warmup_frames")]
    pub camera_warmup_frames: usize,

    #[serde(default = "default_server_url")]
    pub server_url: String,

    #[serde(default = "default_server_upload_timeout_secs")]
    pub server_upload_timeout_secs: u64,
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

    #[serde(default = "default_bind_address")]
    pub bind_address: String,

    #[serde(default = "default_bind_port")]
    pub bind_port: u16,
}

fn default_font_name() -> String {
    "monospace".to_string()
}

fn default_background_path() -> String {
    let base_dirs =
        BaseDirectories::with_prefix("lolcommits").expect("Failed to get XDG base directories");
    base_dirs
        .get_data_home()
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

fn default_bind_address() -> String {
    "0.0.0.0".to_string()
}

fn default_bind_port() -> u16 {
    3000
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
            burned_in_chyron: default_burned_in_chyron(),
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
            bind_address: default_bind_address(),
            bind_port: default_bind_port(),
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
                let base_dirs = BaseDirectories::with_prefix("lolcommits")?;
                let user_config = base_dirs.place_config_file("config.toml")?;
                tracing::debug!(path = %user_config.display(), "Using user config");
                user_config
            }
        };

        if !config_path.exists() {
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
        let base_dirs = BaseDirectories::with_prefix("lolcommits")?;

        let config_path = base_dirs.place_config_file("config.toml")?;

        let contents = toml::to_string_pretty(self)?;

        std::fs::write(&config_path, contents).map_err(|source| Error::ConfigFileWrite {
            path: config_path.clone(),
            source,
        })?;

        tracing::info!(path = %config_path.display(), "Config saved successfully");
        Ok(())
    }

    /// Get the path to the config file
    pub fn config_path() -> Result<PathBuf> {
        let base_dirs = BaseDirectories::with_prefix("lolcommits")?;

        Ok(base_dirs.get_config_home())
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
        assert_eq!(server.bind_address, "0.0.0.0");
        assert_eq!(server.bind_port, 3000);
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
}
