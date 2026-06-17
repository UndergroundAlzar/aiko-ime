use serde_json::Value;
use std::fs;
use std::path::{Component, Path, PathBuf};

const PNG_FALLBACK_ASSETS: &[&str] = &[
    "assets/aiko_desktop_pet.png",
    "assets/mascot/states/aiko_listening.png",
    "assets/mascot/states/aiko_processing.png",
    "assets/mascot/states/aiko_success.png",
    "assets/mascot/states/aiko_error.png",
    "assets/mascot/states/aiko_petted.png",
    "assets/mascot/states/aiko_sleepy.png",
];

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn collect_files_with_suffix(directory: &Path, suffix: &str, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(directory) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files_with_suffix(&path, suffix, files);
        } else if path
            .file_name()
            .is_some_and(|name| name.to_string_lossy().ends_with(suffix))
        {
            files.push(path);
        }
    }
}

fn referenced_path(model_path: &Path, relative: &str) -> PathBuf {
    assert!(
        !relative.trim().is_empty(),
        "{} contains an empty Cubism file reference",
        model_path.display()
    );
    let relative_path = Path::new(relative);
    assert!(
        !relative_path.is_absolute()
            && !relative_path
                .components()
                .any(|component| matches!(component, Component::ParentDir)),
        "{} contains an unsafe Cubism file reference: {relative}",
        model_path.display()
    );
    model_path
        .parent()
        .expect("model3 descriptor should have a parent directory")
        .join(relative_path)
}

fn assert_reference_exists(model_path: &Path, relative: &str) {
    let path = referenced_path(model_path, relative);
    assert!(
        path.is_file(),
        "{} references a missing file: {relative}",
        model_path.display()
    );
    assert!(
        fs::metadata(&path)
            .unwrap_or_else(|error| panic!("failed to inspect {}: {error}", path.display()))
            .len()
            > 0,
        "{} references an empty file: {relative}",
        model_path.display()
    );
}

