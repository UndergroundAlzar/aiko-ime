use std::io::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

fn main() -> Result<()> {
    use_bundled_protoc_if_available();

    // Compile protobuf files
    prost_build::compile_protos(&["proto/asr.proto"], &["proto/"])?;

    // Tell Cargo to rerun if the proto file changes
    println!("cargo:rerun-if-changed=proto/asr.proto");
    println!("cargo:rerun-if-changed=assets/aiko_app_icon.ico");

    embed_windows_icon();

    Ok(())
}

fn use_bundled_protoc_if_available() {
    if std::env::var_os("PROTOC").is_some() {
        return;
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let bundled = manifest_dir
        .join("tools")
        .join("protoc")
        .join("bin")
        .join(if cfg!(windows) {
            "protoc.exe"
        } else {
            "protoc"
        });
    if bundled.exists() {
        std::env::set_var("PROTOC", &bundled);
        println!("cargo:warning=using bundled protoc: {}", bundled.display());
    }
    println!("cargo:rerun-if-env-changed=PROTOC");
    println!("cargo:rerun-if-changed=tools/protoc/bin/protoc.exe");
}

fn embed_windows_icon() {
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    if target_os != "windows" {
        return;
    }

    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap_or_default());
    let icon_path = manifest_dir.join("assets").join("aiko_app_icon.ico");
    if !icon_path.exists() {
        println!("cargo:warning=app icon not found: {}", icon_path.display());
        return;
    }

    let out_dir = match std::env::var("OUT_DIR") {
        Ok(path) => PathBuf::from(path),
        Err(_) => return,
    };
    let rc_path = out_dir.join("aiko_icon.rc");
    let res_path = out_dir.join("aiko_icon.res");
    let icon_path = escape_rc_path(&icon_path);
    let rc_content = format!("1 ICON \"{}\"\n", icon_path);

    if let Err(e) = std::fs::write(&rc_path, rc_content) {
        println!("cargo:warning=failed to write icon resource script: {}", e);
        return;
    }

    let Some(rc_exe) = find_rc_exe() else {
        println!("cargo:warning=failed to find rc.exe for app icon");
        return;
    };

    match Command::new(rc_exe)
        .arg("/nologo")
        .arg(format!("/fo{}", res_path.display()))
        .arg(&rc_path)
        .status()
    {
        Ok(status) if status.success() => {
            println!("cargo:rustc-link-arg-bin=aiko-ime={}", res_path.display());
        }
        Ok(status) => {
            println!("cargo:warning=rc.exe exited with status: {}", status);
        }
        Err(e) => {
            println!("cargo:warning=failed to run rc.exe for app icon: {}", e);
        }
    }
}

fn escape_rc_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "\\\\")
}

fn find_rc_exe() -> Option<PathBuf> {
    if Command::new("rc.exe").arg("/?").output().is_ok() {
        return Some(PathBuf::from("rc.exe"));
    }

    let mut roots = Vec::new();
    if let Ok(path) = std::env::var("ProgramFiles(x86)") {
        roots.push(
            PathBuf::from(path)
                .join("Windows Kits")
                .join("10")
                .join("bin"),
        );
    }
    if let Ok(path) = std::env::var("ProgramFiles") {
        roots.push(
            PathBuf::from(path)
                .join("Windows Kits")
                .join("10")
                .join("bin"),
        );
    }

    let mut candidates = Vec::new();
    for root in roots {
        let Ok(entries) = std::fs::read_dir(root) else {
            continue;
        };
        for entry in entries.flatten() {
            let dir = entry.path();
            candidates.push(dir.join("x64").join("rc.exe"));
            candidates.push(dir.join("x86").join("rc.exe"));
        }
    }

    candidates.sort();
    candidates.into_iter().rev().find(|path| path.exists())
}
