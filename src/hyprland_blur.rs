use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::time::Duration;

pub fn apply(enabled: bool) -> std::io::Result<bool> {
    if !enabled {
        return Ok(false);
    }
    let Ok(signature) = std::env::var("HYPRLAND_INSTANCE_SIGNATURE") else {
        return Ok(false);
    };
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    let path = std::path::PathBuf::from(runtime)
        .join("hypr")
        .join(signature)
        .join(".socket.sock");
    apply_at(&path)?;
    Ok(true)
}

fn apply_at(path: &std::path::Path) -> std::io::Result<()> {
    for rule in [
        "blur,zl",
        "ignorezero,zl",
        "blurpopups,zl",
        "ignorealpha 0.35,zl",
    ] {
        let mut socket = UnixStream::connect(path)?;
        socket.set_read_timeout(Some(Duration::from_secs(2)))?;
        socket.set_write_timeout(Some(Duration::from_secs(2)))?;
        socket.write_all(format!("keyword layerrule {rule}").as_bytes())?;
        let mut response = String::new();
        socket.read_to_string(&mut response)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixListener;
    #[test]
    fn blur_rules_use_the_launcher_namespace_in_reference_order() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("socket");
        let listener = UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let mut commands = Vec::new();
            for _ in 0..4 {
                let (mut stream, _) = listener.accept().unwrap();
                let mut command = [0; 128];
                let n = stream.read(&mut command).unwrap();
                commands.push(String::from_utf8(command[..n].to_vec()).unwrap());
                stream.write_all(b"ok").unwrap();
            }
            commands
        });
        apply_at(&path).unwrap();
        assert_eq!(
            server.join().unwrap(),
            [
                "keyword layerrule blur,zl",
                "keyword layerrule ignorezero,zl",
                "keyword layerrule blurpopups,zl",
                "keyword layerrule ignorealpha 0.35,zl"
            ]
        );
        assert!(apply_at(&path).is_err());
    }

    #[test]
    fn disabled_and_non_hyprland_do_not_open_a_socket() {
        let _lock = crate::testutil::ENV_LOCK.lock().unwrap();
        let old = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE");
        std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "missing");
        assert!(!apply(false).unwrap());
        std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE");
        assert!(!apply(true).unwrap());
        match old {
            Some(value) => std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", value),
            None => std::env::remove_var("HYPRLAND_INSTANCE_SIGNATURE"),
        }
    }
}