fn validate_model3_descriptor(model_path: &Path) {
    let source = fs::read_to_string(model_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", model_path.display()));
    let model: Value = serde_json::from_str(&source)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", model_path.display()));
    let version = model["Version"]
        .as_u64()
        .expect("Cubism model3 descriptor should contain a numeric Version");
    assert!(
        version >= 3,
        "{} should use Cubism model3 schema version 3 or newer",
        model_path.display()
    );

    let references = model["FileReferences"]
        .as_object()
        .expect("Cubism model3 descriptor should contain FileReferences");
    let moc = references
        .get("Moc")
        .and_then(Value::as_str)
        .expect("Cubism FileReferences.Moc should be a string");
    assert!(
        moc.to_ascii_lowercase().ends_with(".moc3"),
        "{} should reference a .moc3 file",
        model_path.display()
    );
    assert_reference_exists(model_path, moc);

    let moc_path = referenced_path(model_path, moc);
    let moc_bytes = fs::read(&moc_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", moc_path.display()));
    assert!(
        moc_bytes.starts_with(b"MOC3"),
        "{} does not have a Cubism MOC3 header",
        moc_path.display()
    );

    let textures = references
        .get("Textures")
        .and_then(Value::as_array)
        .expect("Cubism FileReferences.Textures should be an array");
    assert!(
        !textures.is_empty(),
        "{} should reference at least one texture",
        model_path.display()
    );
    for texture in textures {
        let texture = texture
            .as_str()
            .expect("Cubism texture references should be strings");
        assert!(
            texture.to_ascii_lowercase().ends_with(".png"),
            "{} contains a non-PNG Cubism texture: {texture}",
            model_path.display()
        );
        assert_reference_exists(model_path, texture);
    }

    for key in ["Physics", "Pose", "DisplayInfo", "UserData"] {
        if let Some(relative) = references.get(key).and_then(Value::as_str) {
            assert_reference_exists(model_path, relative);
        }
    }

    if let Some(expressions) = references.get("Expressions").and_then(Value::as_array) {
        for expression in expressions {
            let relative = expression["File"]
                .as_str()
                .expect("Cubism expression entries should contain File");
            assert_reference_exists(model_path, relative);
        }
    }

    if let Some(motion_groups) = references.get("Motions").and_then(Value::as_object) {
        for motions in motion_groups.values() {
            for motion in motions
                .as_array()
                .expect("Cubism motion groups should contain arrays")
            {
                let relative = motion["File"]
                    .as_str()
                    .expect("Cubism motion entries should contain File");
                assert_reference_exists(model_path, relative);
                if let Some(sound) = motion.get("Sound").and_then(Value::as_str) {
                    assert_reference_exists(model_path, sound);
                }
            }
        }
    }
}

#[test]
fn png_fallback_assets_are_always_available() {
    let root = repository_root();
    for relative in PNG_FALLBACK_ASSETS {
        let path = root.join(relative);
        assert!(path.is_file(), "PNG fallback asset is missing: {relative}");
        let image = image::open(&path)
            .unwrap_or_else(|error| panic!("failed to decode {}: {error}", path.display()));
        assert!(
            image.width() > 0 && image.height() > 0,
            "PNG fallback asset has invalid dimensions: {relative}"
        );
    }
}

#[test]
fn cubism_feature_is_explicitly_gated() {
    let cargo_source =
        fs::read_to_string(repository_root().join("Cargo.toml")).expect("read Cargo.toml");
    let cargo: toml::Value = toml::from_str(&cargo_source).expect("parse Cargo.toml");
    let features = cargo
        .get("features")
        .and_then(toml::Value::as_table)
        .expect("Cargo.toml should contain [features]");
    let cubism_features: Vec<_> = features
        .keys()
        .filter(|name| {
            let name = name.to_ascii_lowercase();
            name.contains("cubism") || name.contains("live2d")
        })
        .collect();
    assert_eq!(
        cubism_features.len(),
        1,
        "Cargo.toml should define exactly one explicit Cubism/Live2D feature"
    );

    let defaults = features
        .get("default")
        .and_then(toml::Value::as_array)
        .expect("features.default should be an array");
    assert!(
        !defaults.iter().any(|value| {
            value
                .as_str()
                .is_some_and(|name| name == cubism_features[0].as_str())
        }),
        "the Cubism feature must remain opt-in so builds work without the SDK"
    );
}

#[test]
fn bundled_cubism_models_are_complete_or_png_fallback_is_documented() {
    let root = repository_root();
    let manifest_path = root
        .join("assets")
        .join("live2d")
        .join("aiko")
        .join("asset-manifest.json");
    let manifest_source =
        fs::read_to_string(&manifest_path).expect("read Live2D asset-manifest.json");
    let manifest: Value =
        serde_json::from_str(&manifest_source).expect("parse Live2D asset-manifest.json");
    let runtime_status = manifest["stages"]["runtime"]["status"]
        .as_str()
        .expect("Live2D runtime stage should have a status");
    let model_root = root
        .join("assets")
        .join("live2d")
        .join("aiko")
        .join("runtime");
    let mut descriptors = Vec::new();
    collect_files_with_suffix(&model_root, ".model3.json", &mut descriptors);

    if runtime_status != "ready" {
        assert!(
            descriptors.is_empty(),
            "runtime stage is '{runtime_status}' but contains a model3 descriptor"
        );
        let readme = fs::read_to_string(root.join("README.md")).expect("read README.md");
        assert!(
            readme.contains("PNG fallback") && readme.contains("未"),
            "README must state that a real Cubism model is not bundled and PNG fallback is used"
        );
        return;
    }

    assert!(
        !descriptors.is_empty(),
        "runtime stage is ready but contains no model3 descriptor"
    );
    for descriptor in descriptors {
        validate_model3_descriptor(&descriptor);
    }
}

#[test]
fn cag_operator_skin_contract_is_explicit_and_fixed() {
    let root = repository_root();
    let cag_root = root.join("assets").join("live2d").join("aiko_cag");
    let manifest_path = cag_root.join("asset-manifest.json");
    let manifest_source =
        fs::read_to_string(&manifest_path).expect("read CAG Live2D asset manifest");
    let manifest: Value =
        serde_json::from_str(&manifest_source).expect("parse CAG Live2D asset manifest");

    assert_eq!(manifest["model_id"], "aiko_cag");
    assert_eq!(manifest["stages"]["source"]["status"], "concept_only");
    assert_eq!(manifest["stages"]["runtime"]["status"], "missing");
    assert_eq!(manifest["fixed_builtin_model"], true);
    assert_eq!(manifest["allow_arbitrary_model_import"], false);

    let layer_source = fs::read_to_string(cag_root.join("source").join("layers-addon.csv"))
        .expect("read CAG layer addendum");
    for required in [
        "gpnvg_tube_outer_l",
        "gpnvg_tube_inner_l",
        "gpnvg_tube_inner_r",
        "gpnvg_tube_outer_r",
        "glove_l",
        "glove_r",
        "rifle_sling_l",
        "rifle_sling_r",
        "ks1_rifle",
        "patch_aiko",
        "patch_b_rh_negative",
    ] {
        assert!(
            layer_source.contains(required),
            "CAG layer contract is missing {required}"
        );
    }

    let modeling_master = root
        .join("assets")
        .join("aiko_art")
        .join("modeling")
        .join("aiko_cag_master_cutout.png");
    let image = image::open(&modeling_master).expect("decode CAG modeling master");
    assert!(image.width() > 0 && image.height() > 0);

    let specification = fs::read_to_string(root.join("docs").join("LIVE2D_MODELING.md"))
        .expect("read Live2D modeling guide");
    for invariant in ["四个独立镜筒", "全指手套", "枪口朝下", "B RH-"] {
        assert!(
            specification.contains(invariant),
            "CAG modeling guide is missing invariant: {invariant}"
        );
    }
}
