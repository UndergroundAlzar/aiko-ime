use std::error::Error;
use std::fmt;
use std::path::{Path, PathBuf};

use super::protocol::BuiltinModel;

const CORE_RELATIVE_PATH: &str = "vendor/live2dcubismcore.min.js";
const SDK_ADAPTER_RELATIVE_PATH: &str = "vendor/aiko-cubism-web-sdk.js";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeLayout {
    pub repository_root: PathBuf,
    pub runtime_root: PathBuf,
    pub index_html: PathBuf,
    pub runtime_script: PathBuf,
    pub sdk_loader_script: PathBuf,
    pub core_script: PathBuf,
    pub sdk_adapter_script: PathBuf,
    pub model3_json: PathBuf,
}

impl RuntimeLayout {
    pub fn from_repository_root(root: impl Into<PathBuf>) -> Self {
        let repository_root = root.into();
        let runtime_root = repository_root.join("assets/live2d/web_runtime");
        Self {
            index_html: runtime_root.join("index.html"),
            runtime_script: runtime_root.join("runtime.js"),
            sdk_loader_script: runtime_root.join("sdk-loader.js"),
            core_script: runtime_root.join(CORE_RELATIVE_PATH),
            sdk_adapter_script: runtime_root.join(SDK_ADAPTER_RELATIVE_PATH),
            model3_json: repository_root.join(BuiltinModel::Aiko.model3_relative_path()),
            repository_root,
            runtime_root,
        }
    }

    pub fn bundled() -> Self {
        Self::from_repository_root(env!("CARGO_MANIFEST_DIR"))
    }

    pub fn validate(&self) -> Result<(), RuntimeError> {
        self.validate_for(BuiltinModel::Aiko)
    }

    pub fn validate_for(&self, model: BuiltinModel) -> Result<(), RuntimeError> {
        let public_shell = [
            ("runtime HTML", self.index_html.as_path()),
            ("runtime JavaScript", self.runtime_script.as_path()),
            ("SDK loader", self.sdk_loader_script.as_path()),
        ];
        for (label, path) in public_shell {
            require_file(label, path)?;
        }

        if !self.core_script.is_file() {
            return Err(RuntimeError::CoreUnavailable {
                expected: self.core_script.clone(),
            });
        }
        if !self.sdk_adapter_script.is_file() {
            return Err(RuntimeError::SdkAdapterUnavailable {
                expected: self.sdk_adapter_script.clone(),
            });
        }
        let model3_json = self.repository_root.join(model.model3_relative_path());
        if !model3_json.is_file() {
            return Err(RuntimeError::BuiltinModelUnavailable {
                model,
                expected: model3_json,
            });
        }
        Ok(())
    }
}

fn require_file(label: &'static str, path: &Path) -> Result<(), RuntimeError> {
    if path.is_file() {
        Ok(())
    } else {
        Err(RuntimeError::ShellAssetMissing {
            label,
            expected: path.to_path_buf(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeError {
    ShellAssetMissing {
        label: &'static str,
        expected: PathBuf,
    },
    CoreUnavailable {
        expected: PathBuf,
    },
    SdkAdapterUnavailable {
        expected: PathBuf,
    },
    BuiltinModelUnavailable {
        model: BuiltinModel,
        expected: PathBuf,
    },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ShellAssetMissing { label, expected } => {
                write!(formatter, "Live2D web {label} is missing: {}", expected.display())
            }
            Self::CoreUnavailable { expected } => write!(
                formatter,
                "Live2D Cubism Core for Web is unavailable. Obtain the official Cubism SDK for Web from Live2D and place its unmodified live2dcubismcore.min.js at {}. Core is proprietary and is intentionally not bundled in this repository",
                expected.display()
            ),
            Self::SdkAdapterUnavailable { expected } => write!(
                formatter,
                "the private Cubism Web Framework adapter bundle is unavailable: {}. Build it from the official Cubism SDK for Web; this repository does not copy or emulate the Framework",
                expected.display()
            ),
            Self::BuiltinModelUnavailable { model, expected } => write!(
                formatter,
                "the fixed built-in Aiko Cubism model {model:?} is unavailable: {}. A real Cubism export containing model3.json, moc3, textures, and referenced assets is required",
                expected.display()
            ),
        }
    }
}

impl Error for RuntimeError {}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{RuntimeError, RuntimeLayout};

    struct TestDir {
        path: std::path::PathBuf,
    }

    impl TestDir {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("aiko-live2d-web-{}-{nonce}", std::process::id()));
            fs::create_dir_all(path.join("assets/live2d/web_runtime")).unwrap();
            Self { path }
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn bundled_public_shell_is_present_but_private_core_is_not_faked() {
        let layout = RuntimeLayout::bundled();
        assert!(layout.index_html.is_file());
        assert!(layout.runtime_script.is_file());
        assert!(layout.sdk_loader_script.is_file());

        let error = layout.validate().unwrap_err();
        assert!(matches!(error, RuntimeError::CoreUnavailable { .. }));
        let message = error.to_string();
        assert!(message.contains("official Cubism SDK for Web"));
        assert!(message.contains("intentionally not bundled"));
    }

    #[test]
    fn validation_stops_at_missing_public_shell_before_sdk_checks() {
        let temp = TestDir::new();
        let error = RuntimeLayout::from_repository_root(&temp.path)
            .validate()
            .unwrap_err();
        assert!(matches!(
            error,
            RuntimeError::ShellAssetMissing {
                label: "runtime HTML",
                ..
            }
        ));
    }
}
