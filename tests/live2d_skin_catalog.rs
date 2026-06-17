use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn resolve_inside_assets(base: &Path, relative: &str) -> PathBuf {
    let mut path = base.to_path_buf();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(value) => path.push(value),
            Component::ParentDir => {
                assert!(path.pop(), "path escapes filesystem root: {relative}");
            }
            Component::CurDir => {}
            _ => panic!("unsupported path component: {relative}"),
        }
    }
    let assets = repository_root().join("assets").canonicalize().unwrap();
    let path = path.canonicalize().unwrap();
    assert!(path.starts_with(assets), "path escapes assets: {relative}");
    path
}

#[test]
fn six_fixed_skins_have_real_art_references_and_reserved_runtime_entries() {
    let live2d_root = repository_root().join("assets/live2d");
    let catalog: Value =
        serde_json::from_str(&fs::read_to_string(live2d_root.join("skin-catalog.json")).unwrap())
            .unwrap();
    assert_eq!(catalog["schema_version"], 1);
    assert_eq!(catalog["fixed_builtin_only"], true);
    assert_eq!(catalog["allow_arbitrary_model_import"], false);

    let skins = catalog["skins"].as_array().unwrap();
    assert_eq!(skins.len(), 6);
    let mut config_ids = BTreeSet::new();
    let mut model_ids = BTreeSet::new();
    let mut runtime_entries = BTreeSet::new();

    for skin in skins {
        assert!(config_ids.insert(skin["config_id"].as_str().unwrap()));
        assert!(model_ids.insert(skin["model_id"].as_str().unwrap()));
        assert!(runtime_entries.insert(skin["runtime_entry"].as_str().unwrap()));
        assert_eq!(skin["art_status"], "flat_reference_ready");
        assert_eq!(skin["cubism_runtime_status"], "missing");

        for field in ["source_image", "cutout_image"] {
            let path = resolve_inside_assets(&live2d_root, skin[field].as_str().unwrap());
            let image = image::open(&path)
                .unwrap_or_else(|error| panic!("failed to decode {}: {error}", path.display()));
            assert!(image.width() >= 900);
            assert!(image.height() >= 1500);
            if field == "cutout_image" {
                assert!(image.color().has_alpha(), "{} needs alpha", path.display());
            }
        }

        let runtime = skin["runtime_entry"].as_str().unwrap();
        assert!(runtime.ends_with(".model3.json"));
        assert!(!runtime.contains(".."));
    }
}
