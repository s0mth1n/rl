//! Bounded reads and private writes for the launcher's persistent state.
use std::{
    io::{self, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

pub const MAX_STATE: usize = 64 * 1024 * 1024;

pub fn read(path: impl AsRef<Path>, limit: usize) -> io::Result<Vec<u8>> {
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    let meta = file.metadata()?;
    if !meta.is_file() || meta.len() > limit as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid state file",
        ));
    }
    let mut data = Vec::new();
    file.take(limit as u64 + 1).read_to_end(&mut data)?;
    if data.len() > limit {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "state file too large",
        ));
    }
    Ok(data)
}

pub fn read_to_string(path: impl AsRef<Path>) -> io::Result<String> {
    String::from_utf8(read(path, MAX_STATE)?)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

pub fn write(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> io::Result<()> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .mode(0o600)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid state file",
        ));
    }
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.set_len(0)?;
    file.write_all(data.as_ref())
}
