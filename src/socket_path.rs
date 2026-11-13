//! Remove the control socket on orderly exit, without unlinking a replacement.
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

/// Exclusive daemon guard. The socket path itself cannot carry the lock: a
/// stale file is unlinked before binding, which would drop the lock with it.
/// Two daemons racing past the "already running" connect check used to both
/// bind (the second unlinking the first's live socket), leaving an orphaned
/// daemon. The flock is held for the daemon's lifetime; the kernel releases
/// it when the process exits.
pub struct DaemonLock {
    // Held only to keep the flock; the kernel releases it on process exit.
    _file: std::fs::File,
}

impl DaemonLock {
    pub fn acquire(path: &Path) -> io::Result<Option<Self>> {
        use std::os::unix::io::AsRawFd;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        let ok = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0;
        Ok(if ok { Some(Self { _file: file }) } else { None })
    }
}

pub struct SocketPath {
    path: PathBuf,
    device: u64,
    inode: u64,
}

impl SocketPath {
    pub fn new(path: impl AsRef<Path>) -> io::Result<Self> {
        let metadata = std::fs::symlink_metadata(&path)?;
        Ok(Self {
            path: path.as_ref().into(),
            device: metadata.dev(),
            inode: metadata.ino(),
        })
    }
}

impl Drop for SocketPath {
    fn drop(&mut self) {
        if let Ok(metadata) = std::fs::symlink_metadata(&self.path) {
            if metadata.dev() == self.device && metadata.ino() == self.inode {
                let _ = std::fs::remove_file(&self.path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;

    #[test]
    fn daemon_lock_is_exclusive_and_released_on_drop() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("zl.lock");
        let first = DaemonLock::acquire(&path).unwrap();
        assert!(first.is_some());
        // A racing daemon cannot take the lock.
        assert!(DaemonLock::acquire(&path).unwrap().is_none());
        drop(first);
        // After exit the next daemon starts normally.
        assert!(DaemonLock::acquire(&path).unwrap().is_some());
    }

    #[test]
    fn owned_socket_is_removed_but_replacement_is_preserved() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("control.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let cleanup = SocketPath::new(&path).unwrap();
        drop(cleanup);
        assert!(!path.exists());
        drop(listener);
        let listener = UnixListener::bind(&path).unwrap();
        let cleanup = SocketPath::new(&path).unwrap();
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, "replacement").unwrap();
        drop(cleanup);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "replacement");
        drop(listener);
    }
}
