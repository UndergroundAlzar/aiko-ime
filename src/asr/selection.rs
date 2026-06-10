//! ASR provider selection helpers.

use std::path::PathBuf;
use std::sync::Arc;

use crate::data::AsrConfig;
use crate::offline::{ModelManager, SherpaOnnxConfig, SherpaOnnxProvider};

use super::{
    AsrClient, AsrProvider, DoubaoProvider, ProviderCapabilities, ProviderError, ProviderKind,
};

/// Parse the configured backend/provider names without constructing a provider.
pub fn select_provider_kind(config: &AsrConfig) -> Result<ProviderKind, ProviderError> {
    match config.asr_backend_name().as_str() {
        "online" => match normalize_provider_name(&config.online_provider).as_str() {
            "doubao" => Ok(ProviderKind::Doubao),
            provider => Err(ProviderError::InvalidConfiguration(format!(
                "不支持的在线 ASR provider / unsupported online ASR provider: {provider}"
            ))),
        },
        "offline" => match normalize_provider_name(&config.offline_provider).as_str() {
            "sherpa-onnx" | "sherpa" => Ok(ProviderKind::SherpaOnnx),
            provider => Err(ProviderError::InvalidConfiguration(format!(
                "不支持的离线 ASR provider / unsupported offline ASR provider: {provider}"
            ))),
        },
        backend => Err(ProviderError::InvalidConfiguration(format!(
            "不支持的 ASR backend / unsupported ASR backend: {backend}"
        ))),
    }
}

/// Build the configured provider.
///
/// Online mode keeps using the existing Doubao client. Offline mode validates
/// the model package before returning a sherpa-onnx provider; the native DLL is
/// still loaded lazily when a session starts and only in sherpa-enabled builds.
pub fn build_provider_from_config(
    config: &AsrConfig,
    doubao_client: Arc<AsrClient>,
) -> Result<Arc<dyn AsrProvider>, ProviderError> {
    match select_provider_kind(config)? {
        ProviderKind::Doubao => Ok(Arc::new(DoubaoProvider::new(doubao_client))),
        ProviderKind::SherpaOnnx => {
            let model = load_offline_model(&config.offline_model_dir)?;
            let sherpa_config = SherpaOnnxConfig::new(model)?;
            Ok(Arc::new(SherpaOnnxProvider::new(sherpa_config)?))
        }
    }
}

/// Validate and load the offline model selected by configuration.
pub fn load_offline_model(model_dir: &str) -> Result<crate::offline::ResolvedModel, ProviderError> {
    let model_dir = model_dir.trim();
    if model_dir.is_empty() {
        return ModelManager::standard().find_first();
    }

    ModelManager::load_dir(PathBuf::from(model_dir))
}

pub fn provider_capabilities(kind: ProviderKind) -> ProviderCapabilities {
    match kind {
        ProviderKind::Doubao => ProviderCapabilities {
            streaming: true,
            endpoint_detection: true,
            accepts_opus: true,
            accepts_pcm: false,
            requires_network: true,
        },
        ProviderKind::SherpaOnnx => ProviderCapabilities {
            streaming: true,
            endpoint_detection: true,
            accepts_opus: false,
            accepts_pcm: true,
            requires_network: false,
        },
    }
}

trait AsrConfigNameExt {
    fn asr_backend_name(&self) -> String;
}

impl AsrConfigNameExt for AsrConfig {
    fn asr_backend_name(&self) -> String {
        normalize_provider_name(&self.backend)
    }
}

fn normalize_provider_name(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace('_', "-")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::asr::DeviceCredentials;
    use std::fs;
    use std::io::Write;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "aiko-ime-selection-{name}-{}",
                uuid::Uuid::new_v4()
            ));
            fs::create_dir_all(&path).unwrap();
            Self(path)
        }

        fn touch(&self, name: &str) {
            let mut file = fs::File::create(self.0.join(name)).unwrap();
            file.write_all(b"test").unwrap();
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn doubao_client() -> Arc<AsrClient> {
        Arc::new(AsrClient::new(DeviceCredentials {
            device_id: "3594726794116969".to_string(),
            install_id: "3594726794121065".to_string(),
            cdid: "cdid".to_string(),
            openudid: "openudid".to_string(),
            clientudid: "clientudid".to_string(),
            token: "token".to_string(),
            created_at_ms: 1,
        }))
    }

    #[test]
    fn online_defaults_select_doubao_without_model_validation() {
        let config = AsrConfig::default();

        assert_eq!(select_provider_kind(&config).unwrap(), ProviderKind::Doubao);
        let provider = build_provider_from_config(&config, doubao_client()).unwrap();

        assert_eq!(provider.kind(), ProviderKind::Doubao);
        assert!(provider.capabilities().requires_network);
    }

    #[test]
    fn offline_aliases_select_sherpa_onnx() {
        let mut config = AsrConfig::default();
        config.backend = " offline ".to_string();
        config.offline_provider = "sherpa_onnx".to_string();

        assert_eq!(
            select_provider_kind(&config).unwrap(),
            ProviderKind::SherpaOnnx
        );
        assert!(!provider_capabilities(ProviderKind::SherpaOnnx).requires_network);
    }

    #[test]
    fn offline_provider_reports_missing_model_path() {
        let mut config = AsrConfig::default();
        config.backend = "offline".to_string();
        config.offline_model_dir = std::env::temp_dir()
            .join(format!("aiko-ime-missing-model-{}", uuid::Uuid::new_v4()))
            .display()
            .to_string();

        let error = match build_provider_from_config(&config, doubao_client()) {
            Ok(provider) => panic!("missing model unexpectedly built {:?}", provider.kind()),
            Err(error) => error,
        };

        assert!(matches!(error, ProviderError::ModelUnavailable(_)));
        let message = error.to_string();
        assert!(message.contains("sherpa-onnx model directory"));
    }

    #[test]
    fn offline_provider_builds_after_model_validation() {
        let dir = TestDir::new("valid-model");
        for file in ["encoder.onnx", "decoder.onnx", "joiner.onnx", "tokens.txt"] {
            dir.touch(file);
        }

        let mut config = AsrConfig::default();
        config.backend = "offline".to_string();
        config.offline_model_dir = dir.0.display().to_string();

        let provider = build_provider_from_config(&config, doubao_client()).unwrap();

        assert_eq!(provider.kind(), ProviderKind::SherpaOnnx);
        let capabilities = provider.capabilities();
        assert!(capabilities.accepts_pcm);
        assert!(!capabilities.accepts_opus);
    }

    #[test]
    fn unsupported_provider_name_is_rejected() {
        let mut config = AsrConfig::default();
        config.online_provider = "mystery".to_string();

        let error = select_provider_kind(&config).unwrap_err();

        assert!(matches!(error, ProviderError::InvalidConfiguration(_)));
        assert!(error
            .to_string()
            .contains("unsupported online ASR provider"));
    }
}
