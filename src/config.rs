use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use xdg::BaseDirectories;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
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

    #[serde(default = "default_background_path")]
    pub background_path: String,

    #[serde(default = "default_camera_device")]
    pub camera_device: String,

    #[serde(default = "default_camera_warmup_frames")]
    pub camera_warmup_frames: usize,

    #[serde(default = "default_chyron_opacity")]
    pub chyron_opacity: f32,

    #[serde(default = "default_title_font_size")]
    pub title_font_size: f32,

    #[serde(default = "default_info_font_size")]
    pub info_font_size: f32,

    #[serde(default = "default_center_person")]
    pub center_person: bool,

    #[serde(default = "default_enable_chyron")]
    pub enable_chyron: bool,

    #[serde(default = "default_gallery_title")]
    pub gallery_title: String,

    #[serde(default = "default_server_url")]
    pub server_url: String,

    #[serde(default = "default_server_upload_timeout_secs")]
    pub server_upload_timeout_secs: u64,
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

fn default_camera_device() -> String {
    "0".to_string()
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

fn default_enable_chyron() -> bool {
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

impl Default for Config {
    fn default() -> Self {
        Self {
            default_font_name: default_font_name(),
            message_font_name: None,
            info_font_name: None,
            sha_font_name: None,
            stats_font_name: None,
            background_path: default_background_path(),
            camera_device: default_camera_device(),
            camera_warmup_frames: default_camera_warmup_frames(),
            chyron_opacity: default_chyron_opacity(),
            title_font_size: default_title_font_size(),
            info_font_size: default_info_font_size(),
            center_person: default_center_person(),
            enable_chyron: default_enable_chyron(),
            gallery_title: default_gallery_title(),
            server_url: default_server_url(),
            server_upload_timeout_secs: default_server_upload_timeout_secs(),
        }
    }
}

impl Config {
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
    /// Load configuration from XDG_CONFIG_HOME/lolcommits/config.toml
    pub fn load() -> Result<Self> {
        let base_dirs = BaseDirectories::with_prefix("lolcommits")?;

        let config_path = base_dirs.place_config_file("config.toml")?;

        if !config_path.exists() {
            tracing::info!(path = %config_path.display(), "Config file not found, creating default");
            let default_config = Config::default();
            default_config.save()?;
            return Ok(default_config);
        }

        tracing::debug!(path = %config_path.display(), "Loading config");
        let contents = std::fs::read_to_string(&config_path).map_err(|source| {
            Error::ConfigFileRead {
                path: config_path.clone(),
                source,
            }
        })?;

        let config: Config = toml::from_str(&contents)?;

        tracing::debug!(?config, "Config loaded successfully");
        Ok(config)
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
        assert_eq!(config.camera_device, "0");
        assert_eq!(config.camera_warmup_frames, 3);
        assert_eq!(config.chyron_opacity, 0.75);
        assert!(config.center_person);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.camera_device, parsed.camera_device);
    }

    #[test]
    fn test_font_fallback_all_none() {
        // When all font fields are None, everything should fall back to default_font_name
        let config = Config {
            default_font_name: "DejaVu Sans".to_string(),
            message_font_name: None,
            info_font_name: None,
            sha_font_name: None,
            stats_font_name: None,
            ..Default::default()
        };

        assert_eq!(config.get_message_font_name(), "DejaVu Sans");
        assert_eq!(config.get_info_font_name(), "DejaVu Sans");
        assert_eq!(config.get_sha_font_name(), "DejaVu Sans");
        assert_eq!(config.get_stats_font_name(), "DejaVu Sans");
    }

    #[test]
    fn test_font_fallback_mixed() {
        // When some fonts are specified and others are None
        let config = Config {
            default_font_name: "monospace".to_string(),
            message_font_name: Some("Arial".to_string()),
            info_font_name: None,
            sha_font_name: Some("Courier New".to_string()),
            stats_font_name: None,
            ..Default::default()
        };

        assert_eq!(config.get_message_font_name(), "Arial");
        assert_eq!(config.get_info_font_name(), "monospace");
        assert_eq!(config.get_sha_font_name(), "Courier New");
        assert_eq!(config.get_stats_font_name(), "monospace");
    }

    #[test]
    fn test_default_font_name_is_monospace() {
        // Verify that the default font name is "monospace"
        let config = Config::default();
        assert_eq!(config.default_font_name, "monospace");
        assert_eq!(config.get_message_font_name(), "monospace");
        assert_eq!(config.get_info_font_name(), "monospace");
        assert_eq!(config.get_sha_font_name(), "monospace");
        assert_eq!(config.get_stats_font_name(), "monospace");
    }

    #[test]
    fn test_font_serialization_omits_none() {
        // Verify that None values are not serialized in TOML
        let config = Config {
            default_font_name: "monospace".to_string(),
            message_font_name: Some("Arial".to_string()),
            info_font_name: None,
            sha_font_name: None,
            stats_font_name: None,
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
        // When loading a config that doesn't specify optional fonts
        let toml_str = r#"
            default_font_name = "Liberation Sans"
            camera_device = "0"
        "#;

        let config: Config = toml::from_str(toml_str).unwrap();

        assert_eq!(config.default_font_name, "Liberation Sans");
        assert_eq!(config.message_font_name, None);
        assert_eq!(config.get_message_font_name(), "Liberation Sans");
    }
}
