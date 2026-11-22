use crate::fsutil::write_atomic;
use crate::settings::SettingsError;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const STANDARD_VST3_FOLDER: &str = r"C:\Program Files\Common Files\VST3";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanClass {
    pub class_id: String,
    pub name: String,
    pub vendor: String,
    pub instrument: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScanReport {
    pub path: String,
    pub classes: Vec<ScanClass>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub path: String,
    pub class: ScanClass,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FailedPlugin {
    pub path: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Catalog {
    pub plugins: Vec<CatalogEntry>,
    pub failed: Vec<FailedPlugin>,
}

impl Catalog {
    pub fn load(path: &Path) -> Result<Self, SettingsError> {
        match std::fs::read(path) {
            Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), SettingsError> {
        write_atomic(path, &serde_json::to_vec_pretty(self)?)?;
        Ok(())
    }

    pub fn find(&self, class_id: &str) -> Option<&CatalogEntry> {
        self.plugins
            .iter()
            .find(|e| e.class.class_id.eq_ignore_ascii_case(class_id))
    }

    pub fn add_report(&mut self, report: ScanReport) {
        self.plugins.retain(|e| e.path != report.path);
        self.failed.retain(|f| f.path != report.path);
        match report.error {
            Some(reason) => self.failed.push(FailedPlugin {
                path: report.path,
                reason,
            }),
            None if report.classes.is_empty() => self.failed.push(FailedPlugin {
                path: report.path,
                reason: "no audio plugins inside".into(),
            }),
            None => {
                for class in report.classes {
                    self.plugins.push(CatalogEntry {
                        path: report.path.clone(),
                        class,
                    });
                }
            }
        }
        self.plugins.sort_by(|a, b| {
            a.class
                .name
                .to_lowercase()
                .cmp(&b.class.name.to_lowercase())
        });
    }
}

pub fn parse_report(line: &str) -> Result<ScanReport, String> {
    serde_json::from_str(line).map_err(|e| format!("unreadable scan result: {e}"))
}

pub fn default_catalog_path() -> Option<PathBuf> {
    crate::settings::default_settings_path().map(|p| p.with_file_name("plugins.json"))
}

pub fn find_vst3(folders: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut stack: Vec<PathBuf> = folders.to_vec();
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let is_vst3 = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("vst3"));
            if is_vst3 {
                found.push(path);
            } else if path.is_dir() {
                stack.push(path);
            }
        }
    }
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn class(id: &str, name: &str) -> ScanClass {
        ScanClass {
            class_id: id.into(),
            name: name.into(),
            vendor: "V".into(),
            instrument: false,
        }
    }

    #[test]
    fn reports_replace_older_results_for_the_same_file() {
        let mut catalog = Catalog::default();
        catalog.add_report(ScanReport {
            path: "a.vst3".into(),
            classes: vec![class("AA", "zeta"), class("BB", "Alpha")],
            error: None,
        });
        assert_eq!(catalog.plugins[0].class.name, "Alpha");
        catalog.add_report(ScanReport {
            path: "a.vst3".into(),
            classes: vec![],
            error: Some("crashed".into()),
        });
        assert!(catalog.plugins.is_empty());
        assert_eq!(catalog.failed.len(), 1);
        catalog.add_report(ScanReport {
            path: "a.vst3".into(),
            classes: vec![class("CC", "Gamma")],
            error: None,
        });
        assert!(catalog.failed.is_empty());
        assert!(catalog.find("cc").is_some());
    }

    #[test]
    fn finds_bundles_and_files_without_entering_bundles() {
        let root = std::env::temp_dir().join(format!("fnoob-catalog-{}", std::process::id()));
        let bundle = root.join("Vendor").join("Verb.vst3");
        std::fs::create_dir_all(bundle.join("Contents").join("x86_64-win")).unwrap();
        std::fs::write(
            bundle.join("Contents").join("x86_64-win").join("Verb.vst3"),
            b"",
        )
        .unwrap();
        std::fs::write(root.join("Delay.vst3"), b"").unwrap();
        std::fs::write(root.join("readme.txt"), b"").unwrap();
        let found = find_vst3(std::slice::from_ref(&root));
        assert_eq!(found, [root.join("Delay.vst3"), bundle]);
        std::fs::remove_dir_all(root).unwrap();
    }
}
