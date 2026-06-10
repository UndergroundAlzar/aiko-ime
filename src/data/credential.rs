//! Credential Store
//!
//! Manages device credentials with optional encryption.

use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::time::Duration;

use crate::asr::{get_asr_token, register_device, DeviceCredentials};
use crate::data::AppConfig;

/// Credential store for managing device credentials
pub struct CredentialStore {
    credentials_path: PathBuf,
    credentials: Option<DeviceCredentials>,
}

impl CredentialStore {
    /// Create a new credential store
    pub fn new(_config: &AppConfig) -> Result<Self> {
        let credentials_path = AppConfig::credentials_path();

        // Try to load existing credentials
        let credentials = if credentials_path.exists() {
            DeviceCredentials::load(&credentials_path).ok()
        } else {
            None
        };

        Ok(Self {
            credentials_path,
            credentials,
        })
    }

    /// Ensure we have valid credentials
    pub async fn ensure_credentials(&self) -> Result<DeviceCredentials> {
        // Check if we have existing complete, fresh credentials.
        // Older cached device registrations can still pass the settings API
        // but fail when the ASR backend starts routing audio.
        if let Some(ref creds) = self.credentials {
            if creds.is_complete() && creds.is_fresh() {
                tracing::info!("Using cached credentials");
                return Ok(creds.clone());
            }
            tracing::info!("Cached credentials are missing, old, or stale; refreshing");
        }

        const MAX_REGISTRATION_ATTEMPTS: usize = 5;
        let mut last_error = None;

        for attempt in 1..=MAX_REGISTRATION_ATTEMPTS {
            tracing::info!(
                "Registering new device (attempt {}/{})...",
                attempt,
                MAX_REGISTRATION_ATTEMPTS
            );
            let mut creds = DeviceCredentials::new_generated();

            let result = async {
                register_device(&mut creds).await?;
                get_asr_token(&mut creds).await?;
                creds.save(&self.credentials_path)?;
                Ok::<_, anyhow::Error>(creds)
            }
            .await;

            match result {
                Ok(creds) => {
                    tracing::info!("Credentials saved to {:?}", self.credentials_path);
                    return Ok(creds);
                }
                Err(error) => {
                    tracing::warn!("Device registration attempt {} failed: {}", attempt, error);
                    last_error = Some(error);
                    if attempt < MAX_REGISTRATION_ATTEMPTS {
                        tokio::time::sleep(Duration::from_millis(200 * attempt as u64)).await;
                    }
                }
            }
        }

        Err(anyhow!(
            "Failed to obtain usable ASR credentials after {} attempts: {}",
            MAX_REGISTRATION_ATTEMPTS,
            last_error
                .map(|error| error.to_string())
                .unwrap_or_else(|| "unknown error".to_string())
        ))
    }
}
