//! Application configuration.
//!
//! The file format is intentionally additive: every section and field has a
//! serde default so configurations created by earlier Aiko IME releases keep
//! loading after an upgrade.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_HISTORY_FILE: &str = "dictation_history.jsonl";

/// Application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default)]
    pub general: GeneralConfig,
    #[serde(default)]
    pub audio: AudioConfig,
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
    pub custom_vocabulary: HashMap<String, String>,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            general: GeneralConfig::default(),
            audio: AudioConfig::default(),
            hotkey: HotkeyConfig::default(),
            floating_button: FloatingButtonConfig::default(),
            desktop_pet: DesktopPetConfig::default(),
            asr: AsrConfig::default(),
            ai: AiConfig::default(),
            custom_vocabulary: HashMap::new(),
        }
    }
}

impl AppConfig {
    /// Get the config file path.
    pub fn config_path() -> PathBuf {
        application_dir().join("config.toml")
    }

    /// Get the credentials file path.
    pub fn credentials_path() -> PathBuf {
        application_dir().join("credentials.json")
    }

    /// Load configuration, filling fields introduced by newer releases.
    pub fn load_or_default() -> Result<Self> {
        let path = Self::config_path();
        if !path.exists() {
            let config = AppConfig::default();
            config.save()?;
            return Ok(config);
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read configuration: {}", path.display()))?;
        let mut config: AppConfig = toml::from_str(&content)
            .with_context(|| format!("invalid configuration file: {}", path.display()))?;

        // Keep hand-edited legacy files usable. Invalid enum values and unsafe
        // numeric ranges are repaired once, then persisted in the new format.
        if config.normalize() {
            config.save()?;
        }
        Ok(config)
    }

    /// Validate and save the configuration.
    pub fn save(&self) -> Result<()> {
        self.validate()?;
        let path = Self::config_path();
        let content = toml::to_string_pretty(self)?;
        fs::write(&path, content)
            .with_context(|| format!("failed to save configuration: {}", path.display()))
    }

    /// Reject values that cannot be represented safely by the runtime/UI.
    pub fn validate(&self) -> Result<()> {
        if self.general.language.trim().is_empty() {
            bail!("language cannot be empty");
        }
        if self.general.history_log_enabled && self.general.history_log_path.trim().is_empty() {
            bail!("history path cannot be empty while history is enabled");
        }
        if !matches!(self.hotkey.mode.as_str(), "combo" | "double_tap") {
            bail!("hotkey mode must be 'combo' or 'double_tap'");
        }
        if !is_valid_combo(&self.hotkey.combo_key) {
            bail!("invalid combo hotkey: {}", self.hotkey.combo_key);
        }
        if !matches!(
            self.hotkey.double_tap_key.to_ascii_lowercase().as_str(),
            "ctrl" | "shift" | "alt" | "capslock"
        ) {
            bail!("double-tap key must be Ctrl, Shift, Alt, or CapsLock");
        }
        if !(100..=2_000).contains(&self.hotkey.double_tap_interval) {
            bail!("double-tap interval must be between 100 and 2000 ms");
        }
        if !(96..=320).contains(&self.desktop_pet.size) {
            bail!("desktop pet size must be between 96 and 320 pixels");
        }
        if !(20.0..=500.0).contains(&self.floating_button.stiffness)
            || !(1.0..=80.0).contains(&self.floating_button.damping)
        {
            bail!("floating control animation values are out of range");
        }
        if !matches!(self.asr.backend.as_str(), "online" | "offline") {
            bail!("ASR backend must be 'online' or 'offline'");
        }
        if self.asr.online_provider.trim().is_empty() || self.asr.offline_provider.trim().is_empty()
        {
            bail!("ASR provider cannot be empty");
        }
        if !is_supported_online_provider(&self.asr.online_provider) {
            bail!(
                "unsupported online ASR provider: {}",
                self.asr.online_provider
            );
        }
        if !is_supported_offline_provider(&self.asr.offline_provider) {
            bail!(
                "unsupported offline ASR provider: {}",
                self.asr.offline_provider
            );
        }
        if self.asr.backend == "offline" && self.asr.offline_model_dir.trim().is_empty() {
            bail!("offline model directory cannot be empty in offline mode");
        }
        if self.ai.enabled {
            if !is_http_endpoint(&self.ai.api_endpoint) {
                bail!("AI endpoint must start with http:// or https://");
            }
            if self.ai.model.trim().is_empty() {
                bail!("AI model cannot be empty");
            }
        }
        if self.ai.post_process_enabled && self.ai.post_process_prompt.trim().is_empty() {
            bail!("post-processing prompt cannot be empty");
        }
        if self.ai.translation_enabled && self.ai.translation_prompt.trim().is_empty() {
            bail!("translation prompt cannot be empty");
        }
        if self
            .custom_vocabulary
            .iter()
            .any(|(source, replacement)| source.trim().is_empty() || replacement.trim().is_empty())
        {
            bail!("custom vocabulary entries cannot contain empty terms");
        }
        Ok(())
    }

    /// Normalize old or hand-edited values. Returns true when anything changed.
    pub fn normalize(&mut self) -> bool {
        let before = toml::to_string(self).unwrap_or_default();

        self.general.language = non_empty_or(&self.general.language, "zh-CN");
        self.general.history_log_path =
            non_empty_or(&self.general.history_log_path, DEFAULT_HISTORY_FILE);
        self.audio.input_device = self.audio.input_device.trim().to_string();

        if !matches!(self.hotkey.mode.as_str(), "combo" | "double_tap") {
            self.hotkey.mode = default_hotkey_mode();
        }
        if !is_valid_combo(&self.hotkey.combo_key) {
            self.hotkey.combo_key = default_combo_key();
        }
        if !matches!(
            self.hotkey.double_tap_key.to_ascii_lowercase().as_str(),
            "ctrl" | "shift" | "alt" | "capslock"
        ) {
            self.hotkey.double_tap_key = default_double_tap_key();
        }
        self.hotkey.double_tap_interval = self.hotkey.double_tap_interval.clamp(100, 2_000);

        self.desktop_pet.size = self.desktop_pet.size.clamp(96, 320);
        self.floating_button.stiffness = self.floating_button.stiffness.clamp(20.0, 500.0);
        self.floating_button.damping = self.floating_button.damping.clamp(1.0, 80.0);

        if !matches!(self.asr.backend.as_str(), "online" | "offline") {
            self.asr.backend = default_asr_backend();
        }
        self.asr.online_provider =
            non_empty_or(&self.asr.online_provider, &default_online_provider());
        self.asr.offline_provider =
            non_empty_or(&self.asr.offline_provider, &default_offline_provider());
        self.asr.online_provider = self.asr.online_provider.trim().to_ascii_lowercase();
        self.asr.offline_provider = self.asr.offline_provider.trim().to_ascii_lowercase();
        if self.asr.offline_provider == "sherpa-onnx" {
            self.asr.offline_provider = default_offline_provider();
        }
        if !is_supported_online_provider(&self.asr.online_provider) {
            self.asr.online_provider = default_online_provider();
        }
        if !is_supported_offline_provider(&self.asr.offline_provider) {
            self.asr.offline_provider = default_offline_provider();
        }
        self.asr.offline_model_dir =
            non_empty_or(&self.asr.offline_model_dir, &default_offline_model_dir());

        self.ai.api_endpoint = non_empty_or(&self.ai.api_endpoint, &default_api_endpoint());
        self.ai.model = non_empty_or(&self.ai.model, &default_model());
        if self.ai.post_process_prompt.trim().is_empty() {
            self.ai.post_process_prompt = default_post_process_prompt();
        }
        if self.ai.translation_prompt.trim().is_empty() {
            self.ai.translation_prompt = default_translation_prompt();
        }

        self.custom_vocabulary.retain(|source, replacement| {
            !source.trim().is_empty() && !replacement.trim().is_empty()
        });

        before != toml::to_string(self).unwrap_or_default()
    }

    /// Resolve the history path relative to the application directory.
    pub fn history_path(&self) -> PathBuf {
        let path = PathBuf::from(self.general.history_log_path.trim());
        if path.is_absolute() {
            path
        } else {
            application_dir().join(path)
        }
    }

    /// Delete local dictation history. Returns whether a file existed.
    pub fn clear_history(&self) -> Result<bool> {
        let path = self.history_path();
        if !path.exists() {
            return Ok(false);
        }
        fs::remove_file(&path)
            .with_context(|| format!("failed to clear history: {}", path.display()))?;
        Ok(true)
    }
}

fn application_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(Path::to_path_buf))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn non_empty_or(value: &str, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

fn is_http_endpoint(endpoint: &str) -> bool {
    let endpoint = endpoint.trim().to_ascii_lowercase();
    endpoint.starts_with("https://") || endpoint.starts_with("http://")
}

fn is_valid_combo(combo: &str) -> bool {
    let parts: Vec<_> = combo
        .split('+')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .collect();
    if parts.is_empty() {
        return false;
    }

    let mut primary_keys = 0;
    for part in parts {
        let lower = part.to_ascii_lowercase();
        if matches!(lower.as_str(), "ctrl" | "shift" | "alt" | "win") {
            continue;
        }
        let is_letter_or_number =
            part.len() == 1 && part.chars().all(|ch| ch.is_ascii_alphanumeric());
        let is_function = lower
            .strip_prefix('f')
            .and_then(|number| number.parse::<u8>().ok())
            .is_some_and(|number| (1..=12).contains(&number));
        let is_named = matches!(
            lower.as_str(),
            "space" | "enter" | "tab" | "escape" | "backspace"
        );
        if !(is_letter_or_number || is_function || is_named) {
            return false;
        }
        primary_keys += 1;
    }
    primary_keys == 1
}

fn is_supported_online_provider(provider: &str) -> bool {
    matches!(provider.trim().to_ascii_lowercase().as_str(), "doubao")
}

fn is_supported_offline_provider(provider: &str) -> bool {
    matches!(
        provider.trim().to_ascii_lowercase().as_str(),
        "sherpa_onnx" | "sherpa-onnx"
    )
}

/// General configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GeneralConfig {
    #[serde(default)]
    pub auto_start: bool,
    #[serde(default = "default_language")]
    pub language: String,
    /// History is opt-in for new installations. Existing explicit values remain.
    #[serde(default)]
    pub history_log_enabled: bool,
    #[serde(default = "default_history_log_path")]
    pub history_log_path: String,
}

