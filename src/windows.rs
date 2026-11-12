//! Window listing and focus across compositors, following zlaunch's
//! compositor backends. Detection order matches zlaunch's detect.rs:
//! Hyprland, KWin, Niri, MangoWM, then a no-op fallback so application
//! launching keeps working on unsupported compositors.
use serde::Deserialize;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const IO_TIMEOUT: Duration = Duration::from_millis(250);
const MAX_REPLY: usize = 1024 * 1024;

/// One open window. `address` is the compositor-specific identifier used for
/// focusing (a numeric id for Niri/MangoWM, "0x..." for Hyprland, a UUID for
/// KWin). `icon_data` carries PNG bytes when the compositor supplies them
/// (KWin's WindowsRunner); other backends leave it None and the icon is
/// resolved from the class through the icon theme instead.
#[derive(Clone, Debug)]
pub struct Window {
    pub address: String,
    pub title: String,
    pub app_id: String,
    pub workspace: i32,
    /// Kept for parity with zlaunch's Window; nothing renders focus state yet.
    #[allow(dead_code)]
    pub focused: bool,
    pub icon_data: Option<Vec<u8>>,
}

impl Window {
    pub fn title(&self) -> &str {
        &self.title
    }
    /// "Firefox - Workspace 2", like zlaunch's WindowItem description.
    pub fn description(&self) -> String {
        let name = self.app_id.rsplit('.').next().unwrap_or(&self.app_id);
        let mut chars = name.chars();
        let name = chars
            .next()
            .map(|c| c.to_uppercase().chain(chars).collect::<String>())
            .unwrap_or_default();
        format!("{name} - Workspace {}", self.workspace)
    }
}

/// zlaunch drops its own launcher window from the list; rl's layer namespace
/// and desktop id are "zl", so all three spellings stay excluded.
fn is_launcher(class: &str) -> bool {
    matches!(class.to_ascii_lowercase().as_str(), "zlaunch" | "zl" | "rl")
}

/// Title falls back to the class when empty (zlaunch's get_display_title).
fn display_title(title: &str, class: &str) -> String {
    if title.is_empty() {
        class.to_string()
    } else {
        title.to_string()
    }
}

/// One newline-terminated JSON reply (Niri and MangoWM frame replies by line).
fn request_line(path: &Path, command: &str) -> Result<String, String> {
    let mut stream = connect(path)?;
    stream
        .write_all(command.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);
    let mut reply = String::new();
    read_capped(&mut reader, &mut reply, true)?;
    Ok(reply)
}

/// A reply that ends at EOF (Hyprland closes the socket after answering).
fn request_full(path: &Path, command: &str) -> Result<String, String> {
    let mut stream = connect(path)?;
    stream
        .write_all(command.as_bytes())
        .map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(stream);
    let mut reply = String::new();
    read_capped(&mut reader, &mut reply, false)?;
    Ok(reply)
}

fn connect(path: &Path) -> Result<UnixStream, String> {
    let stream = UnixStream::connect(path).map_err(|e| e.to_string())?;
    stream
        .set_read_timeout(Some(IO_TIMEOUT))
        .map_err(|e| e.to_string())?;
    stream
        .set_write_timeout(Some(IO_TIMEOUT))
        .map_err(|e| e.to_string())?;
    Ok(stream)
}

fn read_capped(
    reader: &mut BufReader<UnixStream>,
    out: &mut String,
    line_framed: bool,
) -> Result<(), String> {
    let deadline = Instant::now() + IO_TIMEOUT;
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or("compositor reply timed out")?;
        reader
            .get_ref()
            .set_read_timeout(Some(remaining))
            .map_err(|e| e.to_string())?;
        let bytes = reader.fill_buf().map_err(|e| e.to_string())?;
        if bytes.is_empty() {
            if out.is_empty() {
                return Err("compositor closed the socket without a reply".into());
            }
            return Ok(());
        }
        let end = if line_framed {
            bytes.iter().position(|b| *b == b'\n').map(|i| i + 1)
        } else {
            None
        };
        let take = end.unwrap_or(bytes.len());
        if out.len() + take > MAX_REPLY {
            return Err("compositor reply exceeds 1 MiB".into());
        }
        out.push_str(&String::from_utf8_lossy(&bytes[..take]));
        reader.consume(take);
        if end.is_some() {
            return Ok(());
        }
    }
}

