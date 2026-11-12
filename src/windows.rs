//! Niri window collection and focus, following zlaunch's compositor backend.
use serde::Deserialize;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Deserialize)]
pub struct Window {
    pub id: u64,
    pub title: Option<String>,
    pub app_id: Option<String>,
    pub workspace_id: Option<u64>,
}

impl Window {
    pub fn title(&self) -> &str {
        self.title
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| self.app_id.as_deref().unwrap_or(""))
    }
    pub fn description(&self) -> String {
        let app = self.app_id.as_deref().unwrap_or("");
        let name = app.rsplit('.').next().unwrap_or(app);
        let mut chars = name.chars();
        let name = chars
            .next()
            .map(|c| c.to_uppercase().chain(chars).collect::<String>())
            .unwrap_or_default();
        format!("{name} - Workspace {}", self.workspace_id.unwrap_or(0))
    }
}

fn request(path: &Path, command: &str) -> Result<serde_json::Value, String> {
    let mut stream = UnixStream::connect(path).map_err(|e| e.to_string())?;
    let timeout = Some(Duration::from_millis(250));
    stream
        .set_read_timeout(timeout)
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(timeout)
        .map_err(|e| e.to_string())?;
    stream
        .write_all(command.as_bytes())
        .map_err(|e| e.to_string())?;
    let deadline = Instant::now() + Duration::from_millis(250);
    let mut reader = BufReader::new(stream);
    let mut response = Vec::new();
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or("Niri reply timed out")?;
        reader
            .get_ref()
            .set_read_timeout(Some(remaining))
            .map_err(|e| e.to_string())?;
        let bytes = reader.fill_buf().map_err(|e| e.to_string())?;
        if bytes.is_empty() {
            return Err("Niri closed the socket before a reply".into());
        }
        let end = bytes.iter().position(|b| *b == b'\n');
        let take = end.map_or(bytes.len(), |i| i + 1);
        if response.len() + take > 1024 * 1024 {
            return Err("Niri reply exceeds 1 MiB".into());
        }
        response.extend_from_slice(&bytes[..take]);
        reader.consume(take);
        if end.is_some() {
            break;
        }
    }
    let reply: Result<serde_json::Value, serde_json::Value> =
        serde_json::from_slice(&response).map_err(|e| e.to_string())?;
    reply.map_err(|error| format!("Niri rejected request: {error}"))
}

fn list_at(path: &Path) -> Result<Vec<Window>, String> {
    let reply = request(path, "\"Windows\"\n")?;
    let windows: Vec<Window> = serde_json::from_value(
        reply
            .get("Windows")
            .cloned()
            .ok_or("Niri reply has no Windows")?,
    )
    .map_err(|e| e.to_string())?;
    Ok(windows
        .into_iter()
        .filter(|w| {
            !matches!(
                w.app_id
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .as_str(),
                "zlaunch" | "zl" | "rl"
            )
        })
        .collect())
}

pub fn scan() -> Vec<Window> {
    let Some(socket) = std::env::var_os("NIRI_SOCKET") else {
        return Vec::new();
    };
    match list_at(Path::new(&socket)) {
        Ok(windows) => windows,
        Err(error) => {
            eprintln!("zl: could not list Niri windows: {error}");
            Vec::new()
        }
    }
}

fn focus_at(path: &Path, id: u64) -> Result<(), String> {
    let command = format!("{{\"Action\":{{\"FocusWindow\":{{\"id\":{id}}}}}}}\n");
    let reply = request(path, &command)?;
    if reply != serde_json::Value::String("Handled".into()) {
        return Err("Unexpected Niri focus reply".into());
    }
    Ok(())
}

