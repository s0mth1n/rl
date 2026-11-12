//! Persistent application rows validated by XDG directory modification times.
use crate::{config::Config, items::App};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

#[derive(Serialize, Deserialize)]
struct Cache {
    version: u32,
    entries: Vec<App>,
    dir_mtimes: BTreeMap<PathBuf, (u64, u32)>,
}

fn directory_mtimes() -> BTreeMap<PathBuf, (u64, u32)> {
    crate::icons::data_dirs()
        .into_iter()
        .filter_map(|base| {
            let dir = base.join("applications");
            directory_signature(&dir).map(|signature| (dir, signature))
        })
        .collect()
}

/// Newest mtime among the directory and every .desktop file inside it.
/// A directory's own mtime only tracks entry creation and removal, so
/// in-place edits of existing desktop files must contribute their own.
fn directory_signature(dir: &Path) -> Option<(u64, u32)> {
    let mut newest = mtime(std::fs::metadata(dir).ok()?)?;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(d) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(kind) = entry.file_type() else {
                continue;
            };
            if kind.is_dir() {
                pending.push(entry.path());
            } else if entry.path().extension().is_some_and(|ext| ext == "desktop") {
                if let Some(t) = entry.metadata().ok().and_then(mtime) {
                    if t > newest {
                        newest = t;
                    }
                }
            }
        }
    }
    Some(newest)
}

fn mtime(metadata: std::fs::Metadata) -> Option<(u64, u32)> {
    let modified = metadata.modified().ok()?;
    let elapsed = modified.duration_since(UNIX_EPOCH).unwrap_or_default();
    Some((elapsed.as_secs(), elapsed.subsec_nanos()))
}

fn cache_path() -> Option<PathBuf> {
    std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|base| base.join("zl/apps.json"))
}

pub fn load(cfg: &Config) -> Vec<App> {
    let Some(path) = cache_path() else {
        return crate::items::scan_apps(cfg);
    };
    load_at(&path, directory_mtimes(), || crate::items::scan_apps(cfg))
}

fn load_at(
    path: &Path,
    dir_mtimes: BTreeMap<PathBuf, (u64, u32)>,
    scan: impl FnOnce() -> Vec<App>,
) -> Vec<App> {
    if let Some(cache) = std::fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice::<Cache>(&data).ok())
    {
        if cache.version == 1 && cache.dir_mtimes == dir_mtimes {
            return cache.entries;
        }
    }
    let mut entries = scan();
    let themes = crate::icons::theme_chain("");
    for app in &mut entries {
        if let Some(path) = crate::icons::find(&app.icon, &themes) {
            app.icon = path.to_string_lossy().into_owned();
        }
    }
    let cache = Cache {
        version: 1,
        entries,
        dir_mtimes,
    };
    let save = || -> Result<(), Box<dyn std::error::Error>> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, serde_json::to_vec_pretty(&cache)?)?;
        Ok(())
    };
    if let Err(error) = save() {
        eprintln!("zl: failed to save application cache: {error}");
    }
    cache.entries
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app(name: &str) -> App {
        App {
            name: name.into(),
            comment: "description".into(),
            exec: "true %U".into(),
            icon: String::new(),
            terminal: true,
        }
    }
    #[test]
    fn cache_hit_miss_version_corruption_and_directory_changes() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("zl/apps.json");
        let dirs = BTreeMap::from([(PathBuf::from("/apps"), (1, 2))]);
        assert_eq!(
            load_at(&path, dirs.clone(), || vec![app("First")]),
            [app("First")]
        );
        assert_eq!(
            load_at(&path, dirs.clone(), || panic!("cache hit must not scan")),
            [app("First")]
        );
        let changed = BTreeMap::from([(PathBuf::from("/apps"), (1, 3))]);
        assert_eq!(
            load_at(&path, changed.clone(), || vec![app("Modified")]),
            [app("Modified")]
        );
        let mut extra = changed.clone();
        extra.insert(PathBuf::from("/new"), (2, 0));
        assert_eq!(load_at(&path, extra, || vec![app("Added")]), [app("Added")]);
        assert_eq!(
            load_at(&path, changed.clone(), || vec![app("Removed")]),
            [app("Removed")]
        );
        let mut cache: Cache = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        cache.version = 99;
        std::fs::write(&path, serde_json::to_vec(&cache).unwrap()).unwrap();
        assert_eq!(
            load_at(&path, changed.clone(), || vec![app("Version")]),
            [app("Version")]
        );
        std::fs::write(&path, b"not json").unwrap();
        assert_eq!(
            load_at(&path, changed, || vec![app("Corruption")]),
            [app("Corruption")]
        );
    }
    #[test]
    fn failed_save_still_returns_scanned_rows() {
        let root = tempfile::tempdir().unwrap();
        let parent = root.path().join("file");
        std::fs::write(&parent, b"not directory").unwrap();
        assert_eq!(
            load_at(&parent.join("apps.json"), BTreeMap::new(), || vec![app(
                "Usable"
            )]),
            [app("Usable")]
        );
    }
}

#[test]
fn in_place_desktop_edit_changes_the_directory_signature() {
    let _env = crate::testutil::ENV_LOCK.lock().unwrap();
    let root = tempfile::tempdir().unwrap();
    let apps = root.path().join("data/applications");
    std::fs::create_dir_all(&apps).unwrap();
    let entry = apps.join("fixture.desktop");
    std::fs::write(&entry, "[Desktop Entry]\nName=One\nExec=one\n").unwrap();
    let old = std::env::var_os("XDG_DATA_HOME");
    std::env::set_var("XDG_DATA_HOME", root.path().join("data"));
    let before = directory_mtimes();
    // Editing the file in place must invalidate the cache even though the
    // directory's own mtime only tracks entry creation and removal.
    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&entry, "[Desktop Entry]\nName=Two\nExec=two\n").unwrap();
    let after = directory_mtimes();
    match old {
        Some(value) => std::env::set_var("XDG_DATA_HOME", value),
        None => std::env::remove_var("XDG_DATA_HOME"),
    }
    assert_ne!(before, after, "in-place edit went unnoticed");
}

#[cfg(test)]
mod icon_tests {
    use super::*;
    #[test]
    fn resolved_absolute_icon_survives_cache_hit() {
        let root = tempfile::tempdir().unwrap();
        let icon = root.path().join("fixture.svg");
        std::fs::write(&icon, "<svg xmlns='http://www.w3.org/2000/svg'/>").unwrap();
        let app = App {
            name: "Icon".into(),
            comment: String::new(),
            exec: "true".into(),
            icon: icon.to_string_lossy().into(),
            terminal: false,
        };
        let path = root.path().join("apps.json");
        assert_eq!(
            load_at(&path, BTreeMap::new(), || vec![app.clone()]).as_slice(),
            std::slice::from_ref(&app)
        );
        assert_eq!(
            load_at(&path, BTreeMap::new(), || panic!("must use stored icon")),
            [app]
        );
    }
}