// ---------- Hyprland (IPC socket, "j/clients" JSON) ----------

fn hyprland_socket() -> Option<PathBuf> {
    let signature = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE")?;
    let runtime = std::env::var_os("XDG_RUNTIME_DIR").unwrap_or("/tmp".into());
    Some(
        Path::new(&runtime)
            .join("hypr")
            .join(signature)
            .join(".socket.sock"),
    )
}

#[derive(Debug, Deserialize)]
struct HyprlandClient {
    address: String,
    title: String,
    class: String,
    workspace: HyprlandWorkspace,
    /// 0 is the focused window, higher ids are less recently focused.
    #[serde(rename = "focusHistoryID")]
    focus_history_id: i32,
    #[serde(default)]
    mapped: bool,
    #[serde(default)]
    hidden: bool,
}

#[derive(Debug, Deserialize)]
struct HyprlandWorkspace {
    id: i32,
}

fn hyprland_list(socket: &Path) -> Result<Vec<Window>, String> {
    let reply = request_full(socket, "j/clients")?;
    let clients: Vec<HyprlandClient> = serde_json::from_str(&reply).map_err(|e| e.to_string())?;
    Ok(clients
        .into_iter()
        .filter(|c| c.mapped && !c.hidden && !c.class.is_empty() && !is_launcher(&c.class))
        .map(|c| Window {
            focused: c.focus_history_id == 0,
            workspace: c.workspace.id,
            title: display_title(&c.title, &c.class),
            address: c.address,
            app_id: c.class,
            icon_data: None,
        })
        .collect())
}

fn hyprland_focus(socket: &Path, address: &str) -> Result<(), String> {
    request_full(socket, &format!("dispatch focuswindow address:{address}"))?;
    Ok(())
}

// ---------- KWin (D-Bus WindowsRunner krunner interface) ----------
// The zbus glue cannot run without a KDE session bus; the parsing half is
// pure and unit-tested below.

fn kwin_connect() -> Option<zbus::blocking::Connection> {
    std::env::var_os("KDE_SESSION_VERSION")?;
    let conn = zbus::blocking::Connection::session().ok()?;
    // Probe KWin itself; a KDE session without KWin (rare) should fall through.
    let probe = zbus::blocking::Proxy::new(&conn, "org.kde.KWin", "/KWin", "org.kde.KWin").ok()?;
    let _: String = probe.call("supportInformation", &()).ok()?;
    Some(conn)
}

/// KRunner match tuple: (match_id, text, subtext, type, relevance, properties).
type KwinMatch = (
    String,
    String,
    String,
    i32,
    f64,
    std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
);

/// Keep only the activate-action entries ("0_<uuid>") and derive the class
/// like zlaunch: the runner's icon name, else the title's last " - " segment.
fn kwin_window(
    match_id: &str,
    title: &str,
    icon_name: Option<String>,
    icon_png: Option<Vec<u8>>,
) -> Option<Window> {
    let address = match_id.strip_prefix("0_")?;
    let class =
        icon_name.unwrap_or_else(|| title.rsplit(" - ").next().unwrap_or(title).to_string());
    Some(Window {
        address: address.to_string(),
        title: title.to_string(),
        app_id: class,
        workspace: 1,   // WindowsRunner does not expose the desktop
        focused: false, // nor the focus state
        icon_data: icon_png,
    })
}

