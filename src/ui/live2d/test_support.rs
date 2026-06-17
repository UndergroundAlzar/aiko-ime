use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) struct TestDir {
    path: PathBuf,
}

impl TestDir {
    pub(crate) fn new(label: &str) -> Self {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "aiko-cubism-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("create test directory");
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

pub(crate) fn write_minimal_model(root: &Path) -> PathBuf {
    fs::create_dir_all(root).unwrap();
    fs::write(root.join("aiko.moc3"), b"MOC3-not-used-by-manifest-tests").unwrap();
    fs::write(root.join("texture.png"), b"png").unwrap();
    fs::write(
        root.join("idle.motion3.json"),
        r#"{"Version":3,"Meta":{"Duration":1.0},"Curves":[]}"#,
    )
    .unwrap();
    fs::write(
        root.join("smile.exp3.json"),
        r#"{"Version":3,"Type":"Live2D Expression","Parameters":[]}"#,
    )
    .unwrap();
    let manifest = root.join("aiko.model3.json");
    fs::write(
        &manifest,
        r#"{
            "Version": 3,
            "FileReferences": {
                "Moc": "aiko.moc3",
                "Textures": ["texture.png"],
                "Expressions": [
                    {"Name": "smile", "File": "smile.exp3.json"}
                ],
                "Motions": {
                    "Idle": [
                        {
                            "File": "idle.motion3.json",
                            "FadeInTime": 0.5,
                            "FadeOutTime": 0.5
                        }
                    ]
                }
            }
        }"#,
    )
    .unwrap();
    manifest
}
