//! Bounded EOF-framed messages on the local control socket.
use std::io::{self, Read};
use std::os::unix::net::UnixStream;
use std::time::{Duration, Instant};

pub fn read_message(stream: &mut UnixStream, limit: usize, budget: Duration) -> io::Result<String> {
    let deadline = Instant::now() + budget;
    let mut message = Vec::new();
    let mut chunk = [0; 4096];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "control message deadline exceeded",
            ));
        }
        stream.set_read_timeout(Some(remaining))?;
        match stream.read(&mut chunk) {
            Ok(0) => {
                return String::from_utf8(message)
                    .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
            }
            Ok(n) => {
                if n > limit.saturating_sub(message.len()) {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        "control message too large",
                    ));
                }
                message.extend_from_slice(&chunk[..n]);
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::net::Shutdown;

    #[test]
    fn complete_utf8_and_exact_limit_require_eof() {
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        writer.write_all("café".as_bytes()).unwrap();
        writer.shutdown(Shutdown::Write).unwrap();
        assert_eq!(
            read_message(&mut reader, 5, Duration::from_secs(1)).unwrap(),
            "café"
        );
        for bytes in [b"123456".as_slice(), b"\xff".as_slice()] {
            let (mut reader, mut writer) = UnixStream::pair().unwrap();
            writer.write_all(bytes).unwrap();
            writer.shutdown(Shutdown::Write).unwrap();
            assert_eq!(
                read_message(&mut reader, 5, Duration::from_secs(1))
                    .unwrap_err()
                    .kind(),
                io::ErrorKind::InvalidData
            );
        }
    }

    #[test]
    fn trickled_partial_command_does_not_extend_deadline() {
        let (mut reader, mut writer) = UnixStream::pair().unwrap();
        writer.write_all(b"quit").unwrap();
        let drip = std::thread::spawn(move || {
            for _ in 0..15 {
                std::thread::sleep(Duration::from_millis(20));
                if writer.write_all(b" ").is_err() {
                    break;
                }
            }
        });
        let start = Instant::now();
        assert!(read_message(&mut reader, 1024, Duration::from_millis(80)).is_err());
        assert!(start.elapsed() < Duration::from_millis(250));
        drop(reader);
        drip.join().unwrap();
    }
}
