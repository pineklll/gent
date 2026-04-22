use once_cell::sync::OnceCell;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Application-level configuration loaded from XDG config directory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppConfig {
    /// Default LLM format when a node leaves the format field empty.
    /// E.g. "openai" or "anthropic"
    pub default_format: Option<String>,

    /// Per-format/provider defaults.
    #[serde(default)]
    pub providers: HashMap<String, ProviderConfig>,
}

/// Defaults for a single provider/format.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub model: Option<String>,
    pub api_key: Option<String>,
    /// Base endpoint URL (maps to `custom_url` in the LLM command).
    pub endpoint: Option<String>,
    /// MiniMax group ID for embedding API.
    pub group_id: Option<String>,
}

static CONFIG: OnceCell<AppConfig> = OnceCell::new();

/// Load configuration from the XDG config path (`~/.config/gent/config.toml`).
/// If the file does not exist or is malformed, an empty default config is used.
pub fn load_config() {
    let config_path = config_file_path();
    let config = if config_path.exists() {
        std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|content| toml::from_str::<AppConfig>(&content).ok())
            .unwrap_or_default()
    } else {
        AppConfig::default()
    };
    let _ = CONFIG.set(config);
}

/// Returns the expected path to `gent/config.toml` inside the XDG config directory.
pub fn config_file_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
        .join("gent")
        .join("config.toml")
}

/// Get the globally loaded config.
pub fn get_config() -> &'static AppConfig {
    CONFIG.get().expect("config not loaded")
}
