use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use super::error::{io_error, Live2dError};

const MODEL_SUFFIX: &str = ".model3.json";

#[derive(Debug, Clone)]
pub struct ModelManifest {
    pub id: String,
    pub path: PathBuf,
    pub root: PathBuf,
    pub moc: PathBuf,
    pub textures: Vec<PathBuf>,
    pub motions: BTreeMap<String, Vec<MotionAsset>>,
    pub expressions: BTreeMap<String, ExpressionAsset>,
    pub physics: Option<PathBuf>,
    pub pose: Option<PathBuf>,
    pub user_data: Option<PathBuf>,
    pub display_info: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MotionAsset {
    pub group: String,
    pub index: usize,
    pub file: PathBuf,
    pub sound: Option<PathBuf>,
    pub fade_in_seconds: Option<f32>,
    pub fade_out_seconds: Option<f32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionAsset {
    pub id: String,
    pub file: PathBuf,
}

impl ModelManifest {
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Live2dError> {
        let requested_path = path.as_ref();
        validate_model_file_name(requested_path)?;

        let manifest_path =
            fs::canonicalize(requested_path).map_err(|source| io_error(requested_path, source))?;
        let root = manifest_path
            .parent()
            .ok_or_else(|| Live2dError::InvalidManifest {
                path: manifest_path.clone(),
                message: "manifest has no parent directory".to_string(),
            })?
            .to_path_buf();
        let bytes = fs::read(&manifest_path).map_err(|source| io_error(&manifest_path, source))?;
        let raw: RawModelManifest =
            serde_json::from_slice(&bytes).map_err(|source| Live2dError::Json {
                path: manifest_path.clone(),
                source,
            })?;

        if raw.version != 3 {
            return Err(invalid(
                &manifest_path,
                format!("unsupported Version {}; expected 3", raw.version),
            ));
        }

        let moc = resolve_asset(
            &root,
            &manifest_path,
            &raw.file_references.moc,
            ".moc3",
            false,
        )?;
        validate_moc_header(&moc)?;

        let textures = raw
            .file_references
            .textures
            .iter()
            .map(|path| resolve_asset(&root, &manifest_path, path, ".png", false))
            .collect::<Result<Vec<_>, _>>()?;
        if textures.is_empty() {
            return Err(invalid(
                &manifest_path,
                "FileReferences.Textures must contain at least one texture",
            ));
        }

        let mut expressions = BTreeMap::new();
        for expression in raw.file_references.expressions {
            if expression.name.trim().is_empty() {
                return Err(invalid(&manifest_path, "expression Name must not be empty"));
            }
            let file = resolve_asset(&root, &manifest_path, &expression.file, ".exp3.json", true)?;
            let id = expression.name;
            if expressions
                .insert(
                    id.clone(),
                    ExpressionAsset {
                        id: id.clone(),
                        file,
                    },
                )
                .is_some()
            {
                return Err(invalid(
                    &manifest_path,
                    format!("duplicate expression Name '{id}'"),
                ));
            }
        }

        let mut motions = BTreeMap::new();
        for (group, entries) in raw.file_references.motions {
            if group.trim().is_empty() {
                return Err(invalid(
                    &manifest_path,
                    "motion group name must not be empty",
                ));
            }

            let mut assets = Vec::with_capacity(entries.len());
            for (index, motion) in entries.into_iter().enumerate() {
                let file =
                    resolve_asset(&root, &manifest_path, &motion.file, ".motion3.json", true)?;
                let sound = motion
                    .sound
                    .as_deref()
                    .map(|path| resolve_asset(&root, &manifest_path, path, "", false))
                    .transpose()?;
                assets.push(MotionAsset {
                    group: group.clone(),
                    index,
                    file,
                    sound,
                    fade_in_seconds: motion.fade_in_time,
                    fade_out_seconds: motion.fade_out_time,
                });
            }
            motions.insert(group, assets);
        }

        let physics = resolve_optional_json(
            &root,
            &manifest_path,
            raw.file_references.physics.as_deref(),
            ".physics3.json",
        )?;
        let pose = resolve_optional_json(
            &root,
            &manifest_path,
            raw.file_references.pose.as_deref(),
            ".pose3.json",
        )?;
        let user_data = resolve_optional_json(
            &root,
            &manifest_path,
            raw.file_references.user_data.as_deref(),
            ".userdata3.json",
        )?;
        let display_info = resolve_optional_json(
            &root,
            &manifest_path,
            raw.file_references.display_info.as_deref(),
            ".cdi3.json",
        )?;

        validate_unique_paths(&manifest_path, &textures)?;

        let file_name = manifest_path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("model.model3.json");
        let id = file_name[..file_name.len() - MODEL_SUFFIX.len()].to_string();

        Ok(Self {
            id,
            path: manifest_path,
            root,
            moc,
            textures,
            motions,
            expressions,
            physics,
            pose,
            user_data,
            display_info,
        })
    }

    pub fn motion(&self, group: &str, index: usize) -> Option<&MotionAsset> {
        self.motions
            .get(group)
            .and_then(|entries| entries.get(index))
    }

    pub fn expression(&self, id: &str) -> Option<&ExpressionAsset> {
        self.expressions.get(id)
    }
}

fn validate_model_file_name(path: &Path) -> Result<(), Live2dError> {
    let valid = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase().ends_with(MODEL_SUFFIX))
        .unwrap_or(false);
    if valid {
        Ok(())
    } else {
        Err(invalid(path, "file name must end with .model3.json"))
    }
}

fn resolve_optional_json(
    root: &Path,
    manifest: &Path,
    relative: Option<&str>,
    suffix: &str,
) -> Result<Option<PathBuf>, Live2dError> {
    relative
        .filter(|path| !path.trim().is_empty())
        .map(|path| resolve_asset(root, manifest, path, suffix, true))
        .transpose()
}

fn resolve_asset(
    root: &Path,
    manifest: &Path,
    relative: &str,
    suffix: &str,
    validate_json: bool,
) -> Result<PathBuf, Live2dError> {
    let relative = relative.trim();
    if relative.is_empty() {
        return Err(invalid(
            manifest,
            format!("required {suffix} asset path is empty"),
        ));
    }

    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(Live2dError::AssetOutsideModel {
            manifest: manifest.to_path_buf(),
            asset: relative_path.to_path_buf(),
        });
    }