fn kwin_list(conn: &zbus::blocking::Connection) -> Result<Vec<Window>, String> {
    let runner =
        zbus::blocking::Proxy::new(conn, "org.kde.KWin", "/WindowsRunner", "org.kde.krunner1")
            .map_err(|e| e.to_string())?;
    let matches: Vec<KwinMatch> = runner.call("Match", &("",)).map_err(|e| e.to_string())?;
    let mut seen = std::collections::HashSet::new();
    Ok(matches
        .into_iter()
        .filter_map(|(id, title, _, _, _, props)| {
            let icon_png = props.get("icon-data").and_then(kwin_icon_from_value);
            let icon_name = props
                .get("icon")
                .and_then(|v| TryInto::<String>::try_into(v.clone()).ok());
            let window = kwin_window(&id, &title, icon_name, icon_png)?;
            seen.insert(window.address.clone()).then_some(window)
        })
        .collect())
}

fn kwin_focus(conn: &zbus::blocking::Connection, address: &str) -> Result<(), String> {
    let runner =
        zbus::blocking::Proxy::new(conn, "org.kde.KWin", "/WindowsRunner", "org.kde.krunner1")
            .map_err(|e| e.to_string())?;
    let match_id = format!("0_{address}");
    if runner.call::<_, _, ()>("Run", &(&match_id, "")).is_ok() {
        return Ok(());
    }
    // zlaunch's fallback: drive the same interface through qdbus.
    let ok = std::process::Command::new("timeout")
        .args([
            "2",
            "qdbus",
            "org.kde.KWin",
            "/WindowsRunner",
            "org.kde.krunner1.Run",
            &match_id,
            "",
        ])
        .status()
        .map_err(|e| e.to_string())?
        .success();
    ok.then_some(())
        .ok_or("qdbus WindowsRunner.Run failed".into())
}

/// Decode the runner's icon-data property. The D-Bus structure is
/// (width i32, height i32, rowstride i32, has_alpha bool, bits i32,
/// channels i32, pixels [u8]); the pixels become a PNG like zlaunch does.
fn kwin_icon_from_value(value: &zbus::zvariant::OwnedValue) -> Option<Vec<u8>> {
    use zbus::zvariant::{Structure, Value};
    let fields: &Structure = value.downcast_ref().ok()?;
    let f = fields.fields();
    if f.len() < 7 {
        return None;
    }
    let width: i32 = f[0].downcast_ref().ok()?;
    let height: i32 = f[1].downcast_ref().ok()?;
    let rowstride: i32 = f[2].downcast_ref().ok()?;
    let has_alpha: bool = f[3].downcast_ref().ok()?;
    let channels: i32 = f[5].downcast_ref().ok()?;
    let Value::Array(bytes) = &f[6] else {
        return None;
    };
    let pixels: Vec<u8> = bytes
        .iter()
        .filter_map(|b| b.downcast_ref::<u8>().ok())
        .collect();
    icon_png(width, height, rowstride, has_alpha, channels, &pixels)
}

/// Re-encode raw icon pixels as PNG (pure; unit-tested without D-Bus).
fn icon_png(
    width: i32,
    height: i32,
    rowstride: i32,
    has_alpha: bool,
    channels: i32,
    pixels: &[u8],
) -> Option<Vec<u8>> {
    if width <= 0 || height <= 0 || width > 256 || height > 256 {
        return None;
    }
    if rowstride <= 0 || !(3..=4).contains(&channels) {
        return None;
    }
    let (width, height, rowstride, channels) = (
        width as usize,
        height as usize,
        rowstride as usize,
        channels as usize,
    );
    let mut rgba = Vec::with_capacity(width * height * 4);
    for y in 0..height {
        for x in 0..width {
            let at = y
                .checked_mul(rowstride)?
                .checked_add(x.checked_mul(channels)?)?;
            let px = match (channels, has_alpha) {
                (4, true) => [
                    *pixels.get(at)?,
                    *pixels.get(at + 1)?,
                    *pixels.get(at + 2)?,
                    *pixels.get(at + 3)?,
                ],
                (3, _) | (4, false) => [
                    *pixels.get(at)?,
                    *pixels.get(at + 1)?,
                    *pixels.get(at + 2)?,
                    255,
                ],
                _ => return None,
            };
            rgba.extend_from_slice(&px);
        }
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, width as u32, height as u32);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().ok()?;
        writer.write_image_data(&rgba).ok()?;
    }
    Some(png)
}

