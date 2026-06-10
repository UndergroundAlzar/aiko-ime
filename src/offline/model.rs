//! sherpa-onnx model package discovery and validation.

use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::asr::ProviderError;

pub const MODEL_MANIFEST_FILE: &str = "aiko-sherpa-model.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ModelFamily {
    OnlineTransducer,
    OnlineParaformer,
    OnlineZipformer2Ctc,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelManifest {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    pub name: String,
    pub family: ModelFamily,
    #[serde(default = "default_sample_rate")]
    pub sample_rate: u32,
    #[serde(default = "default_feature_dim")]
    pub feature_dim: u32,
    pub files: ManifestFiles,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestFiles {
    pub tokens: PathBuf,
    #[serde(default)]
    pub encoder: Option<PathBuf>,
    #[serde(default)]
    pub decoder: Option<PathBuf>,
    #[serde(default)]
    pub joiner: Option<PathBuf>,
    #[serde(default)]
    pub model: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedModel {
    pub name: String,
    pub root: PathBuf,
    pub family: ModelFamily,
    pub sample_rate: u32,
    pub feature_dim: u32,
    pub files: ResolvedModelFiles,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedModelFiles {
    Transducer {
        encoder: PathBuf,
        decoder: PathBuf,
        joiner: PathBuf,
        tokens: PathBuf,
    },
    Paraformer {
        encoder: PathBuf,
        decoder: PathBuf,
        tokens: PathBuf,
    },
    Zipformer2Ctc {
        model: PathBuf,
        tokens: PathBuf,
    },
}

impl ResolvedModel {
    pub fn validate(&self) -> Result<(), ProviderError> {
        if self.sample_rate == 0 {
            return Err(ProviderError::ModelUnavailable(format!(
                "模型 {} 的 sample_rate 必须大于 0 / sample_rate must be positive",
                self.name
            )));
        }
        if self.feature_dim == 0 {
            return Err(ProviderError::ModelUnavailable(format!(
                "模型 {} 的 feature_dim 必须大于 0 / feature_dim must be positive",
                self.name
            )));
        }

        let missing: Vec<PathBuf> = self
            .required_files()
            .into_iter()
            .filter(|path| !path.is_file())
            .collect();
        if missing.is_empty() {
            Ok(())
        } else {
            Err(ProviderError::ModelUnavailable(format!(
                "模型 {} 缺少文件 / missing files: {}",
                self.name,
                display_paths(&missing)
            )))
        }
    }

    pub fn required_files(&self) -> Vec<PathBuf> {
        match &self.files {
            ResolvedModelFiles::Transducer {
                encoder,
                decoder,
                joiner,
                tokens,
            } => vec![
                encoder.clone(),
                decoder.clone(),
                joiner.clone(),
                tokens.clone(),
            ],
            ResolvedModelFiles::Paraformer {
                encoder,
                decoder,
                tokens,
            } => vec![encoder.clone(), decoder.clone(), tokens.clone()],
            ResolvedModelFiles::Zipformer2Ctc { model, tokens } => {
                vec![model.clone(), tokens.clone()]
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct ModelDiscovery {
    pub models: Vec<ResolvedModel>,
    pub diagnostics: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ModelManager {
    roots: Vec<PathBuf>,
}

impl ModelManager {
    pub fn new(roots: impl IntoIterator<Item = PathBuf>) -> Self {
        let mut unique = Vec::new();
        for root in roots {
            if !unique.contains(&root) {
                unique.push(root);
            }
        }
        Self { roots: unique }
    }

    /// App-owned locations only; this intentionally does not scan arbitrary
    /// user directories.
    pub fn standard() -> Self {
        let mut roots = Vec::new();

        if let Some(path) = env::var_os("AIKO_IME_MODEL_DIR") {
            roots.push(PathBuf::from(path));
        }
        if let Ok(exe) = env::current_exe() {
            if let Some(dir) = exe.parent() {
                roots.push(dir.join("models"));
                roots.push(dir.join("models").join("sherpa-onnx"));
            }
        }
        if let Some(local_app_data) = env::var_os("LOCALAPPDATA") {
            roots.push(PathBuf::from(local_app_data).join("AikoIME").join("models"));
        }
        if let Ok(current_dir) = env::current_dir() {
            roots.push(current_dir.join("models"));
        }

        Self::new(roots)
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn discover(&self) -> ModelDiscovery {
        let mut discovery = ModelDiscovery::default();

        for root in &self.roots {
            if !root.exists() {
                discovery.diagnostics.push(format!(
                    "模型目录不存在 / model directory not found: {}",
                    root.display()
                ));
                continue;
            }

            inspect_candidate(root, &mut discovery);
            match fs::read_dir(root) {
                Ok(entries) => {
                    let mut child_dirs: Vec<PathBuf> = entries
                        .filter_map(Result::ok)
                        .map(|entry| entry.path())
                        .filter(|path| path.is_dir())
                        .collect();
                    child_dirs.sort();
                    for child in child_dirs {
                        inspect_candidate(&child, &mut discovery);
                    }
                }
                Err(error) => discovery.diagnostics.push(format!(
                    "无法读取模型目录 / cannot read model directory {}: {}",
                    root.display(),
                    error
                )),
            }
        }

        discovery
    }

    pub fn find_first(&self) -> Result<ResolvedModel, ProviderError> {
        let mut discovery = self.discover();
        if !discovery.models.is_empty() {
            return Ok(discovery.models.remove(0));
        }

        let searched = if self.roots.is_empty() {
            "<none>".to_string()
        } else {
            display_paths(&self.roots)
        };
        let diagnostics = if discovery.diagnostics.is_empty() {
            String::new()
        } else {
            format!("; {}", discovery.diagnostics.join("; "))
        };
        Err(ProviderError::ModelUnavailable(format!(
            "未找到可用 sherpa-onnx 流式模型 / no usable streaming model found; searched: {}{}",
            searched, diagnostics
        )))
    }

    pub fn load_dir(path: impl AsRef<Path>) -> Result<ResolvedModel, ProviderError> {
        load_candidate(path.as_ref())?.ok_or_else(|| {
            ProviderError::ModelUnavailable(format!(
                "{} 不是 sherpa-onnx 模型目录 / is not a sherpa-onnx model directory",
                path.as_ref().display()
            ))
        })
    }
}

fn inspect_candidate(path: &Path, discovery: &mut ModelDiscovery) {
    match load_candidate(path) {
        Ok(Some(model)) => {
            if !discovery
                .models
                .iter()
                .any(|existing| existing.root == model.root)
            {
                discovery.models.push(model);
            }
        }
        Ok(None) => {}
        Err(error) => discovery.diagnostics.push(error.to_string()),
    }
}

fn load_candidate(path: &Path) -> Result<Option<ResolvedModel>, ProviderError> {
    let manifest_path = path.join(MODEL_MANIFEST_FILE);
    if manifest_path.is_file() {
        return load_manifest(path, &manifest_path).map(Some);
    }

    load_conventional_transducer(path)
}

fn load_manifest(root: &Path, manifest_path: &Path) -> Result<ResolvedModel, ProviderError> {
    let bytes = fs::read(manifest_path).map_err(|error| {
        ProviderError::ModelUnavailable(format!(
            "无法读取模型清单 / cannot read manifest {}: {}",
            manifest_path.display(),
            error
        ))
    })?;
    let manifest: ModelManifest = serde_json::from_slice(&bytes).map_err(|error| {
        ProviderError::ModelUnavailable(format!(
            "模型清单 JSON 无效 / invalid model manifest {}: {}",
            manifest_path.display(),
            error
        ))
    })?;
    if manifest.schema_version != 1 {
        return Err(ProviderError::ModelUnavailable(format!(
            "不支持模型清单 schema_version={} / unsupported schema version in {}",
            manifest.schema_version,
            manifest_path.display()
        )));
    }

    let canonical_root = fs::canonicalize(root).map_err(|error| {
        ProviderError::ModelUnavailable(format!(
            "无法解析模型目录 / cannot resolve model directory {}: {}",
            root.display(),
            error
        ))
    })?;
    let resolve = |relative: &Path| resolve_model_file(&canonical_root, relative);
    let tokens = resolve(&manifest.files.tokens)?;

    let files = match manifest.family {
        ModelFamily::OnlineTransducer => ResolvedModelFiles::Transducer {
            encoder: resolve(required_path(&manifest.files.encoder, "files.encoder")?)?,
            decoder: resolve(required_path(&manifest.files.decoder, "files.decoder")?)?,
            joiner: resolve(required_path(&manifest.files.joiner, "files.joiner")?)?,
            tokens,
        },
        ModelFamily::OnlineParaformer => ResolvedModelFiles::Paraformer {
            encoder: resolve(required_path(&manifest.files.encoder, "files.encoder")?)?,
            decoder: resolve(required_path(&manifest.files.decoder, "files.decoder")?)?,
            tokens,
        },
        ModelFamily::OnlineZipformer2Ctc => ResolvedModelFiles::Zipformer2Ctc {
            model: resolve(required_path(&manifest.files.model, "files.model")?)?,
            tokens,
        },
    };

    let model = ResolvedModel {
        name: manifest.name,
        root: canonical_root,
        family: manifest.family,
        sample_rate: manifest.sample_rate,
        feature_dim: manifest.feature_dim,
        files,
    };
    model.validate()?;
    Ok(model)
}

fn required_path<'a>(value: &'a Option<PathBuf>, label: &str) -> Result<&'a Path, ProviderError> {
    value.as_deref().ok_or_else(|| {
        ProviderError::ModelUnavailable(format!(
            "模型清单缺少 {} / manifest field is required: {}",
            label, label
        ))
    })
}

fn resolve_model_file(root: &Path, relative: &Path) -> Result<PathBuf, ProviderError> {
    if relative.is_absolute() {
        return Err(ProviderError::ModelUnavailable(format!(
            "模型清单只能使用相对路径 / manifest paths must be relative: {}",
            relative.display()
        )));
    }
    let joined = root.join(relative);
    let resolved = fs::canonicalize(&joined).map_err(|error| {
        ProviderError::ModelUnavailable(format!(
            "模型文件不存在 / model file not found {}: {}",
            joined.display(),
            error
        ))
    })?;
    if !resolved.starts_with(root) {
        return Err(ProviderError::ModelUnavailable(format!(
            "模型文件越过模型目录 / model path escapes package root: {}",
            relative.display()
        )));
    }
    Ok(resolved)
}

fn load_conventional_transducer(root: &Path) -> Result<Option<ResolvedModel>, ProviderError> {
    if !root.is_dir() {
        return Ok(None);
    }

    let mut files: Vec<PathBuf> = fs::read_dir(root)
        .map_err(|error| {
            ProviderError::ModelUnavailable(format!(
                "无法读取模型目录 / cannot read model directory {}: {}",
                root.display(),
                error
            ))
        })?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();

    let onnx_files: Vec<&PathBuf> = files
        .iter()
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("onnx"))
        })
        .collect();
    if onnx_files.is_empty() {
        return Ok(None);
    }

    let find_component = |needle: &str| -> Option<PathBuf> {
        let mut matches: Vec<&PathBuf> = onnx_files
            .iter()
            .copied()
            .filter(|path| {
                path.file_name()
                    .and_then(|value| value.to_str())
                    .is_some_and(|value| value.to_ascii_lowercase().contains(needle))
            })
            .collect();
        matches.sort_by_key(|path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            (!name.contains("int8"), name.len())
        });
        matches.first().map(|path| (*path).clone())
    };

    let tokens = files.iter().find(|path| {
        path.file_name()
            .and_then(|value| value.to_str())
            .is_some_and(|value| value.eq_ignore_ascii_case("tokens.txt"))
    });
    let encoder = find_component("encoder");
    let decoder = find_component("decoder");
    let joiner = find_component("joiner");

    let mut missing = Vec::new();
    if encoder.is_none() {
        missing.push("*encoder*.onnx");
    }
    if decoder.is_none() {
        missing.push("*decoder*.onnx");
    }
    if joiner.is_none() {
        missing.push("*joiner*.onnx");
    }
    if tokens.is_none() {
        missing.push("tokens.txt");
    }
    if !missing.is_empty() {
        return Err(ProviderError::ModelUnavailable(format!(
            "模型目录不完整 / incomplete streaming transducer model {}: missing {}",
            root.display(),
            missing.join(", ")
        )));
    }

    let canonical_root = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let model = ResolvedModel {
        name: root
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or("sherpa-onnx-model")
            .to_string(),
        root: canonical_root,
        family: ModelFamily::OnlineTransducer,
        sample_rate: default_sample_rate(),
        feature_dim: default_feature_dim(),
        files: ResolvedModelFiles::Transducer {
            encoder: encoder.unwrap(),
            decoder: decoder.unwrap(),
            joiner: joiner.unwrap(),
            tokens: tokens.unwrap().clone(),
        },
    };
    model.validate()?;
    Ok(Some(model))
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

const fn default_schema_version() -> u32 {
    1
}

const fn default_sample_rate() -> u32 {
    16_000
}

const fn default_feature_dim() -> u32 {
    80
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    struct TestDir(PathBuf);

    impl TestDir {
        fn new(name: &str) -> Self {
            let path = env::temp_dir().join(format!("aiko-ime-{name}-{}", uuid::Uuid::new_v4()));
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

    #[test]
    fn discovers_conventional_transducer_and_prefers_int8() {
        let dir = TestDir::new("model-discovery");
        for file in [
            "encoder.onnx",
            "encoder.int8.onnx",
            "decoder.onnx",
            "joiner.onnx",
            "tokens.txt",
        ] {
            dir.touch(file);
        }

        let model = ModelManager::load_dir(&dir.0).unwrap();
        match model.files {
            ResolvedModelFiles::Transducer { encoder, .. } => {
                assert_eq!(
                    encoder.file_name().unwrap().to_string_lossy(),
                    "encoder.int8.onnx"
                );
            }
            _ => panic!("expected transducer"),
        }
    }

    #[test]
    fn incomplete_model_has_actionable_error() {
        let dir = TestDir::new("incomplete-model");
        dir.touch("encoder.onnx");

        let error = ModelManager::load_dir(&dir.0).unwrap_err().to_string();
        assert!(error.contains("decoder"));
        assert!(error.contains("joiner"));
        assert!(error.contains("tokens.txt"));
    }

    #[test]
    fn manifest_paths_cannot_escape_package() {
        let parent = TestDir::new("manifest-escape");
        let model_dir = parent.0.join("model");
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(parent.0.join("tokens.txt"), "token").unwrap();
        fs::write(model_dir.join("encoder.onnx"), "encoder").unwrap();
        fs::write(model_dir.join("decoder.onnx"), "decoder").unwrap();
        fs::write(model_dir.join("joiner.onnx"), "joiner").unwrap();
        fs::write(
            model_dir.join(MODEL_MANIFEST_FILE),
            r#"{
                "name": "bad",
                "family": "online-transducer",
                "files": {
                    "encoder": "encoder.onnx",
                    "decoder": "decoder.onnx",
                    "joiner": "joiner.onnx",
                    "tokens": "../tokens.txt"
                }
            }"#,
        )
        .unwrap();

        let error = ModelManager::load_dir(&model_dir).unwrap_err().to_string();
        assert!(error.contains("escapes package root") || error.contains("越过模型目录"));
    }
}
