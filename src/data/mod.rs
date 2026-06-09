//! Data module for configuration and credential management

mod config;
mod credential;

pub use config::{
    AppConfig, AsrConfig, DesktopPetConfig, FloatingButtonConfig, GeneralConfig, HotkeyConfig,
};
pub use credential::CredentialStore;