fn default_language() -> String {
    "zh-CN".to_string()
}

fn default_history_log_path() -> String {
    DEFAULT_HISTORY_FILE.to_string()
}

impl Default for GeneralConfig {
    fn default() -> Self {
        Self {
            auto_start: false,
            language: default_language(),
            history_log_enabled: false,
            history_log_path: default_history_log_path(),
        }
    }
}

/// Audio input configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AudioConfig {
    /// Empty means the Windows default input device.
    #[serde(default)]
    pub input_device: String,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            input_device: String::new(),
        }
    }
}

/// Hotkey configuration.
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

/// Floating control configuration.
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
            position_x: default_position(),
            position_y: default_position(),
            stiffness: default_stiffness(),
            damping: default_damping(),
        }
    }
}

/// Desktop pet configuration.
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

/// Speech-recognition backend configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsrConfig {
    /// "online" or "offline".
    #[serde(default = "default_asr_backend")]
    pub backend: String,
    #[serde(default = "default_online_provider")]
    pub online_provider: String,
    #[serde(default = "default_offline_provider")]
    pub offline_provider: String,
    #[serde(default = "default_offline_model_dir")]
    pub offline_model_dir: String,
    #[serde(default = "default_true")]
    pub vad_enabled: bool,
}

fn default_asr_backend() -> String {
    "online".to_string()
}

