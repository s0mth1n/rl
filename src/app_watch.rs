//! Desktop entry notifications wake the main loop without polling directories.
//!
//! Besides the applications directories that exist at startup, the watcher
//! also covers data directories themselves, data directories that do not
//! exist yet (through their nearest existing ancestor), and the parents of
//! symlinked path components, so installs into new directories and nix
//! profile switches are noticed without a restart.
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub struct AppWatch {
    watcher: RecommendedWatcher,
    events: UnixStream,
    pending: Option<Instant>,
    watched: HashSet<PathBuf>,
}

impl AppWatch {
    pub fn new() -> Result<Self, String> {
        let (events, mut send) = UnixStream::pair().map_err(|e| e.to_string())?;
        events.set_nonblocking(true).map_err(|e| e.to_string())?;
        send.set_nonblocking(true).map_err(|e| e.to_string())?;
        let watcher =
            notify::recommended_watcher(move |event: Result<Event, notify::Error>| match event {
                Ok(event) if relevant(&event) => {
                    let _ = send.write(&[1]);
                }
                Err(error) => eprintln!("zl: application watcher: {error}"),
                _ => {}
            })
            .map_err(|e| e.to_string())?;
        let mut this = Self {
            watcher,
            events,
            pending: None,
            watched: HashSet::new(),
        };
        this.re_arm();
        Ok(this)
    }
    pub fn fd(&self) -> RawFd {
        self.events.as_raw_fd()
    }
    pub fn changed(&mut self) -> bool {
        let mut bytes = [0; 256];
        let mut changed = false;
        while self.events.read(&mut bytes).is_ok_and(|n| n > 0) {
            changed = true;
        }
        if changed {
            // Installs can create directories this watcher skipped at
            // startup; pick them up before the reload looks at them.
            self.re_arm();
        }
        debounce(&mut self.pending, Instant::now(), changed)
    }
    pub fn wait_ms(&self) -> Option<i32> {
        self.pending.map(|first| {
            Duration::from_millis(500)
                .saturating_sub(first.elapsed())
                .as_millis() as i32
                + 1
        })
    }

    /// Watch every data directory and applications directory that exists but
    /// is not watched yet. Missing data directories are covered through
    /// their nearest existing ancestor, and symlinked path components (nix
    /// profiles) through their parent, since replacing the symlink
    /// retargets every path below it.
    fn re_arm(&mut self) {
        for base in crate::icons::data_dirs() {
            let mut targets = Vec::new();
            if base.exists() {
                targets.push((base.clone(), RecursiveMode::NonRecursive));
                let applications = base.join("applications");
                if applications.exists() {
                    targets.push((applications, RecursiveMode::Recursive));
                }
            } else if let Some(ancestor) = base.ancestors().skip(1).find(|p| p.exists()) {
                targets.push((ancestor.to_path_buf(), RecursiveMode::NonRecursive));
            }
            for parent in symlink_parents(&base) {
                targets.push((parent, RecursiveMode::NonRecursive));
            }
            for (path, mode) in targets {
                if self.watched.insert(path.clone()) {
                    if let Err(error) = self.watcher.watch(&path, mode) {
                        eprintln!("zl: cannot watch {}: {error}", path.display());
                    }
                }
            }
        }
    }
}

/// Parents of the symlinked components of `path`: replacing such a symlink
/// (a nix profile switch) does not touch the resolved target, so the
/// component's own watches keep pointing at the old generation.
fn symlink_parents(path: &Path) -> Vec<PathBuf> {
    path.ancestors()
        .filter(|p| {
            p.symlink_metadata()
                .is_ok_and(|m| m.file_type().is_symlink())
        })
        .filter_map(|p| p.parent().map(Path::to_path_buf))
        .collect()
}

fn debounce(pending: &mut Option<Instant>, now: Instant, changed: bool) -> bool {
    if changed {
        pending.get_or_insert(now);
    }
    if pending.is_some_and(|first| now.duration_since(first) >= Duration::from_millis(500)) {
        *pending = None;
        true
    } else {
        false
    }
}

