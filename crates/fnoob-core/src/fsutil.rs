use std::ffi::OsString;
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

pub fn temp_path_for(path: &Path) -> PathBuf {
    let mut name = path.file_name().map(OsString::from).unwrap_or_default();
    name.push(".tmp");
    path.with_file_name(name)
}

pub fn write_atomic(path: &Path, bytes: &[u8]) -> io::Result<()> {
    write_atomic_with(path, bytes, || Ok(()))
}

pub fn write_atomic_with(
    path: &Path,
    bytes: &[u8],
    before_replace: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)?;
    }
    let tmp = temp_path_for(path);
    let result = File::create(&tmp)
        .and_then(|mut file| {
            file.write_all(bytes)?;
            file.sync_all()
        })
        .and_then(|()| before_replace())
        .and_then(|()| fs::rename(&tmp, path));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir()
            .join("fnoob-core-tests")
            .join(format!("{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn temp_path_appends_suffix() {
        let p = Path::new("C:/x/project.json");
        assert_eq!(temp_path_for(p), Path::new("C:/x/project.json.tmp"));
    }

    #[test]
    fn write_atomic_creates_parent_and_replaces_existing() {
        let dir = scratch_dir("replace");
        let path = dir.join("nested").join("file.json");
        write_atomic(&path, b"first").unwrap();
        write_atomic(&path, b"second").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"second");
        assert!(!temp_path_for(&path).exists());
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_atomic_failure_keeps_old_file() {
        let dir = scratch_dir("fail");
        let path = dir.join("file.json");
        write_atomic(&path, b"original").unwrap();
        fs::create_dir_all(temp_path_for(&path)).unwrap();
        assert!(write_atomic(&path, b"broken").is_err());
        assert_eq!(fs::read(&path).unwrap(), b"original");
        fs::remove_dir_all(&dir).unwrap();
    }
}