// ---------- Niri (IPC socket, newline-framed JSON) ----------

#[derive(Debug, Deserialize)]
struct NiriWindow {
    id: u64,
    title: Option<String>,
    app_id: Option<String>,
    workspace_id: Option<i64>,
    is_focused: bool,
}

fn niri_list(socket: &Path) -> Result<Vec<Window>, String> {
    let reply = request_line(socket, "\"Windows\"\n")?;
    let reply: Result<serde_json::Value, serde_json::Value> =
        serde_json::from_str(&reply).map_err(|e| e.to_string())?;
    let reply = reply.map_err(|error| format!("Niri rejected request: {error}"))?;
    let windows: Vec<NiriWindow> = serde_json::from_value(
        reply
            .get("Windows")
            .cloned()
            .ok_or("Niri reply has no Windows")?,
    )
    .map_err(|e| e.to_string())?;
    Ok(windows
        .into_iter()
        .filter(|w| !is_launcher(w.app_id.as_deref().unwrap_or("")))
        .map(|w| {
            let class = w.app_id.unwrap_or_default();
            Window {
                address: w.id.to_string(),
                title: display_title(w.title.as_deref().unwrap_or(""), &class),
                app_id: class,
                workspace: w.workspace_id.unwrap_or(0) as i32,
                focused: w.is_focused,
                icon_data: None,
            }
        })
        .collect())
}

fn niri_focus(socket: &Path, address: &str) -> Result<(), String> {
    let id: u64 = address
        .parse()
        .map_err(|_| "bad Niri window id".to_string())?;
    let command = format!("{{\"Action\":{{\"FocusWindow\":{{\"id\":{id}}}}}}}\n");
    let reply = request_line(socket, &command)?;
    let reply: Result<serde_json::Value, serde_json::Value> =
        serde_json::from_str(&reply).map_err(|e| e.to_string())?;
    let reply = reply.map_err(|error| format!("Niri rejected request: {error}"))?;
    if reply != serde_json::Value::String("Handled".into()) {
        return Err("Unexpected Niri focus reply".into());
    }
    Ok(())
}

// ---------- MangoWM (IPC socket from MANGO_INSTANCE_SIGNATURE) ----------

#[derive(Debug, Deserialize)]
struct MangoClients {
    clients: Vec<MangoWindow>,
}

#[derive(Debug, Deserialize)]
struct MangoWindow {
    id: u64,
    title: String,
    appid: String,
    is_focused: bool,
    tags: Vec<i32>,
}

fn mango_list(socket: &Path) -> Result<Vec<Window>, String> {
    let reply = request_line(socket, "get all-clients\n")?;
    let reply: MangoClients = serde_json::from_str(&reply).map_err(|e| e.to_string())?;
    Ok(reply
        .clients
        .into_iter()
        .filter(|w| !is_launcher(&w.appid))
        .map(|w| Window {
            address: w.id.to_string(),
            workspace: w.tags.first().copied().unwrap_or(1),
            title: display_title(&w.title, &w.appid),
            app_id: w.appid,
            focused: w.is_focused,
            icon_data: None,
        })
        .collect())
}

fn mango_focus(socket: &Path, address: &str) -> Result<(), String> {
    request_line(socket, &format!("dispatch focusid client,{address}\n"))?;
    Ok(())
}

// ---------- detection and dispatch (zlaunch's detect.rs order) ----------

enum Backend {
    Hyprland(PathBuf),
    KWin(zbus::blocking::Connection),
    Niri(PathBuf),
    Mango(PathBuf),
    Noop,
}

