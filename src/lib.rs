//! AikoIME - Windows voice input tool
//!
//! A lightweight voice input tool that uses ASR for real-time
//! speech recognition and inserts text into the focused window.

pub mod asr;
pub mod audio;
pub mod business;
pub mod data;
pub mod offline;
pub mod platform;
pub mod ui;

pub use asr::AsrClient;
pub use audio::AudioCapture;
pub use business::{HotkeyManager, TextInputError, TextInserter, VoiceController};
pub use data::{AppConfig, CredentialStore};
