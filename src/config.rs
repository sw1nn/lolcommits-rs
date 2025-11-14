use crate::error::{LolcommitsError, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use xdg::BaseDirectories;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_font_name")]
    pub font_name: String,

    #[serde(default = "default_background_path")]
    pub background_path: String,

    #[serde(default = "default_camera_index")]
    pub camera_index: usize,

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
}

fn default_font_name() -> String {
    "monospace".to_string()
}

fn default_background_path() -> String {
    let base_dirs = BaseDirectories::with_prefix("lolcommits-rs")
        .expect("Failed to get XDG base directories");
    base_dirs
        .get_data_home()
        .join("background.png")
        .to_string_lossy()
        .to_string()
}

fn default_camera_index() -> usize {
    0
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

impl Default for Config {
    fn default() -> Self {
        Self {
            font_name: default_font_name(),
            background_path: default_background_path(),
            camera_index: default_camera_index(),
            camera_warmup_frames: default_camera_warmup_frames(),
            chyron_opacity: default_chyron_opacity(),
            title_font_size: default_title_font_size(),
            info_font_size: default_info_font_size(),
            center_person: default_center_person(),
        }
    }
}

impl Config {
    /// Load configuration from XDG_CONFIG_HOME/lolcommits-rs/config.toml
    pub fn load() -> Result<Self> {
        let base_dirs = BaseDirectories::with_prefix("lolcommits-rs")
            .map_err(|e| LolcommitsError::ConfigError {
                message: format!("Failed to get XDG base directories: {}", e),
            })?;

        let config_path = base_dirs
            .place_config_file("config.toml")
            .map_err(|e| LolcommitsError::ConfigError {
                message: format!("Failed to create config directory: {}", e),
            })?;

        if !config_path.exists() {
            tracing::info!(path = %config_path.display(), "Config file not found, creating default");
            let default_config = Config::default();
            default_config.save()?;
            return Ok(default_config);
        }

        tracing::debug!(path = %config_path.display(), "Loading config");
        let contents = std::fs::read_to_string(&config_path).map_err(|e| {
            LolcommitsError::ConfigError {
                message: format!("Failed to read config file: {}", e),
            }
        })?;

        let config: Config = toml::from_str(&contents).map_err(|e| LolcommitsError::ConfigError {
            message: format!("Failed to parse config file: {}", e),
        })?;

        tracing::debug!(?config, "Config loaded successfully");
        Ok(config)
    }

    /// Save configuration to XDG_CONFIG_HOME/lolcommits-rs/config.toml
    pub fn save(&self) -> Result<()> {
        let base_dirs = BaseDirectories::with_prefix("lolcommits-rs")
            .map_err(|e| LolcommitsError::ConfigError {
                message: format!("Failed to get XDG base directories: {}", e),
            })?;

        let config_path = base_dirs
            .place_config_file("config.toml")
            .map_err(|e| LolcommitsError::ConfigError {
                message: format!("Failed to create config directory: {}", e),
            })?;

        let contents = toml::to_string_pretty(self).map_err(|e| LolcommitsError::ConfigError {
            message: format!("Failed to serialize config: {}", e),
        })?;

        std::fs::write(&config_path, contents).map_err(|e| LolcommitsError::ConfigError {
            message: format!("Failed to write config file: {}", e),
        })?;

        tracing::info!(path = %config_path.display(), "Config saved successfully");
        Ok(())
    }

    /// Get the path to the config file
    pub fn config_path() -> Result<PathBuf> {
        let base_dirs = BaseDirectories::with_prefix("lolcommits-rs")
            .map_err(|e| LolcommitsError::ConfigError {
                message: format!("Failed to get XDG base directories: {}", e),
            })?;

        Ok(base_dirs.get_config_home())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.camera_index, 0);
        assert_eq!(config.camera_warmup_frames, 3);
        assert_eq!(config.chyron_opacity, 0.75);
        assert!(config.center_person);
    }

    #[test]
    fn test_config_serialization() {
        let config = Config::default();
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        assert_eq!(config.camera_index, parsed.camera_index);
    }
}
