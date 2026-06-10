use std::fs;
use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_repository_file(relative_path: &str) -> String {
    fs::read_to_string(repository_root().join(relative_path))
        .unwrap_or_else(|error| panic!("{relative_path} should be readable: {error}"))
}

fn collect_files(directory: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(directory).expect("read asset directory") {
        let path = entry.expect("read asset entry").path();
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

#[test]
fn config_template_parses_and_has_safe_hotkey_defaults() {
    let root = repository_root();
    let source = fs::read_to_string(root.join("config.toml.example"))
        .expect("config.toml.example should be readable");
    let config: toml::Value = toml::from_str(&source).expect("config template should parse");
    let hotkey = config
        .get("hotkey")
        .and_then(toml::Value::as_table)
        .expect("config template should contain [hotkey]");
    let mode = hotkey
        .get("mode")
        .and_then(toml::Value::as_str)
        .expect("hotkey.mode should be a string");
    let double_tap_interval = hotkey
        .get("double_tap_interval")
        .and_then(toml::Value::as_integer)
        .expect("hotkey.double_tap_interval should be an integer");
    let double_tap_key = hotkey
        .get("double_tap_key")
        .and_then(toml::Value::as_str)
        .expect("hotkey.double_tap_key should be a string");
    let combo_key = hotkey
        .get("combo_key")
        .and_then(toml::Value::as_str)
        .expect("hotkey.combo_key should be a string");

    assert!(
        matches!(mode, "double_tap" | "combo"),
        "unsupported hotkey mode: {mode}"
    );
    assert!(
        (100..=1_000).contains(&double_tap_interval),
        "double-tap interval should remain usable"
    );
    assert!(!double_tap_key.trim().is_empty());
    assert!(!combo_key.trim().is_empty());
}

#[test]
fn every_png_asset_decodes_and_is_nonempty() {
    let assets = repository_root().join("assets");
    let mut files = Vec::new();
    collect_files(&assets, &mut files);
    assert!(!files.is_empty(), "assets directory should not be empty");

    for path in files
        .iter()
        .filter(|path| path.extension().is_some_and(|ext| ext == "png"))
    {
        let image = image::open(path)
            .unwrap_or_else(|error| panic!("failed to decode {}: {error}", path.display()));
        assert!(
            image.width() > 0 && image.height() > 0,
            "{} has invalid dimensions",
            path.display()
        );
    }
}

#[test]
fn app_icon_has_an_ico_header() {
    let icon = fs::read(repository_root().join("assets/aiko_app_icon.ico"))
        .expect("app icon should be readable");
    assert!(icon.len() > 6, "app icon is unexpectedly small");
    assert_eq!(&icon[0..4], &[0, 0, 1, 0], "invalid ICO header");
}

#[test]
fn release_documents_are_bilingual_and_reference_existing_assets() {
    let root = repository_root();
    let readme = read_repository_file("README.md");
    let release_notes = read_repository_file("RELEASE_NOTES.md");

    for (name, document) in [("README.md", &readme), ("RELEASE_NOTES.md", &release_notes)] {
        assert!(
            document.contains("中文") || document.contains("功能"),
            "{name} should contain Chinese documentation"
        );
        assert!(
            document.contains("English") || document.contains("Features"),
            "{name} should contain English documentation"
        );
    }

    let showcase = "assets/aiko_readme_showcase.png";
    assert!(readme.contains(showcase));
    assert!(root.join(showcase).is_file());
}

#[test]
fn release_documents_cover_v1_3_through_v1_5() {
    let readme = read_repository_file("README.md");
    let release_notes = read_repository_file("RELEASE_NOTES.md");

    for version in ["v1.3", "v1.4", "v1.5"] {
        assert!(
            readme.contains(version),
            "README.md should describe the {version} release line"
        );
        assert!(
            release_notes.contains(version),
            "RELEASE_NOTES.md should describe the {version} release line"
        );
    }

    for heading in [
        "## v1.5.0",
        "## v1.4.0",
        "## v1.3.0",
        "### 中文",
        "### English",
    ] {
        assert!(
            release_notes.contains(heading),
            "RELEASE_NOTES.md should contain heading '{heading}'"
        );
    }
}

#[test]
fn portable_layout_contract_requires_runtime_files_and_assets() {
    let manifest: serde_json::Value =
        serde_json::from_str(&read_repository_file("tests/portable-layout.json"))
            .expect("portable layout manifest should parse as JSON");
    let files = manifest["requiredFiles"]
        .as_array()
        .expect("requiredFiles should be an array");
    let directories = manifest["requiredDirectories"]
        .as_array()
        .expect("requiredDirectories should be an array");

    for required in [
        "aiko-ime.exe",
        "config.toml",
        "README.md",
        "RELEASE_NOTES.md",
        "LICENSE",
        "VERSION.txt",
    ] {
        assert!(
            files.iter().any(|value| value == required),
            "portable manifest should require {required}"
        );
    }

    assert!(
        directories.iter().any(|value| value == "assets"),
        "portable manifest should require the assets directory"
    );

    let script = read_repository_file("scripts/Test-PortablePackage.ps1");
    for fragment in [
        "$Manifest.requiredFiles",
        "$Manifest.requiredDirectories",
        "aiko-ime.exe",
        "assets",
    ] {
        assert!(
            script.contains(fragment),
            "portable validation script should check '{fragment}'"
        );
    }
}

#[test]
fn ci_quality_gate_runs_format_lint_test_build_package_and_smoke() {
    let workflow = read_repository_file(".github/workflows/build.yml");
    let ordered_steps = [
        "cargo fmt --all -- --check",
        "cargo clippy --locked --all-targets --all-features",
        "cargo test --locked --all-targets",
        "cargo build --locked --release --target x86_64-pc-windows-msvc",
        "./scripts/build-portable.ps1 -SkipBuild",
        "./scripts/Test-WindowsSmoke.ps1",
    ];

    let mut previous = 0;
    for step in ordered_steps {
        let index = workflow
            .find(step)
            .unwrap_or_else(|| panic!("workflow should run '{step}'"));
        assert!(
            index >= previous,
            "workflow should run '{step}' after the previous quality gate"
        );
        previous = index;
    }

    for fragment in [
        "actions/upload-artifact@v4",
        "softprops/action-gh-release@v2",
        "dist/aiko-ime-v*-portable.zip",
        "dist/aiko-ime-v*-portable.zip.sha256",
    ] {
        assert!(
            workflow.contains(fragment),
            "workflow should contain release fragment '{fragment}'"
        );
    }
}