fn default_online_provider() -> String {
    "doubao".to_string()
}

fn default_offline_provider() -> String {
    "sherpa_onnx".to_string()
}

fn default_offline_model_dir() -> String {
    "models/sherpa-onnx".to_string()
}

impl Default for AsrConfig {
    fn default() -> Self {
        Self {
            backend: default_asr_backend(),
            online_provider: default_online_provider(),
            offline_provider: default_offline_provider(),
            offline_model_dir: default_offline_model_dir(),
            vad_enabled: true,
        }
    }
}

/// AI post-processing and translation configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
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
            api_key: String::new(),
            api_endpoint: default_api_endpoint(),
            model: default_model(),
            post_process_enabled: false,
            post_process_prompt: default_post_process_prompt(),
            translation_enabled: false,
            translation_prompt: default_translation_prompt(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_config_loads_with_new_defaults() {
        let config: AppConfig = toml::from_str(
            r#"
                [general]
                language = "zh-CN"

                [hotkey]
                mode = "double_tap"
                double_tap_key = "Ctrl"
                combo_key = "Ctrl+Shift+V"
                double_tap_interval = 300

                [asr]
                vad_enabled = true
            "#,
        )
        .unwrap();

        assert_eq!(config.asr.backend, "online");
        assert_eq!(config.asr.offline_provider, "sherpa_onnx");
        assert!(!config.general.history_log_enabled);
        assert!(config.audio.input_device.is_empty());
        config.validate().unwrap();
    }

    #[test]
    fn invalid_values_are_normalized() {
        let mut config = AppConfig::default();
        config.hotkey.mode = "mystery".to_string();
        config.hotkey.combo_key = "Ctrl+Banana".to_string();
        config.hotkey.double_tap_interval = 5;
        config.desktop_pet.size = 4_000;
        config.asr.backend = "cloudish".to_string();
        config.asr.online_provider = "future_cloud".to_string();
        config.asr.offline_provider = "sherpa-onnx".to_string();

        assert!(config.normalize());
        assert_eq!(config.hotkey.mode, "double_tap");
        assert_eq!(config.hotkey.combo_key, "Ctrl+Shift+V");
        assert_eq!(config.hotkey.double_tap_interval, 100);
        assert_eq!(config.desktop_pet.size, 320);
        assert_eq!(config.asr.backend, "online");
        assert_eq!(config.asr.online_provider, "doubao");
        assert_eq!(config.asr.offline_provider, "sherpa_onnx");
        config.validate().unwrap();
    }

    #[test]
    fn combo_validation_accepts_supported_keys_only() {
        assert!(is_valid_combo("Ctrl+Shift+V"));
        assert!(is_valid_combo("Alt+F12"));
        assert!(!is_valid_combo("Ctrl"));
        assert!(!is_valid_combo("Ctrl+Mouse4"));
        assert!(!is_valid_combo("Ctrl+V+B"));
    }
}