fn detect() -> Backend {
    if let Some(socket) = hyprland_socket() {
        return Backend::Hyprland(socket);
    }
    if let Some(conn) = kwin_connect() {
        return Backend::KWin(conn);
    }
    if let Some(socket) = std::env::var_os("NIRI_SOCKET") {
        return Backend::Niri(socket.into());
    }
    if let Some(socket) = std::env::var_os("MANGO_INSTANCE_SIGNATURE") {
        return Backend::Mango(socket.into());
    }
    Backend::Noop
}

pub fn scan() -> Vec<Window> {
    let result = match detect() {
        Backend::Hyprland(socket) => hyprland_list(&socket),
        Backend::KWin(conn) => kwin_list(&conn),
        Backend::Niri(socket) => niri_list(&socket),
        Backend::Mango(socket) => mango_list(&socket),
        Backend::Noop => Ok(Vec::new()),
    };
    match result {
        Ok(windows) => windows,
        Err(error) => {
            eprintln!("zl: could not list windows: {error}");
            Vec::new()
        }
    }
}

pub fn focus(address: &str) -> Result<(), String> {
    match detect() {
        Backend::Hyprland(socket) => hyprland_focus(&socket, address),
        Backend::KWin(conn) => kwin_focus(&conn, address),
        Backend::Niri(socket) => niri_focus(&socket, address),
        Backend::Mango(socket) => mango_focus(&socket, address),
        Backend::Noop => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;
    /// Mock compositor socket: captures the command, replies with the fixture.
    fn server(reply: &'static str) -> (tempfile::TempDir, std::thread::JoinHandle<String>) {
        let dir = tempfile::tempdir().unwrap();
        let listener = std::os::unix::net::UnixListener::bind(dir.path().join("socket")).unwrap();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut command = Vec::new();
            let mut byte = [0];
            // Commands are short and newline-terminated or one-shot reads.
            while stream.read(&mut byte).unwrap() == 1 {
                command.push(byte[0]);
                if byte[0] == b'\n'
                    || command.ends_with(b"j/clients")
                    || command.ends_with(b"0xabc")
                    || command.ends_with(b"zl")
                {
                    break;
                }
            }
            stream.write_all(reply.as_bytes()).unwrap();
            String::from_utf8(command).unwrap()
        });
        (dir, worker)
    }

    #[test]
    fn hyprland_lists_mapped_windows_and_marks_focus() {
        let (dir, worker) = server(
            r#"[{"address":"0x1","title":"Docs","class":"firefox","workspace":{"id":2},"focusHistoryID":0,"mapped":true,"hidden":false},{"address":"0x2","title":"","class":"foot","workspace":{"id":1},"focusHistoryID":1,"mapped":true,"hidden":false},{"address":"0x3","title":"Scratch","class":"htop","workspace":{"id":1},"focusHistoryID":2,"mapped":false,"hidden":false},{"address":"0x4","title":"Hidden","class":"term","workspace":{"id":1},"focusHistoryID":3,"mapped":true,"hidden":true},{"address":"0x5","title":"No class","class":"","workspace":{"id":1},"focusHistoryID":4,"mapped":true,"hidden":false},{"address":"0x6","title":"Launcher","class":"zlaunch","workspace":{"id":1},"focusHistoryID":5,"mapped":true,"hidden":false}]"#,
        );
        let windows = hyprland_list(&dir.path().join("socket")).unwrap();
        assert_eq!(worker.join().unwrap(), "j/clients");
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].address, "0x1");
        assert_eq!(windows[0].title(), "Docs");
        assert_eq!(windows[0].workspace, 2);
        assert!(windows[0].focused);
        assert_eq!(windows[0].description(), "Firefox - Workspace 2");
        // Empty title falls back to the class.
        assert_eq!(windows[1].title(), "foot");
        assert!(!windows[1].focused);
    }

    #[test]
    fn hyprland_focus_and_blur_commands() {
        let (dir, worker) = server("ok");
        hyprland_focus(&dir.path().join("socket"), "0xabc").unwrap();
        assert_eq!(worker.join().unwrap(), "dispatch focuswindow address:0xabc");
    }

    #[test]
    fn niri_windows_keep_ids_titles_and_filter_the_launcher() {
        let (dir, worker) = server("{\"Ok\":{\"Windows\":[{\"id\":42,\"title\":\"Docs\",\"app_id\":\"firefox\",\"workspace_id\":2,\"is_focused\":true},{\"id\":9,\"title\":null,\"app_id\":\"foot\",\"workspace_id\":null,\"is_focused\":false},{\"id\":4,\"title\":\"Launcher\",\"app_id\":\"zlaunch\",\"workspace_id\":1,\"is_focused\":false}]}}\n");
        let windows = niri_list(&dir.path().join("socket")).unwrap();
        assert_eq!(worker.join().unwrap(), "\"Windows\"\n");
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].address, "42");
        assert_eq!(windows[0].title(), "Docs");
        assert_eq!(windows[0].description(), "Firefox - Workspace 2");
        assert!(windows[0].focused);
        assert_eq!(windows[1].title(), "foot");
        assert_eq!(windows[1].workspace, 0);
    }

    #[test]
    fn niri_focus_sends_the_numeric_id_and_rejects_errors() {
        let (dir, worker) = server("{\"Ok\":\"Handled\"}\n");
        niri_focus(&dir.path().join("socket"), "42").unwrap();
        assert_eq!(
            worker.join().unwrap(),
            "{\"Action\":{\"FocusWindow\":{\"id\":42}}}\n"
        );
        let (dir, worker) = server("{\"Err\":\"No such window\"}\n");
        assert!(niri_focus(&dir.path().join("socket"), "42")
            .unwrap_err()
            .contains("No such window"));
        worker.join().unwrap();
        assert!(niri_focus(&dir.path().join("socket"), "not-a-number").is_err());
    }

    #[test]
    fn mango_windows_take_workspace_from_the_first_tag() {
        let (dir, worker) = server(
            r#"{"clients":[{"id":7,"title":"Editor","appid":"emacs","is_focused":true,"tags":[3,1]},{"id":8,"title":"","appid":"zlaunch","is_focused":false,"tags":[1]},{"id":9,"title":"Term","appid":"foot","is_focused":false,"tags":[]}]}"#,
        );
        let windows = mango_list(&dir.path().join("socket")).unwrap();
        assert_eq!(worker.join().unwrap(), "get all-clients\n");
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].address, "7");
        assert_eq!(windows[0].workspace, 3);
        assert!(windows[0].focused);
        // No tags: workspace defaults to 1.
        assert_eq!(windows[1].workspace, 1);
        let (dir, worker) = server("ok\n");
        mango_focus(&dir.path().join("socket"), "7").unwrap();
        assert_eq!(worker.join().unwrap(), "dispatch focusid client,7\n");
    }

    #[test]
    fn kwin_window_keeps_only_activate_matches_and_derives_class() {
        let w = kwin_window("0_{uuid}", "Document - Mozilla Firefox", None, None).unwrap();
        assert_eq!(w.address, "{uuid}");
        assert_eq!(w.app_id, "Mozilla Firefox");
        assert_eq!(w.workspace, 1);
        assert!(!w.focused);
        let w = kwin_window(
            "0_{uuid2}",
            "Title",
            Some("firefox".into()),
            Some(vec![1, 2]),
        )
        .unwrap();
        assert_eq!(w.app_id, "firefox");
        assert_eq!(w.icon_data, Some(vec![1, 2]));
        // Close/other actions are not windows to switch to.
        assert!(kwin_window("1_{uuid}", "Title", None, None).is_none());
    }

    #[test]
    fn kwin_icon_pixels_encode_as_png() {
        // 2x1 RGBA red/green with rowstride 8.
        let pixels = [255, 0, 0, 255, 0, 255, 0, 128];
        let png = icon_png(2, 1, 8, true, 4, &pixels).unwrap();
        assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);
        // Decode straight back (decode_png rescales, so it is not lossless here).
        let dec = png::Decoder::new(&png[..]);
        let mut reader = dec.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut buf).unwrap();
        assert_eq!(&buf[..8], &[255, 0, 0, 255, 0, 255, 0, 128]);
        // RGB without alpha gains an opaque alpha channel.
        let rgb = [1, 2, 3, 4, 5, 6];
        let png = icon_png(2, 1, 6, false, 3, &rgb).unwrap();
        assert_eq!(&png[..4], &[0x89, b'P', b'N', b'G']);
        // Rejections like zlaunch's validation.
        assert!(icon_png(0, 1, 4, true, 4, &pixels).is_none());
        assert!(icon_png(300, 1, 1200, true, 4, &[]).is_none());
        assert!(icon_png(2, 1, 8, true, 5, &pixels).is_none());
        assert!(icon_png(2, 1, 8, true, 4, &pixels[..4]).is_none());
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
            assert!(niri_list(&dir.path().join("socket")).is_err());
            worker.join().unwrap();
        }
        let dir = tempfile::tempdir().unwrap();
        let listener = std::os::unix::net::UnixListener::bind(dir.path().join("socket")).unwrap();
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            std::thread::sleep(Duration::from_millis(500));
        });
        assert!(niri_list(&dir.path().join("socket")).is_err());
        worker.join().unwrap();
        assert!(niri_list(&dir.path().join("missing")).is_err());
    }

    #[test]
    fn search_keeps_window_address_and_application_workspace_text() {
        let cfg = crate::config::Config {
            search_providers: None,
            ..crate::config::Config::default()
        };
        let history = crate::clip::History::new(0);
        let windows = [Window {
            address: "73".into(),
            title: "Documentation".into(),
            app_id: "org.mozilla.firefox".into(),
            workspace: 3,
            focused: false,
            icon_data: None,
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
        assert_eq!(rows[0].icon.as_deref(), Some("window:org.mozilla.firefox"));
        assert_eq!(rows[0].action, crate::items::Action::Window("73".into()));
    }

    #[test]
    fn windows_with_compositor_icons_use_the_data_icon_name() {
        let cfg = crate::config::Config::default();
        let history = crate::clip::History::new(0);
        let windows = [Window {
            address: "{uuid}".into(),
            title: "Document".into(),
            app_id: "firefox".into(),
            workspace: 1,
            focused: false,
            icon_data: Some(b"png".to_vec()),
        }];
        let search = crate::items::Search {
            cfg: &cfg,
            apps: &[],
            history: &history,
            windows: &windows,
        };
        let rows = search.run("firefox");
        assert_eq!(rows[0].icon.as_deref(), Some("window-data:{uuid}"));
    }

    #[test]
    fn detection_follows_zlaunch_order() {
        let _lock = crate::testutil::ENV_LOCK.lock().unwrap();
        let vars = [
            "HYPRLAND_INSTANCE_SIGNATURE",
            "KDE_SESSION_VERSION",
            "NIRI_SOCKET",
            "MANGO_INSTANCE_SIGNATURE",
        ];
        let saved: Vec<_> = vars.iter().map(|v| (*v, std::env::var_os(v))).collect();
        for (v, _) in &saved {
            std::env::remove_var(v);
        }
        assert!(matches!(detect(), Backend::Noop));
        std::env::set_var("MANGO_INSTANCE_SIGNATURE", "/tmp/mango.sock");
        assert!(matches!(detect(), Backend::Mango(_)));
        std::env::set_var("NIRI_SOCKET", "/tmp/niri.sock");
        assert!(matches!(detect(), Backend::Niri(_)));
        // No session bus in the test sandbox: KDE falls through to Niri.
        std::env::set_var("KDE_SESSION_VERSION", "6");
        assert!(matches!(detect(), Backend::Niri(_)));
        std::env::set_var("HYPRLAND_INSTANCE_SIGNATURE", "sig");
        assert!(matches!(detect(), Backend::Hyprland(_)));
        for (v, old) in saved {
            match old {
                Some(value) => std::env::set_var(v, value),
                None => std::env::remove_var(v),
            }
        }
    }
}
