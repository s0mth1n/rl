//! Remove the control socket on orderly exit, without unlinking a replacement.
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

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
