//! Desktop entry notifications wake the main loop without polling directories.
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

pub struct AppWatch {
    _watcher: RecommendedWatcher,
    events: UnixStream,
    pending: Option<Instant>,
}

impl AppWatch {
    pub fn new() -> Result<Self, String> {
        let (events, mut send) = UnixStream::pair().map_err(|e| e.to_string())?;
        events.set_nonblocking(true).map_err(|e| e.to_string())?;
        send.set_nonblocking(true).map_err(|e| e.to_string())?;
        let mut watcher =
            notify::recommended_watcher(move |event: Result<Event, notify::Error>| match event {
                Ok(event) if relevant(&event) => {
                    let _ = send.write(&[1]);
                }
                Err(error) => eprintln!("zl: application watcher: {error}"),
                _ => {}
            })
            .map_err(|e| e.to_string())?;
        for dir in crate::icons::data_dirs() {
            let path = dir.join("applications");
            if path.exists() {
                if let Err(error) = watcher.watch(&path, RecursiveMode::Recursive) {
                    eprintln!("zl: cannot watch {}: {error}", path.display());
                }
            }
        }
        Ok(Self {
            _watcher: watcher,
            events,
            pending: None,
        })
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
        match event.kind {
            EventKind::Create(_) | EventKind::Remove(_) => desktop || path.is_dir(),
            EventKind::Modify(_) => desktop,
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
    fn only_desktop_file_changes_and_directory_changes_wake_a_rescan() {
        use notify::event::{AccessKind, CreateKind, ModifyKind, RemoveKind};
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
        assert!(!relevant(
            &Event::new(EventKind::Access(AccessKind::Any)).add_path(file)
        ));
        assert!(!relevant(
            &Event::new(EventKind::Modify(ModifyKind::Any)).add_path("unrelated.txt".into())
        ));
    }
}
