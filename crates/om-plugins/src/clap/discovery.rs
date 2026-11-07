//! Where CLAP plugins are looked for. Scanning runs plugin initialisers, so the standard system
//! folders are scanned only after the user accepted the first-run prompt (ADR 0007, decision 6).

use std::path::{Path, PathBuf};

/// File extension of a CLAP plugin.
pub const CLAP_EXTENSION: &str = "clap";

/// The folders to scan. `user_folder` is the plugin folder setting and is always scanned; the
/// standard CLAP folders only when `scan_standard_paths` is true.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClapSearch {
    pub user_folder: Option<PathBuf>,
    pub scan_standard_paths: bool,
}

impl ClapSearch {
    /// The standard CLAP folders for this platform given the user's home (`~/.clap`, `/usr/lib/clap`).
    pub fn standard_paths(home: Option<&Path>) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if cfg!(target_os = "linux") {
            paths.extend(home.map(|h| h.join(".clap")));
            paths.push(PathBuf::from("/usr/lib/clap"));
        }
        paths
    }

    /// Folders to scan, in order: the user folder, then the standard ones when allowed.
    pub fn folders(&self, home: Option<&Path>) -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = self.user_folder.iter().cloned().collect();
        if self.scan_standard_paths {
            folders.extend(Self::standard_paths(home));
        }
        folders
    }

    /// The `.clap` files directly in the folders (no recursion, no symlinks), sorted per folder.
    pub fn candidates(&self, home: Option<&Path>) -> Vec<PathBuf> {
        self.folders(home)
            .iter()
            .flat_map(|dir| clap_files(dir))
            .collect()
    }
}

/// The `.clap` files in `dir` (not recursive, no symlinks). A folder that does not exist has none.
pub fn clap_files(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == CLAP_EXTENSION))
        .collect();
    found.sort();
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_folders_are_scanned_only_after_consent() {
        let home = Path::new("/home/u");
        let mut search = ClapSearch {
            user_folder: Some(PathBuf::from("/plugins")),
            scan_standard_paths: false,
        };
        assert_eq!(search.folders(Some(home)), [PathBuf::from("/plugins")]);
        search.scan_standard_paths = true;
        let folders = search.folders(Some(home));
        if cfg!(target_os = "linux") {
            assert_eq!(
                folders,
                [
                    PathBuf::from("/plugins"),
                    PathBuf::from("/home/u/.clap"),
                    PathBuf::from("/usr/lib/clap")
                ]
            );
        }
    }

    #[test]
    fn only_clap_files_are_candidates() {
        let dir = tempfile::tempdir().expect("tempdir");
        for name in ["b.clap", "a.clap", "c.so", "d.txt"] {
            std::fs::write(dir.path().join(name), b"x").expect("write");
        }
        let names: Vec<_> = clap_files(dir.path())
            .iter()
            .map(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_owned()
            })
            .collect();
        assert_eq!(names, ["a.clap", "b.clap"]);
        assert!(clap_files(&dir.path().join("missing")).is_empty());
    }
}