fn relevant(event: &Event) -> bool {
    event.paths.iter().any(|path| {
        let desktop = path
            .extension()
            .is_some_and(|extension| extension == "desktop");
        // The applications directory itself matters even when the event
        // removed it and path.is_dir() can no longer answer.
        let applications = path.file_name().is_some_and(|name| name == "applications");
        match event.kind {
            EventKind::Create(_) | EventKind::Remove(_) => desktop || applications || path.is_dir(),
            // Modify covers in-place edits and atomic rename installs; the
            // rename destination may be the only path that still exists.
            EventKind::Modify(_) => desktop || applications || path.is_dir(),
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reload_waits_500ms_from_first_event_and_drains_the_burst() {
        let start = Instant::now();
        let mut pending = None;
        assert!(!debounce(&mut pending, start, false));
        assert!(!debounce(&mut pending, start, true));
        assert!(!debounce(
            &mut pending,
            start + Duration::from_millis(499),
            true
        ));
        assert!(debounce(
            &mut pending,
            start + Duration::from_millis(500),
            true
        ));
        assert!(pending.is_none());
        assert!(!debounce(
            &mut pending,
            start + Duration::from_millis(600),
            false
        ));
        assert!(!debounce(
            &mut pending,
            start + Duration::from_millis(700),
            true
        ));
        assert!(debounce(
            &mut pending,
            start + Duration::from_millis(1200),
            false
        ));
    }
    #[test]
    fn desktop_dir_and_rename_events_wake_a_rescan() {
        use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind, RenameMode};
        let file = std::path::PathBuf::from("fixture.desktop");
        assert!(relevant(
            &Event::new(EventKind::Create(CreateKind::File)).add_path(file.clone())
        ));
        assert!(relevant(
            &Event::new(EventKind::Modify(ModifyKind::Any)).add_path(file.clone())
        ));
        assert!(relevant(
            &Event::new(EventKind::Remove(RemoveKind::File)).add_path(file.clone())
        ));
        // Atomic rename install: the destination is a .desktop path.
        assert!(relevant(
            &Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
                .add_path("partial-4f9c2a".into())
                .add_path(file.clone())
        ));
        // The applications directory still counts after it was removed.
        let gone = std::path::PathBuf::from("/definitely/not/there/applications");
        assert!(!gone.exists());
        assert!(relevant(
            &Event::new(EventKind::Remove(RemoveKind::Folder)).add_path(gone.clone())
        ));
        assert!(relevant(
            &Event::new(EventKind::Create(CreateKind::Folder)).add_path(gone)
        ));
        assert!(!relevant(
            &Event::new(EventKind::Access(AccessKind::Any)).add_path(file)
        ));
        assert!(!relevant(
            &Event::new(EventKind::Modify(ModifyKind::Any)).add_path("unrelated.txt".into())
        ));
    }

    #[test]
    fn re_arm_covers_late_directories_and_symlink_parents() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let real = root.path().join("real");
        std::fs::create_dir_all(&real).unwrap();
        let link = root.path().join("linked");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        let missing = root.path().join("missing");
        let old_home = std::env::var_os("XDG_DATA_HOME");
        let old_dirs = std::env::var_os("XDG_DATA_DIRS");
        std::env::set_var("XDG_DATA_HOME", &link);
        std::env::set_var("XDG_DATA_DIRS", &missing);
        let mut watch = AppWatch::new().unwrap();
        // The symlinked data home watches its parent; the missing data dir
        // falls back to the nearest existing ancestor.
        assert!(watch.watched.contains(root.path()));
        assert!(watch.watched.contains(&link));
        // Applications directory created after startup gets picked up.
        std::fs::create_dir_all(real.join("applications")).unwrap();
        watch.re_arm();
        assert!(watch.watched.contains(&link.join("applications")));
        match old_home {
            Some(value) => std::env::set_var("XDG_DATA_HOME", value),
            None => std::env::remove_var("XDG_DATA_HOME"),
        }
        match old_dirs {
            Some(value) => std::env::set_var("XDG_DATA_DIRS", value),
            None => std::env::remove_var("XDG_DATA_DIRS"),
        }
    }
}
