//! Application Configuration
//!
//! Handles loading and saving application configuration.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub hotkey: HotkeyConfig,
    #[serde(default)]
    pub floating_button: FloatingButtonConfig,
    #[serde(default)]
    pub desktop_pet: DesktopPetConfig,
    #[serde(default)]
    pub asr: AsrConfig,
    #[serde(default)]
    pub ai: AiConfig,
    #[serde(default)]
    pub custom_vocabulary: std::collections::HashMap<String, String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            hotkey: HotkeyConfig::default(),
            floating_button: FloatingButtonConfig::default(),
            desktop_pet: DesktopPetConfig::default(),
            asr: AsrConfig::default(),
            ai: AiConfig::default(),
            custom_vocabulary: std::collections::HashMap::new(),
        }
    }
}

impl AppConfig {
    /// Get the config file path
    pub fn config_path() -> PathBuf {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        exe_dir.join("config.toml")
    }

    /// Get the credentials file path
    pub fn credentials_path() -> PathBuf {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));
        exe_dir.join("credentials.json")
    }

    /// Load configuration from file or create default
    pub fn load_or_default() -> Result<Self> {
        let path = Self::config_path();

        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let config: AppConfig = toml::from_str(&content)?;
            Ok(config)
        } else {
            let config = AppConfig::default();
            config.save()?;
            Ok(config)
        }
    }

    /// Save configuration to file
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path();
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)?;
        Ok(())
    }
}

/// General configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_history_log_enabled")]
    pub history_log_enabled: bool,
    #[serde(default = "default_history_log_path")]
    pub history_log_path: String,
}

fn default_language() -> String {
    "zh-CN".to_string()
}

fn default_history_log_enabled() -> bool {
    true
}

fn default_history_log_path() -> String {
    "dictation_history.jsonl".to_string()
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            auto_start: false,
            language: default_language(),
            history_log_enabled: default_history_log_enabled(),
            history_log_path: default_history_log_path(),
        }
    }
}

/// Hotkey configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    #[serde(default = "default_hotkey_mode")]
    pub mode: String,
    #[serde(default = "default_combo_key")]
    pub combo_key: String,
    #[serde(default = "default_double_tap_key")]
    pub double_tap_key: String,
    #[serde(default = "default_double_tap_interval")]
    pub double_tap_interval: u64,
}

fn default_hotkey_mode() -> String {
    "double_tap".to_string()
}

fn default_combo_key() -> String {
    "Ctrl+Shift+V".to_string()
}

fn default_double_tap_key() -> String {
    "Ctrl".to_string()
}

fn default_double_tap_interval() -> u64 {
    300
}

impl Default for HotkeyConfig {
    fn default() -> Self {
        Self {
            mode: default_hotkey_mode(),
            combo_key: default_combo_key(),
            double_tap_key: default_double_tap_key(),
            double_tap_interval: default_double_tap_interval(),
        }
    }
}

/// Floating button configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FloatingButtonConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_position")]
    pub position_x: i32,
    #[serde(default = "default_position")]
    pub position_y: i32,
    #[serde(default = "default_stiffness")]
    pub stiffness: f32,
    #[serde(default = "default_damping")]
    pub damping: f32,
}

fn default_true() -> bool {
    true
}

fn default_position() -> i32 {
    100
}

fn default_stiffness() -> f32 {
    180.0
}

fn default_damping() -> f32 {
    12.0
}

impl Default for FloatingButtonConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            position_x: 100,
            position_y: 100,
            stiffness: 180.0,
            damping: 12.0,
        }
    }
}

/// Desktop pet configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DesktopPetConfig {
    #[serde(default = "default_desktop_pet_enabled")]
    pub enabled: bool,
    #[serde(default = "default_desktop_pet_position")]
    pub position_x: i32,
    #[serde(default = "default_desktop_pet_position")]
    pub position_y: i32,
    #[serde(default = "default_desktop_pet_size")]
    pub size: i32,
}

fn default_desktop_pet_enabled() -> bool {
    true
}

fn default_desktop_pet_position() -> i32 {
    -1
}

fn default_desktop_pet_size() -> i32 {
    160
}

impl Default for DesktopPetConfig {
    fn default() -> Self {
        Self {
            enabled: default_desktop_pet_enabled(),
            position_x: default_desktop_pet_position(),
            position_y: default_desktop_pet_position(),
            size: default_desktop_pet_size(),
        }
    }
}

/// ASR configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrConfig {
    #[serde(default = "default_true")]
    pub vad_enabled: bool,
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self { vad_enabled: true }
    }
}

/// AI post-processing and translation configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default = "default_api_key")]
    pub api_key: String,
    #[serde(default = "default_api_endpoint")]
    pub api_endpoint: String,
    #[serde(default = "default_model")]
    pub model: String,
    #[serde(default)]
    pub post_process_enabled: bool,
    #[serde(default = "default_post_process_prompt")]
    pub post_process_prompt: String,
    #[serde(default)]
    pub translation_enabled: bool,
    #[serde(default = "default_translation_prompt")]
    pub translation_prompt: String,
}

fn default_api_key() -> String {
    "".to_string()
}

fn default_api_endpoint() -> String {
    "https://api.openai.com/v1".to_string()
}

fn default_model() -> String {
    "gpt-3.5-turbo".to_string()
}

fn default_post_process_prompt() -> String {
    "You are an AI assistant helping to format and correct grammar for dictated speech. Correct spelling, grammar, punctuation, and formatting errors while keeping the original meaning and tone. Output only the corrected text and nothing else.".to_string()
}

fn default_translation_prompt() -> String {
    "Translate the input text into English if it is Chinese, or into Chinese if it is English. Keep the output concise and only return the translated text.".to_string()
}

impl Default for AiConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_key: default_api_key(),
            api_endpoint: default_api_endpoint(),
            model: default_model(),
            post_process_enabled: false,
            post_process_prompt: default_post_process_prompt(),
            translation_enabled: false,
            translation_prompt: default_translation_prompt(),
        }
    }
}
