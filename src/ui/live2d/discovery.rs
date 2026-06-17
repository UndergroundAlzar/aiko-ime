use std::fs;
use std::path::{Path, PathBuf};

use super::error::{io_error, Live2dError};
use super::manifest::ModelManifest;

#[derive(Debug, Default)]
pub struct DiscoveryReport {
    pub models: Vec<ModelManifest>,
    pub failures: Vec<DiscoveryFailure>,
}

#[derive(Debug)]
pub struct DiscoveryFailure {
    pub path: PathBuf,
    pub error: Live2dError,
}

pub fn discover_models<I, P>(roots: I) -> DiscoveryReport
where
    I: IntoIterator<Item = P>,
    P: AsRef<Path>,
{
    let mut report = DiscoveryReport::default();
    for root in roots {
        scan(root.as_ref(), &mut report);
    }
    report
        .models
        .sort_by(|left, right| left.path.cmp(&right.path));
    report
        .models
        .dedup_by(|left, right| left.path == right.path);
    report
        .failures
        .sort_by(|left, right| left.path.cmp(&right.path));
    report
}

fn scan(path: &Path, report: &mut DiscoveryReport) {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(source) => {
            report.failures.push(DiscoveryFailure {
                path: path.to_path_buf(),
                error: io_error(path, source),
            });
            return;
        }
    };
    if metadata.file_type().is_symlink() {
        return;
    }
    if metadata.is_file() {
        if is_model_manifest(path) {
            match ModelManifest::load(path) {
                Ok(model) => report.models.push(model),
                Err(error) => report.failures.push(DiscoveryFailure {
                    path: path.to_path_buf(),
                    error,
                }),
            }
        }
        return;
    }
    if !metadata.is_dir() {
        return;
    }

    let entries = match fs::read_dir(path) {
        Ok(entries) => entries,
        Err(source) => {
            report.failures.push(DiscoveryFailure {
                path: path.to_path_buf(),
                error: io_error(path, source),
            });
            return;
        }
    };
    let mut paths = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .collect::<Vec<_>>();
    paths.sort();
    for child in paths {
        scan(&child, report);
    }
}

fn is_model_manifest(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase().ends_with(".model3.json"))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::discover_models;
    use crate::ui::live2d::test_support::{write_minimal_model, TestDir};

    #[test]
    fn discovery_keeps_valid_models_and_reports_invalid_ones() {
        let temp = TestDir::new("discovery");
        let valid = temp.path().join("valid");
        fs::create_dir_all(&valid).unwrap();
        write_minimal_model(&valid);

        let invalid = temp.path().join("invalid");
        fs::create_dir_all(&invalid).unwrap();
        fs::write(invalid.join("broken.model3.json"), b"{not-json").unwrap();

        let report = discover_models([temp.path()]);

        assert_eq!(report.models.len(), 1);
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].path.ends_with("broken.model3.json"));
    }
}
