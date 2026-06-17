use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

const CONTRACT_FILES: &[&str] = &[
    "source/reference.json",
    "source/layers.csv",
    "source/layers-addon.csv",
    "source/parameters.csv",
    "source/motions.csv",
    "source/expressions.csv",
    "source/occlusion-redraw.csv",
];

type CsvRow = BTreeMap<String, String>;

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn cag_root() -> PathBuf {
    repository_root()
        .join("assets")
        .join("live2d")
        .join("aiko_cag")
}

fn parse_csv_line(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = line.chars().peekable();
    while let Some(character) = chars.next() {
        match character {
            '"' if quoted && chars.peek() == Some(&'"') => {
                field.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(field);
                field = String::new();
            }
            _ => field.push(character),
        }
    }
    assert!(!quoted, "unterminated quoted CSV field: {line}");
    fields.push(field);
    fields
}

fn read_csv(path: &Path, expected_headers: &[&str]) -> Vec<CsvRow> {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let mut lines = source.lines().filter(|line| !line.trim().is_empty());
    let headers = parse_csv_line(
        lines
            .next()
            .unwrap_or_else(|| panic!("CSV is empty: {}", path.display())),
    );
    assert_eq!(
        headers,
        expected_headers,
        "unexpected CSV headers in {}",
        path.display()
    );

    let rows: Vec<_> = lines
        .enumerate()
        .map(|(index, line)| {
            let values = parse_csv_line(line);
            assert_eq!(
                values.len(),
                headers.len(),
                "wrong field count in {} line {}",
                path.display(),
                index + 2
            );
            headers.iter().cloned().zip(values).collect()
        })
        .collect();
    assert!(!rows.is_empty(), "CSV has no data rows: {}", path.display());
    rows
}

fn value<'a>(row: &'a CsvRow, key: &str) -> &'a str {
    row.get(key)
        .unwrap_or_else(|| panic!("CSV row is missing column {key}"))
}

fn assert_unique(rows: &[CsvRow], key: &str) {
    let mut values = BTreeSet::new();
    for row in rows {
        let current = value(row, key);
        assert!(!current.is_empty(), "{key} must not be empty");
        assert!(values.insert(current), "duplicate {key}: {current}");
    }
}

fn row_by<'a>(rows: &'a [CsvRow], key: &str, expected: &str) -> &'a CsvRow {
    let matches: Vec<_> = rows
        .iter()
        .filter(|row| value(row, key) == expected)
        .collect();
    assert_eq!(
        matches.len(),
        1,
        "expected exactly one row with {key}={expected}"
    );
    matches[0]
}

fn tags(row: &CsvRow) -> BTreeSet<&str> {
    value(row, "contract_tags").split(';').collect()
}

fn sha256_hex(path: &Path) -> String {
    let bytes =
        fs::read(path).unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    hex::encode(ring::digest::digest(&ring::digest::SHA256, &bytes))
}

fn resolve_repository_reference(base: &Path, reference: &str) -> PathBuf {
    let relative = Path::new(reference);
    assert!(
        !relative.is_absolute(),
        "reference must be relative: {reference}"
    );
    let mut resolved = base.to_path_buf();
    for component in relative.components() {
        match component {
            Component::Normal(segment) => resolved.push(segment),
            Component::ParentDir => {
                assert!(
                    resolved.pop(),
                    "reference escapes filesystem root: {reference}"
                );
            }
            Component::CurDir => {}
            _ => panic!("unsupported reference component: {reference}"),
        }
    }
    let canonical = resolved
        .canonicalize()
        .unwrap_or_else(|error| panic!("failed to resolve {}: {error}", resolved.display()));
    assert!(
        canonical.starts_with(
            repository_root()
                .canonicalize()
                .expect("canonical repository root")
        ),
        "reference escapes repository: {reference}"
    );
    canonical
}

