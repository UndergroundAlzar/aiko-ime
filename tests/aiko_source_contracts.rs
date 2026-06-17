use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

const REFERENCE_IDS: &[&str] = &[
    "master_source",
    "master_cutout",
    "expression_sheet",
    "head_angle_sheet",
    "motion_sheet",
];

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn manifest_path() -> PathBuf {
    repository_root().join("assets/live2d/aiko/source/production-manifest.json")
}

fn read_manifest() -> Value {
    serde_json::from_str(
        &fs::read_to_string(manifest_path()).expect("read Aiko production manifest"),
    )
    .expect("parse Aiko production manifest")
}

fn sha256_hex(path: &Path) -> String {
    let bytes =
        fs::read(path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    hex::encode(ring::digest::digest(&ring::digest::SHA256, &bytes))
}

#[test]
fn production_manifest_pins_all_modeling_references() {
    let root = repository_root();
    let manifest = read_manifest();

    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["model_id"], "aiko");
    assert_eq!(manifest["workspace_layout_version"], 1);
    assert_eq!(manifest["canvas"]["width"], 1024);
    assert_eq!(manifest["canvas"]["height"], 1536);

    let references = manifest["references"]
        .as_array()
        .expect("references should be an array");
    assert_eq!(references.len(), REFERENCE_IDS.len());
    for expected_id in REFERENCE_IDS {
        let reference = references
            .iter()
            .find(|entry| entry["id"] == *expected_id)
            .unwrap_or_else(|| panic!("missing reference: {expected_id}"));
        let source = reference["source"].as_str().expect("reference source");
        let path = root.join(source);
        assert!(path.is_file(), "reference is missing: {source}");
        assert_eq!(
            sha256_hex(&path),
            reference["sha256"].as_str().expect("reference SHA-256"),
            "reference content changed without updating the production manifest: {source}"
        );

        let image = image::open(&path)
            .unwrap_or_else(|error| panic!("failed to decode {}: {error}", path.display()));
        assert_eq!(image.width() as u64, reference["width"].as_u64().unwrap());
        assert_eq!(image.height() as u64, reference["height"].as_u64().unwrap());
    }

    let contract_names: Vec<_> = manifest["contracts"]
        .as_array()
        .expect("contracts should be an array")
        .iter()
        .map(|entry| {
            Path::new(entry["destination"].as_str().expect("contract destination"))
                .file_name()
                .expect("contract file name")
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    assert_eq!(
        contract_names,
        [
            "layers.csv",
            "parameters.csv",
            "hit-areas.csv",
            "motions.csv",
            "expressions.csv",
        ]
    );
}

#[test]
fn automation_contract_forbids_fabricated_model_assets() {
    let manifest = read_manifest();
    for capability in [
        "creates_layered_psd",
        "creates_cmo3",
        "creates_moc3",
        "creates_model3",
        "performs_art_separation",
        "performs_rigging",
        "certifies_cubism_authenticity",
    ] {
        assert_eq!(
            manifest["automation_limits"][capability], false,
            "automation capability must remain disabled: {capability}"
        );
    }
    for policy in [
        "allow_empty_or_renamed_psd",
        "allow_fake_cmo3",
        "allow_fake_moc3",
        "allow_handwritten_model3_as_editor_export",
    ] {
        assert_eq!(
            manifest["placeholder_policy"][policy], false,
            "placeholder policy must remain disabled: {policy}"
        );
    }

    let source = repository_root().join("assets/live2d/aiko/source");
    for entry in fs::read_dir(&source).expect("read Aiko source directory") {
        let path = entry.expect("read source entry").path();
        if path.is_file() {
            let name = path
                .file_name()
                .expect("file name")
                .to_string_lossy()
                .to_ascii_lowercase();
            assert!(
                !(name.ends_with(".psd")
                    || name.ends_with(".cmo3")
                    || name.ends_with(".moc3")
                    || name.ends_with(".model3.json")),
                "specification source contains a fabricated deliverable: {}",
                path.display()
            );
        }
    }
}

#[test]
fn production_guide_is_bilingual_and_assigns_cubism_work() {
    let guide = fs::read_to_string(repository_root().join("docs/AIKO_LIVE2D_PRODUCTION.md"))
        .expect("read bilingual production guide");
    for required in [
        "## 中文",
        "## English",
        "Prepare-AikoLive2DSource.ps1",
        "Test-AikoLive2DSource.ps1",
        "-RequireLayeredSource",
        "-RequireCubismProject",
        "-RequireRuntimeExport",
        "必须在 Live2D Cubism Editor 完成",
        "Work that must be done in Live2D Cubism Editor",
        "不会自动拆件",
        "does not separate artwork",
        "不能证明",
        "not authenticity certification",
    ] {
        assert!(
            guide.contains(required),
            "production guide is missing required text: {required}"
        );
    }
}

#[test]
fn prepare_then_validate_creates_only_workspace_scaffolding() {
    let root = repository_root();
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock")
        .as_nanos();
    let workspace =
        std::env::temp_dir().join(format!("aiko-live2d-source-{}-{nonce}", std::process::id()));
    if workspace.exists() {
        fs::remove_dir_all(&workspace).expect("remove stale test workspace");
    }

    let prepare = Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(root.join("scripts/Prepare-AikoLive2DSource.ps1"))
        .arg("-RepositoryRoot")
        .arg(&root)
        .arg("-WorkspaceRoot")
        .arg(&workspace)
        .output()
        .expect("run Aiko workspace preparation");
    assert!(
        prepare.status.success(),
        "workspace preparation failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&prepare.stdout),
        String::from_utf8_lossy(&prepare.stderr)
    );
    let first_manifest =
        fs::read(workspace.join("workspace-manifest.json")).expect("read first workspace manifest");
    let modeler_note = workspace.join("handoff/source/modeler-note.txt");
    fs::write(&modeler_note, b"preserve modeler-owned handoff files\n")
        .expect("write modeler-owned sentinel");

    let repeat_prepare = Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(root.join("scripts/Prepare-AikoLive2DSource.ps1"))
        .arg("-RepositoryRoot")
        .arg(&root)
        .arg("-WorkspaceRoot")
        .arg(&workspace)
        .output()
        .expect("repeat Aiko workspace preparation");
    assert!(
        repeat_prepare.status.success(),
        "repeated workspace preparation failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&repeat_prepare.stdout),
        String::from_utf8_lossy(&repeat_prepare.stderr)
    );
    assert_eq!(
        fs::read(workspace.join("workspace-manifest.json"))
            .expect("read repeated workspace manifest"),
        first_manifest,
        "repeated preparation should generate an identical workspace manifest"
    );
    assert_eq!(
        fs::read(&modeler_note).expect("read modeler-owned sentinel"),
        b"preserve modeler-owned handoff files\n",
        "preparation must preserve modeler-owned handoff files"
    );

    let validate = Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(root.join("scripts/Test-AikoLive2DSource.ps1"))
        .arg("-RepositoryRoot")
        .arg(&root)
        .arg("-WorkspaceRoot")
        .arg(&workspace)
        .output()
        .expect("run Aiko workspace validation");
    assert!(
        validate.status.success(),
        "workspace validation failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&validate.stdout),
        String::from_utf8_lossy(&validate.stderr)
    );

    let empty_cmo3 = workspace.join("handoff/ready/aiko.cmo3");
    fs::write(&empty_cmo3, []).expect("write empty CMO3 placeholder");
    let reject_placeholder = Command::new("pwsh")
        .args(["-NoProfile", "-File"])
        .arg(root.join("scripts/Test-AikoLive2DSource.ps1"))
        .arg("-RepositoryRoot")
        .arg(&root)
        .arg("-WorkspaceRoot")
        .arg(&workspace)
        .output()
        .expect("validate empty CMO3 placeholder");
    assert!(
        !reject_placeholder.status.success(),
        "workspace validation must reject an empty CMO3 placeholder"
    );
    assert!(
        String::from_utf8_lossy(&reject_placeholder.stderr)
            .contains("Empty placeholder artifact is forbidden"),
        "placeholder rejection should explain the forbidden empty artifact"
    );
    fs::remove_file(&empty_cmo3).expect("remove empty CMO3 placeholder");

    assert!(workspace.join("workspace-manifest.json").is_file());
    for directory in [
        "references",
        "contracts",
        "handoff/source/png/parts",
        "handoff/ready",
        "handoff/runtime",
        "review",
    ] {
        assert!(
            workspace.join(directory).is_dir(),
            "workspace directory is missing: {directory}"
        );
    }

    let mut stack = vec![workspace.clone()];
    while let Some(directory) = stack.pop() {
        for entry in fs::read_dir(&directory).expect("read generated workspace") {
            let path = entry.expect("read generated entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let name = path
                .file_name()
                .expect("generated file name")
                .to_string_lossy()
                .to_ascii_lowercase();
            assert!(
                !(name.ends_with(".psd")
                    || name.ends_with(".cmo3")
                    || name.ends_with(".moc3")
                    || name.ends_with(".model3.json")),
                "preparation fabricated a model deliverable: {}",
                path.display()
            );
        }
    }

    fs::remove_dir_all(&workspace).expect("remove test workspace");
}