pub fn focus(id: u64) -> Result<(), String> {
    let socket = std::env::var_os("NIRI_SOCKET").ok_or("NIRI_SOCKET is not set")?;
    focus_at(Path::new(&socket), id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    use std::os::unix::net::UnixListener;
    fn server(reply: &'static str) -> (tempfile::TempDir, std::thread::JoinHandle<String>) {
        let dir = tempfile::tempdir().unwrap();
        let listener = UnixListener::bind(dir.path().join("socket")).unwrap();
        let worker = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            let mut stream = BufReader::new(stream);
            let mut command = String::new();
            stream.read_line(&mut command).unwrap();
            stream.get_mut().write_all(reply.as_bytes()).unwrap();
            command
        });
        (dir, worker)
    }
    #[test]
    fn windows_reply_preserves_ids_and_filters_the_launcher() {
        let (dir, worker) = server("{\"Ok\":{\"Windows\":[{\"id\":42,\"title\":\"Docs\",\"app_id\":\"firefox\",\"workspace_id\":2,\"is_focused\":true},{\"id\":9,\"title\":null,\"app_id\":\"foot\",\"workspace_id\":null,\"is_focused\":false},{\"id\":4,\"title\":\"Launcher\",\"app_id\":\"zlaunch\",\"workspace_id\":1,\"is_focused\":false}]}}\n");
        let windows = list_at(&dir.path().join("socket")).unwrap();
        assert_eq!(worker.join().unwrap(), "\"Windows\"\n");
        assert_eq!(windows.len(), 2);
        assert_eq!(
            (windows[0].id, windows[0].title(), windows[0].description()),
            (42, "Docs", "Firefox - Workspace 2".into())
        );
        assert_eq!(windows[1].title(), "foot");
    }
    #[test]
    fn focus_sends_the_numeric_id_and_rejects_errors() {
        let (dir, worker) = server("{\"Ok\":\"Handled\"}\n");
        focus_at(&dir.path().join("socket"), 42).unwrap();
        assert_eq!(
            worker.join().unwrap(),
            "{\"Action\":{\"FocusWindow\":{\"id\":42}}}\n"
        );
        let (dir, worker) = server("{\"Err\":\"No such window\"}\n");
        assert!(focus_at(&dir.path().join("socket"), 42)
            .unwrap_err()
            .contains("No such window"));
        worker.join().unwrap();
        let (dir, worker) = server("{\"Ok\":{\"Windows\":[]}}\n");
        assert!(focus_at(&dir.path().join("socket"), 42).is_err());
        worker.join().unwrap();
    }
    #[test]
    fn oversized_replies_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let listener = UnixListener::bind(dir.path().join("socket")).unwrap();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0; 10];
            stream.read_exact(&mut request).unwrap();
            let _ = stream.write_all(&vec![b'x'; 1024 * 1024 + 1]);
        });
        assert!(list_at(&dir.path().join("socket"))
            .unwrap_err()
            .contains("exceeds 1 MiB"));
        worker.join().unwrap();
    }
    #[test]
    fn search_keeps_window_id_and_application_workspace_text() {
        let cfg = crate::config::Config::default();
        let history = crate::clip::History::new(0);
        let windows = [Window {
            id: 73,
            title: Some("Documentation".into()),
            app_id: Some("org.mozilla.firefox".into()),
            workspace_id: Some(3),
        }];
        let search = crate::items::Search {
            cfg: &cfg,
            apps: &[],
            history: &history,
            windows: &windows,
        };
        let rows = search.run("firefox");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Documentation");
        assert_eq!(rows[0].sub, "Firefox - Workspace 3");
        assert_eq!(rows[0].action, crate::items::Action::Window(73));
    }
    #[test]
    fn bad_and_unfinished_replies_do_not_hang() {
        for reply in [
            "bad\n",
            "{\"Ok\":\"Handled\"}\n",
            "{\"Err\":\"bad request\"}\n",
            "",
        ] {
            let (dir, worker) = server(reply);
            assert!(list_at(&dir.path().join("socket")).is_err());
            worker.join().unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        let listener = UnixListener::bind(dir.path().join("socket")).unwrap();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        assert!(list_at(&dir.path().join("socket")).is_err());
        worker.join().unwrap();
        assert!(list_at(&dir.path().join("missing")).is_err());
    }
}