fn assert_no_fake_cubism_artifacts(path: &Path) {
    for entry in fs::read_dir(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
    {
        let entry = entry.expect("read CAG directory entry");
        let child = entry.path();
        if child.is_dir() {
            assert_no_fake_cubism_artifacts(&child);
            continue;
        }
        let name = child
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .to_ascii_lowercase();
        assert!(
            !(name.ends_with(".psd")
                || name.ends_with(".cmo3")
                || name.ends_with(".moc3")
                || name.ends_with(".model3.json")),
            "concept_only CAG tree contains a forbidden placeholder: {}",
            child.display()
        );
    }
}

#[test]
fn cag_manifest_and_reference_images_are_pinned() {
    let root = cag_root();
    let manifest: Value = serde_json::from_str(
        &fs::read_to_string(root.join("asset-manifest.json")).expect("read CAG manifest"),
    )
    .expect("parse CAG manifest");

    assert_eq!(manifest["schema_version"], 2);
    assert_eq!(manifest["model_id"], "aiko_cag");
    assert_eq!(manifest["stages"]["source"]["status"], "concept_only");
    assert_eq!(manifest["stages"]["ready"]["status"], "missing");
    assert_eq!(manifest["stages"]["runtime"]["status"], "missing");
    assert_eq!(manifest["fixed_builtin_model"], true);
    assert_eq!(manifest["allow_arbitrary_model_import"], false);
    assert_eq!(manifest["placeholder_policy"]["allow_fake_moc3"], false);
    assert_eq!(manifest["placeholder_policy"]["allow_fake_cmo3"], false);
    assert_eq!(
        manifest["placeholder_policy"]["allow_handwritten_model3"],
        false
    );
    assert_eq!(manifest["invariants"]["gpnvg_tube_count"], 4);
    assert_eq!(manifest["invariants"]["glove_coverage"], "full_finger");
    assert_eq!(manifest["invariants"]["weapon_model"], "KAC KS-1");
    assert_eq!(manifest["invariants"]["weapon_carry"], "chest_slung");
    assert_eq!(manifest["invariants"]["weapon_muzzle"], "down");
    assert_eq!(
        manifest["invariants"]["chest_patches"],
        serde_json::json!(["AIKO", "B RH-"])
    );
    assert_eq!(manifest["invariants"]["allow_equipment_boom_mic"], true);
    assert_eq!(
        manifest["invariants"]["allow_microphone_icon_or_patch"],
        false
    );

    let listed: Vec<_> = manifest["stages"]["source"]["contract_files"]
        .as_array()
        .expect("source.contract_files array")
        .iter()
        .map(|entry| entry.as_str().expect("contract file string"))
        .collect();
    assert_eq!(listed, CONTRACT_FILES);
    for relative in CONTRACT_FILES {
        assert!(
            root.join(relative).is_file(),
            "manifest contract file is missing: {relative}"
        );
    }

    let reference_path = root.join("source").join("reference.json");
    let reference: Value = serde_json::from_str(
        &fs::read_to_string(&reference_path).expect("read CAG reference contract"),
    )
    .expect("parse CAG reference contract");
    assert_eq!(reference["canvas"]["width"], 1024);
    assert_eq!(reference["canvas"]["height"], 1536);

    for key in [
        "master",
        "cutout",
        "expression_sheet",
        "motion_sheet",
        "parts_reference",
    ] {
        let entry = &reference[key];
        let image_path = resolve_repository_reference(
            reference_path.parent().expect("reference parent"),
            entry["path"].as_str().expect("reference image path"),
        );
        let image = image::open(&image_path)
            .unwrap_or_else(|error| panic!("failed to decode {}: {error}", image_path.display()));
        let expected_width = entry["width"].as_u64().unwrap_or(1024) as u32;
        let expected_height = entry["height"].as_u64().unwrap_or(1536) as u32;
        assert_eq!(
            image.width(),
            expected_width,
            "{} width changed",
            image_path.display()
        );
        assert_eq!(
            image.height(),
            expected_height,
            "{} height changed",
            image_path.display()
        );
        assert_eq!(
            sha256_hex(&image_path),
            entry["sha256"].as_str().expect("reference SHA-256"),
            "{} content changed without updating reference.json",
            image_path.display()
        );
    }

    assert_no_fake_cubism_artifacts(&root);
}

#[test]
fn cag_layers_encode_the_visual_invariants() {
    const HEADERS: &[&str] = &[
        "sort",
        "layer_id",
        "psd_group",
        "png_file",
        "part",
        "side",
        "required",
        "physics",
        "occlusion_role",
        "visual_text",
        "contract_tags",
        "note_zh",
    ];
    let source = cag_root().join("source");
    let layers = read_csv(&source.join("layers.csv"), HEADERS);
    let addon = read_csv(&source.join("layers-addon.csv"), HEADERS);
    assert!(layers.len() >= 70);
    assert!(addon.len() >= 35);
    assert_unique(&layers, "layer_id");
    assert_unique(&layers, "png_file");
    assert_unique(&addon, "layer_id");

    for row in &layers {
        let id = value(row, "layer_id");
        assert_eq!(value(row, "png_file"), format!("png/parts/{id}.png"));
        assert!(matches!(value(row, "side"), "left" | "right" | "center"));
        assert!(matches!(value(row, "required"), "true" | "false"));
        assert!(matches!(value(row, "physics"), "true" | "false"));
    }
    for addon_row in &addon {
        assert_eq!(
            addon_row,
            row_by(&layers, "layer_id", value(addon_row, "layer_id")),
            "tactical addendum must be an exact subset of layers.csv"
        );
    }

    let tube_ids = [
        "gpnvg_tube_outer_l",
        "gpnvg_tube_inner_l",
        "gpnvg_tube_inner_r",
        "gpnvg_tube_outer_r",
    ];
    let required_tubes: Vec<_> = layers
        .iter()
        .filter(|row| value(row, "part") == "gpnvg_tube" && value(row, "required") == "true")
        .collect();
    assert_eq!(
        required_tubes.len(),
        4,
        "GPNVG must have exactly four tubes"
    );
    for id in tube_ids {
        assert!(tags(row_by(&layers, "layer_id", id)).contains("four_tube"));
    }

    for id in ["glove_l", "glove_r", "glove_fingers_l", "glove_fingers_r"] {
        assert!(
            tags(row_by(&layers, "layer_id", id)).contains("full_finger"),
            "{id} must specify full-finger coverage"
        );
    }

    let rifle = row_by(&layers, "layer_id", "ks1_rifle");
    let rifle_tags = tags(rifle);
    for required in ["KAC_KS-1", "chest_slung", "muzzle_down", "hands_clear"] {
        assert!(
            rifle_tags.contains(required),
            "rifle tag missing: {required}"
        );
    }

    let patches: Vec<_> = layers
        .iter()
        .filter(|row| value(row, "part") == "patch")
        .collect();
    assert_eq!(patches.len(), 2, "only two chest patches are allowed");
    let patch_text: BTreeSet<_> = patches
        .iter()
        .map(|row| value(row, "visual_text"))
        .collect();
    assert_eq!(patch_text, BTreeSet::from(["AIKO", "B RH-"]));

    let forbidden = [
        "mic_icon",
        "microphone_icon",
        "mic_patch",
        "microphone_patch",
        "麦克风图标",
        "麦克风贴章",
    ];
    for row in &layers {
        let visual_surface = format!(
            "{}|{}",
            value(row, "layer_id").to_ascii_lowercase(),
            value(row, "visual_text").to_ascii_lowercase()
        );
        for token in forbidden {
            assert!(
                !visual_surface.contains(&token.to_ascii_lowercase()),
                "forbidden microphone icon/patch token in {}: {token}",
                value(row, "layer_id")
            );
        }
    }
}

#[test]
fn cag_animation_and_redraw_contracts_are_closed() {
    let source = cag_root().join("source");
    let layers = read_csv(
        &source.join("layers.csv"),
        &[
            "sort",
            "layer_id",
            "psd_group",
            "png_file",
            "part",
            "side",
            "required",
            "physics",
            "occlusion_role",
            "visual_text",
            "contract_tags",
            "note_zh",
        ],
    );
    let layer_ids: BTreeSet<_> = layers.iter().map(|row| value(row, "layer_id")).collect();

    let parameters = read_csv(
        &source.join("parameters.csv"),
        &[
            "parameter_id",
            "min",
            "default",
            "max",
            "required",
            "driver",
            "bind_layers",
            "constraint_zh",
            "purpose_zh",
        ],
    );
    assert_unique(&parameters, "parameter_id");
    for row in &parameters {
        let minimum: f64 = value(row, "min").parse().expect("parameter minimum");
        let default: f64 = value(row, "default").parse().expect("parameter default");
        let maximum: f64 = value(row, "max").parse().expect("parameter maximum");
        assert!(minimum <= default && default <= maximum);
    }
    for id in [
        "ParamCagGpnvgDeploy",
        "ParamCagSlingL",
        "ParamCagSlingR",
        "ParamCagRifleX",
        "ParamCagRifleY",
        "ParamCagRadioCable",
        "ParamAikoListening",
    ] {
        row_by(&parameters, "parameter_id", id);
    }
    assert!(
        parameters
            .iter()
            .all(|row| value(row, "parameter_id") != "ParamAikoMicGlow"),
        "CAG must not reuse the microphone glow branding parameter"
    );

    let motions = read_csv(
        &source.join("motions.csv"),
        &[
            "group",
            "name",
            "file",
            "fade_in_sec",
            "fade_out_sec",
            "loop",
            "trigger",
            "return_state",
            "required",
            "hand_pose",
            "weapon_pose",
            "gpnvg_pose",
            "acceptance_zh",
        ],
    );
    assert_unique(&motions, "name");
    for row in &motions {
        assert!(value(row, "hand_pose").ends_with("_clear"));
        assert_eq!(value(row, "weapon_pose"), "chest_slung_muzzle_down");
    }
    for group in [
        "idle",
        "listening",
        "processing",
        "success",
        "error",
        "petted",
        "sleepy",
        "wave",
    ] {
        let count = motions
            .iter()
            .filter(|row| value(row, "group") == group && value(row, "required") == "true")
            .count();
        assert_eq!(count, 1, "required motion group missing: {group}");
    }

    let expressions = read_csv(
        &source.join("expressions.csv"),
        &[
            "name",
            "file",
            "fade_in_sec",
            "fade_out_sec",
            "required",
            "uses_icon",
            "patch_visibility",
            "purpose_zh",
        ],
    );
    assert_unique(&expressions, "name");
    for row in &expressions {
        assert_eq!(value(row, "uses_icon"), "false");
        assert_eq!(value(row, "patch_visibility"), "clear");
    }

    let redraw = read_csv(
        &source.join("occlusion-redraw.csv"),
        &[
            "redraw_id",
            "back_layer_id",
            "occluder_layer_ids",
            "region",
            "required",
            "deliverable",
            "acceptance_zh",
        ],
    );
    assert!(redraw.len() >= 15);
    assert_unique(&redraw, "redraw_id");
    for row in &redraw {
        assert!(
            layer_ids.contains(value(row, "back_layer_id")),
            "unknown redraw back layer: {}",
            value(row, "back_layer_id")
        );
        for occluder in value(row, "occluder_layer_ids").split(';') {
            assert!(
                layer_ids.contains(occluder),
                "unknown redraw occluder: {occluder}"
            );
        }
    }

    let checklist = fs::read_to_string(repository_root().join("docs/CAG_MODELING_CHECKLIST.md"))
        .expect("read CAG modeling checklist");
    for required in [
        "source/reference.json",
        "source/layers.csv",
        "source/layers-addon.csv",
        "source/parameters.csv",
        "source/motions.csv",
        "source/expressions.csv",
        "source/occlusion-redraw.csv",
        "四个独立镜筒",
        "全指手套",
        "KAC KS-1",
        "枪口始终朝下",
        "AIKO",
        "B RH-",
        "禁止麦克风图标",
    ] {
        assert!(
            checklist.contains(required),
            "checklist is missing contract text: {required}"
        );
    }
}