    if !suffix.is_empty() && !relative.to_ascii_lowercase().ends_with(suffix) {
        return Err(invalid(
            manifest,
            format!("asset '{relative}' must end with {suffix}"),
        ));
    }

    let candidate = root.join(relative_path);
    if !candidate.is_file() {
        return Err(Live2dError::MissingAsset {
            manifest: manifest.to_path_buf(),
            asset: candidate,
        });
    }
    let metadata = fs::metadata(&candidate).map_err(|source| io_error(&candidate, source))?;
    if metadata.len() == 0 {
        return Err(invalid(
            manifest,
            format!("asset '{}' is empty", candidate.display()),
        ));
    }
    let resolved = fs::canonicalize(&candidate).map_err(|source| io_error(&candidate, source))?;
    if !resolved.starts_with(root) {
        return Err(Live2dError::AssetOutsideModel {
            manifest: manifest.to_path_buf(),
            asset: resolved,
        });
    }

    if validate_json {
        validate_cubism_json(&resolved)?;
    }
    Ok(resolved)
}

fn validate_moc_header(path: &Path) -> Result<(), Live2dError> {
    let bytes = fs::read(path).map_err(|source| io_error(path, source))?;
    if !bytes.starts_with(b"MOC3") {
        return Err(invalid(
            path,
            "file does not start with the Cubism MOC3 header",
        ));
    }
    Ok(())
}

fn validate_cubism_json(path: &Path) -> Result<(), Live2dError> {
    let bytes = fs::read(path).map_err(|source| io_error(path, source))?;
    let value: Value = serde_json::from_slice(&bytes).map_err(|source| Live2dError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    let version = value.get("Version").and_then(Value::as_u64);
    if version != Some(3) {
        return Err(invalid(
            path,
            format!(
                "referenced Cubism JSON has Version {:?}; expected 3",
                version
            ),
        ));
    }
    Ok(())
}

fn validate_unique_paths(manifest: &Path, paths: &[PathBuf]) -> Result<(), Live2dError> {
    let mut unique = BTreeSet::new();
    for path in paths {
        if !unique.insert(path) {
            return Err(invalid(
                manifest,
                format!("duplicate texture reference '{}'", path.display()),
            ));
        }
    }
    Ok(())
}

fn invalid(path: &Path, message: impl Into<String>) -> Live2dError {
    Live2dError::InvalidManifest {
        path: path.to_path_buf(),
        message: message.into(),
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawModelManifest {
    version: u32,
    file_references: RawFileReferences,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "PascalCase")]
struct RawFileReferences {
    moc: String,
    textures: Vec<String>,
    physics: Option<String>,
    pose: Option<String>,
    user_data: Option<String>,
    display_info: Option<String>,
    expressions: Vec<RawExpression>,
    motions: BTreeMap<String, Vec<RawMotion>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawExpression {
    name: String,
    file: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct RawMotion {
    file: String,
    sound: Option<String>,
    fade_in_time: Option<f32>,
    fade_out_time: Option<f32>,
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::ModelManifest;
    use crate::ui::live2d::test_support::{write_minimal_model, TestDir};

    #[test]
    fn loads_and_resolves_real_cubism_manifest_assets() {
        let temp = TestDir::new("manifest-valid");
        let manifest_path = write_minimal_model(temp.path());

        let manifest = ModelManifest::load(&manifest_path).expect("valid model manifest");

        assert_eq!(manifest.id, "aiko");
        assert!(manifest.moc.ends_with("aiko.moc3"));
        assert_eq!(manifest.textures.len(), 1);
        assert!(manifest.motion("Idle", 0).is_some());
        assert!(manifest.expression("smile").is_some());
    }

    #[test]
    fn rejects_asset_path_escape() {
        let temp = TestDir::new("manifest-escape");
        fs::write(temp.path().join("outside.moc3"), b"moc").unwrap();
        let model_dir = temp.path().join("model");
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(model_dir.join("texture.png"), b"png").unwrap();
        fs::write(
            model_dir.join("aiko.model3.json"),
            r#"{
                "Version": 3,
                "FileReferences": {
                    "Moc": "../outside.moc3",
                    "Textures": ["texture.png"]
                }
            }"#,
        )
        .unwrap();

        let error = ModelManifest::load(model_dir.join("aiko.model3.json")).unwrap_err();
        assert!(error.to_string().contains("escapes the model directory"));
    }

    #[test]
    fn rejects_missing_referenced_motion() {
        let temp = TestDir::new("manifest-missing-motion");
        let manifest_path = write_minimal_model(temp.path());
        fs::remove_file(temp.path().join("idle.motion3.json")).unwrap();

        let error = ModelManifest::load(manifest_path).unwrap_err();
        assert!(error.to_string().contains("idle.motion3.json"));
        assert!(error.to_string().contains("missing"));
    }
}
