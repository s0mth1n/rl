//! Clipboard history: bounded text and PNG copies, persisted under the XDG state directory.

pub const MAX_ENTRY: usize = 16 * 1024;
pub const MAX_IMAGE: usize = 4 * 1024 * 1024;
pub const SENSITIVE_MIME: &str = "x-kde-passwordManagerHint";

/// Which text mime type to ask for, if any. Sensitive offers require store_passwords.
pub fn pick_mime(mimes: &[String], store_passwords: bool) -> Option<String> {
    if !store_passwords
        && mimes
            .iter()
            .any(|m| m == SENSITIVE_MIME || m == "vicinae/concealed")
    {
        return None;
    }
    if mimes.iter().any(|m| m == "text/uri-list") {
        return Some("text/uri-list".to_string());
    }
    for want in [
        "text/plain;charset=utf-8",
        "text/plain",
        "UTF8_STRING",
        "STRING",
        "TEXT",
    ] {
        if mimes.iter().any(|m| m == want) {
            return Some(want.to_string());
        }
    }
    // An HTML-only copy is still taken: the tags are stripped after the read, so it lands as plain text.
    if mimes.iter().any(|m| m == "text/html") {
        return Some("text/html".to_string());
    }
    None
}

/// Plain text out of an HTML copy: tags dropped (block tags become line breaks), common entities decoded, lines tidied.
pub fn strip_html(h: &str) -> String {
    let mut out = String::with_capacity(h.len());
    let mut rest = h;
    while let Some(c) = rest.chars().next() {
        if c == '<' {
            if let Some(end) = rest.find('>') {
                let tag = rest[1..end]
                    .trim()
                    .trim_start_matches('/')
                    .split([' ', '/'])
                    .next()
                    .unwrap_or("")
                    .to_ascii_lowercase();
                if matches!(
                    tag.as_str(),
                    "br" | "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4"
                ) {
                    out.push('\n');
                }
                rest = &rest[end + 1..];
                continue;
            }
        }
        if c == '&' {
            if let Some(end) = rest.find(';') {
                if end < 10 {
                    let rep = match &rest[..=end] {
                        "&amp;" => Some('&'),
                        "&lt;" => Some('<'),
                        "&gt;" => Some('>'),
                        "&quot;" => Some('"'),
                        "&#39;" | "&apos;" => Some('\''),
                        "&nbsp;" => Some(' '),
                        _ => None,
                    };
                    if let Some(r) = rep {
                        out.push(r);
                        rest = &rest[end + 1..];
                        continue;
                    }
                }
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Which image mime type to ask for. An offered image wins over text.
pub fn pick_image(mimes: &[String], store_passwords: bool) -> Option<String> {
    if !store_passwords
        && mimes
            .iter()
            .any(|m| m == SENSITIVE_MIME || m == "vicinae/concealed")
    {
        return None;
    }
    mimes.iter().find(|m| *m == "image/png").cloned()
}

/// Width and height from a PNG header, None when the data does not start like a PNG.
pub fn png_size(d: &[u8]) -> Option<(u32, u32)> {
    if d.len() < 24 || d[..8] != [0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a] {
        return None;
    }
    let w = u32::from_be_bytes(d[16..20].try_into().ok()?);
    let h = u32::from_be_bytes(d[20..24].try_into().ok()?);
    if w == 0 || h == 0 || w > 8192 || h > 8192 {
        None
    } else {
        Some((w, h))
    }
}

pub fn timestamp(at: u64, now: u64) -> String {
    let Some(age) = now.checked_sub(at) else {
        return "Earlier".into();
    };
    match age {
        0..60 => "Just now".into(),
        60..3600 => {
            let minutes = age / 60;
            format!("{minutes} min{} ago", if minutes > 1 { "s" } else { "" })
        }
        3600..86400 => {
            let hours = age / 3600;
            format!("{hours} hour{} ago", if hours > 1 { "s" } else { "" })
        }
        _ => "Earlier".into(),
    }
}

pub fn preview(text: &str) -> String {
    let line = text.trim_start().lines().next().unwrap_or("");
    let short: String = line.chars().take(30).collect();
    if short.len() < line.len() {
        format!("{short}...")
    } else {
        short
    }
}

pub struct Img {
    pub id: u64,
    /// PNG bytes while held in memory (fresh copies); `None` once the image
    /// is stored, in which case `path` provides the bytes on demand, like
    /// zlaunch's ImageData::Stored.
    pub png: Option<Vec<u8>>,
    pub path: std::path::PathBuf,
    pub w: u32,
    pub h: u32,
    /// When it was copied, epoch seconds.
    pub at: u64,
}

impl Img {
    /// PNG bytes, loading them from disk on demand. A missing or damaged
    /// store reads as unavailable, like a dropped database row in zlaunch.
    pub fn bytes(&self) -> Option<Vec<u8>> {
        if let Some(png) = &self.png {
            return Some(png.clone());
        }
        let png = std::fs::read(&self.path).ok()?;
        if png.len() > MAX_IMAGE || png_size(&png) != Some((self.w, self.h)) || fnv(&png) != self.id
        {
            return None;
        }
        Some(png)
    }
}

/// A text copy and when it was taken (epoch seconds). `files` marks a
/// text/uri-list capture: `text` then holds one decoded path per line.
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub text: String,
    pub at: u64,
    pub files: bool,
}

/// Decode a `file://` URI to a filesystem path (percent-decoded), like zlaunch.
pub fn file_url_path(text: &str) -> Option<String> {
    let rest = text.trim().strip_prefix("file://")?;
    Some(percent_decode(rest))
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Paths from a text/uri-list copy: one URI per line, `#` comments skipped.
pub fn uri_list_paths(text: &str) -> Option<Vec<String>> {
    let mut paths = Vec::new();
    for line in text.lines() {
        let line = line.trim_end_matches('\r');
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        paths.push(file_url_path(line)?);
    }
    (!paths.is_empty()).then_some(paths)
}

/// List title for file paths: the file name, or "[N files]".
pub fn file_title(paths: &[String]) -> String {
    if paths.len() == 1 {
        std::path::Path::new(&paths[0])
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("[File]")
            .to_string()
    } else {
        format!("[{} files]", paths.len())
    }
}

/// Extensions zlaunch previews as images in the clipboard pane.
pub fn is_image_path(path: &str) -> bool {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    matches!(
        ext.as_deref(),
        Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "svg")
    )
}

/// zlaunch's colour text parsing: #rgb/#rrggbb/#rrggbbaa or rgb()/rgba().
pub fn parse_color(text: &str) -> Option<[u8; 4]> {
    let text = text.trim();
    if let Some(hex) = text.strip_prefix('#') {
        let c = |i: usize| u8::from_str_radix(&hex[i..i + 1].repeat(2), 16).ok();
        let p = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        return match hex.len() {
            3 => Some([c(0)?, c(1)?, c(2)?, 255]),
            6 => Some([p(0)?, p(2)?, p(4)?, 255]),
            8 => Some([p(0)?, p(2)?, p(4)?, p(6)?]),
            _ => None,
        };
    }
    let inner = text
        .strip_prefix("rgb(")
        .or_else(|| text.strip_prefix("rgba("))?;
    let inner = inner.strip_suffix(')')?;
    let parts: Vec<&str> = inner.split(',').map(str::trim).collect();
    if parts.len() < 3 {
        return None;
    }
    let v = |i: usize| parts[i].parse::<u8>().ok();
    let a = if parts.len() >= 4 {
        (parts[3].parse::<f32>().ok()? * 255.0) as u8
    } else {
        255
    };
    Some([v(0)?, v(1)?, v(2)?, a])
}

/// zlaunch's per-extension phosphor icon for a file path.
pub fn clip_file_icon(path: &str) -> &'static str {
    if is_image_path(path) {
        "phosphor:file-image"
    } else if is_text_path(path) {
        "phosphor:file-text"
    } else {
        "phosphor:file"
    }
}

/// Extensions zlaunch previews as text in the clipboard pane.
pub fn is_text_path(path: &str) -> bool {
    let ext = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());
    matches!(
        ext.as_deref(),
        Some(
            "txt"
                | "md"
                | "rs"
                | "py"
                | "js"
                | "ts"
                | "json"
                | "yaml"
                | "yml"
                | "toml"
                | "html"
                | "css"
                | "sh"
        )
    )
}

/// Epoch seconds, strictly increasing within the process so the newest copy
/// always sorts ahead of older ones, like zlaunch's single history deque.
pub fn now() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static LAST: AtomicU64 = AtomicU64::new(0);
    let real = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let mut prev = LAST.load(Ordering::Relaxed);
    loop {
        let next = real.max(prev.saturating_add(1));
        match LAST.compare_exchange_weak(prev, next, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => return next,
            Err(p) => prev = p,
        }
    }
}

fn fnv(d: &[u8]) -> u64 {
    d.iter().fold(0xcbf29ce484222325u64, |h, b| {
        (h ^ *b as u64).wrapping_mul(0x100000001b3)
    })
}

/// Where the history is kept between runs: one index file, one PNG per image.
pub fn dir() -> std::path::PathBuf {
    let base = std::env::var("XDG_STATE_HOME")
        .unwrap_or_else(|_| format!("{}/.local/state", std::env::var("HOME").unwrap_or_default()));
    std::path::PathBuf::from(base).join("zl/clipboard")
}

/// Tab and newline cannot appear raw in the index, so they are escaped.
pub(crate) fn esc(t: &str) -> String {
    t.replace('\\', "\\\\")
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}
pub(crate) fn unesc(t: &str) -> String {
    let mut out = String::with_capacity(t.len());
    let mut c = t.chars();
    while let Some(x) = c.next() {
        if x == '\\' {
            match c.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('\\') => out.push('\\'),
                Some(o) => {
                    out.push('\\');
                    out.push(o);
                }
                None => out.push('\\'),
            }
        } else {
            out.push(x);
        }
    }
    out
}

#[derive(Default)]
pub struct History {
    pub entries: Vec<Clip>,
    pub images: Vec<Img>,
    pub max: usize,
}

impl History {
    pub fn new(max: usize) -> Self {
        History {
            entries: Vec::new(),
            images: Vec::new(),
            max,
        }
    }

    /// Add a copy at the front. Blank and oversized text is dropped, and a repeat of the latest copy is not added again (zlaunch data.rs).
    pub fn push(&mut self, text: &str) -> bool {
        self.push_kind(text, false)
    }

    /// Add a text/uri-list capture: decoded paths, one per line.
    pub fn push_files(&mut self, text: &str) -> bool {
        self.push_kind(text, true)
    }

    /// True when the newest copy overall is an image: zlaunch dedups against
    /// the front of one shared deque, not per content kind.
    fn front_is_image(&self) -> bool {
        match (self.entries.first(), self.images.first()) {
            (Some(e), Some(i)) => i.at > e.at,
            (None, Some(_)) => true,
            _ => false,
        }
    }

    fn push_kind(&mut self, text: &str, files: bool) -> bool {
        if text.trim().is_empty() || text.len() > MAX_ENTRY || self.max == 0 {
            return false;
        }
        // zlaunch skips consecutive duplicates only; older equal entries stay.
        if !self.front_is_image()
            && self
                .entries
                .first()
                .is_some_and(|e| e.text == text && e.files == files)
        {
            return false;
        }
        self.entries.insert(
            0,
            Clip {
                text: text.to_string(),
                at: now(),
                files,
            },
        );
        self.trim();
        true
    }

    /// Drop the oldest copies past the configured limit. One bound spans
    /// text and images like zlaunch's single deque, not a cap per kind.
    pub fn trim(&mut self) {
        while self.entries.len() + self.images.len() > self.max {
            let oldest_text = self.entries.last().map(|e| e.at);
            let oldest_image = self.images.last().map(|i| i.at);
            match (oldest_text, oldest_image) {
                (Some(t), Some(i)) if i < t => {
                    self.images.pop();
                }
                (Some(_), Some(_)) | (Some(_), None) => {
                    self.entries.pop();
                }
                (None, Some(_)) => {
                    self.images.pop();
                }
                (None, None) => break,
            }
        }
    }

    /// Add a PNG copy at the front. Anything that is not a sane PNG, or is too big, is dropped.
    pub fn push_image(&mut self, png: &[u8]) -> bool {
        if self.max == 0 || png.len() > MAX_IMAGE {
            return false;
        }
        let Some((w, h)) = png_size(png) else {
            return false;
        };
        let id = fnv(png);
        // zlaunch skips consecutive duplicates only; older equal images stay.
        if self.front_is_image() && self.images.first().is_some_and(|i| i.id == id) {
            return false;
        }
        self.images.insert(
            0,
            Img {
                id,
                png: Some(png.to_vec()),
                path: dir().join(format!("{id}.png")),
                w,
                h,
                at: now(),
            },
        );
        self.trim();
        true
    }

    /// Write the history to disk so a restart does not lose it.
    pub fn save(&self) {
        self.save_to(&dir())
    }

    pub fn save_to(&self, d: &std::path::Path) {
        use std::io::Write;
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        if std::fs::create_dir_all(d).is_err() {
            return;
        }
        // Copied text can hold secrets: the folder and everything in it are for the user alone.
        if std::fs::set_permissions(d, std::fs::Permissions::from_mode(0o700)).is_err() {
            return;
        }
        let mut s = String::from("v1\n");
        for e in &self.entries {
            let kind = if e.files { 'f' } else { 't' };
            s.push_str(&format!("{kind}\t{}\t{}\n", e.at, esc(&e.text)));
        }
        for i in &self.images {
            s.push_str(&format!("i\t{}\t{}\t{}\t{}\n", i.at, i.id, i.w, i.h));
            let p = d.join(format!("{}.png", i.id));
            if let Some(png) = &i.png {
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(true)
                    .mode(0o600)
                    .open(&p)
                {
                    if file
                        .set_permissions(std::fs::Permissions::from_mode(0o600))
                        .is_ok()
                    {
                        let _ = file.write_all(png);
                    }
                }
            } else if !p.exists() && i.path.is_file() {
                // Stored image saved under another directory: carry the file.
                if std::fs::copy(&i.path, &p).is_ok() {
                    let _ = std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o600));
                }
            }
        }
        if let Ok(rd) = std::fs::read_dir(d) {
            for f in rd.flatten() {
                let n = f.file_name();
                if let Some(id) = n
                    .to_string_lossy()
                    .strip_suffix(".png")
                    .and_then(|x| x.parse::<u64>().ok())
                {
                    if !self.images.iter().any(|i| i.id == id) {
                        let _ = std::fs::remove_file(f.path());
                    }
                }
            }
        }
        let tmp = d.join("state.tmp");
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
        {
            if file
                .set_permissions(std::fs::Permissions::from_mode(0o600))
                .is_ok()
                && file.write_all(s.as_bytes()).is_ok()
            {
                let _ = std::fs::rename(&tmp, d.join("state"));
            }
        }
    }

    /// Read a saved history. Anything malformed or oversized is skipped.
    pub fn load(max: usize) -> History {
        Self::load_from(&dir(), max)
    }

    pub fn load_from(d: &std::path::Path, max: usize) -> History {
        let mut h = History::new(max);
        let Ok(s) = std::fs::read_to_string(d.join("state")) else {
            return h;
        };
        for l in s.lines().skip(1) {
            let f: Vec<&str> = l.split('\t').collect();
            match f.as_slice() {
                [kind @ ("t" | "f"), at, t] => {
                    if let Ok(at) = at.parse() {
                        let text = unesc(t);
                        if !text.trim().is_empty() && text.len() <= MAX_ENTRY {
                            h.entries.push(Clip {
                                text,
                                at,
                                files: *kind == "f",
                            });
                        }
                    }
                }
                ["i", at, id, w, hh] => {
                    if let (Ok(at), Ok(id), Ok(w), Ok(hh)) =
                        (at.parse(), id.parse(), w.parse(), hh.parse())
                    {
                        // Bytes load on demand like zlaunch's stored images;
                        // only the file's presence and size are checked here.
                        let path = d.join(format!("{id}.png"));
                        if let Ok(meta) = std::fs::metadata(&path) {
                            if (24..=MAX_IMAGE as u64).contains(&meta.len()) {
                                h.images.push(Img {
                                    id,
                                    png: None,
                                    path,
                                    w,
                                    h: hh,
                                    at,
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        h.trim();
        h
    }

    /// Remove the entry an action points at. False when it is not a history entry or is already gone.
    pub fn remove(&mut self, action: &crate::items::Action) -> bool {
        use crate::items::Action;
        let (a, b) = (self.entries.len(), self.images.len());
        match action {
            Action::Clip(t) => self.entries.retain(|e| &e.text != t),
            Action::ClipImg(id) => self.images.retain(|i| i.id != *id),
            _ => return false,
        }
        a != self.entries.len() || b != self.images.len()
    }

    #[cfg(test)]
    pub fn clear(&mut self) {
        self.entries.clear();
        self.images.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn v(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| s.to_string()).collect()
    }
    #[test]
    fn timestamps_follow_clipboard_age() {
        for (age, expected) in [
            (0, "Just now"),
            (59, "Just now"),
            (60, "1 min ago"),
            (120, "2 mins ago"),
            (3599, "59 mins ago"),
            (3600, "1 hour ago"),
            (7200, "2 hours ago"),
            (86399, "23 hours ago"),
            (86400, "Earlier"),
        ] {
            assert_eq!(timestamp(100000 - age, 100000), expected);
        }
        assert_eq!(timestamp(100001, 100000), "Earlier");
        assert_eq!(preview("  two\nlines\twith   space"), "two");
        assert_eq!(preview(&"é".repeat(101)), format!("{}...", "é".repeat(30)));
    }

    #[test]
    fn clipboard_preview_uses_the_first_line_and_thirty_unicode_characters() {
        assert_eq!(preview(""), "");
        assert_eq!(preview(" \t\n"), "");
        assert_eq!(preview("\n  first\r\nsecond"), "first");
        assert_eq!(preview("one  two\tthree\nsecond"), "one  two\tthree");
        assert_eq!(preview(&"é".repeat(30)), "é".repeat(30));
        assert_eq!(preview(&"é".repeat(31)), format!("{}...", "é".repeat(30)));
        assert_eq!(preview(&"a".repeat(31)), format!("{}...", "a".repeat(30)));
    }
    #[test]
    fn removing_one_copy_keeps_the_others() {
        let mut history = History::new(4);
        history.push("first");
        history.push("second");
        assert!(history.remove(&crate::items::Action::Clip("first".into())));
        assert_eq!(texts(&history), v(&["second"]));
        assert!(!history.remove(&crate::items::Action::Clip("missing".into())));
        assert_eq!(texts(&history), v(&["second"]));
        history.images.push(Img {
            id: 1,
            png: Some(vec![]),
            path: std::path::PathBuf::new(),
            w: 1,
            h: 1,
            at: 0,
        });
        history.images.push(Img {
            id: 2,
            png: Some(vec![]),
            path: std::path::PathBuf::new(),
            w: 1,
            h: 1,
            at: 0,
        });
        assert!(history.remove(&crate::items::Action::ClipImg(1)));
        assert_eq!(history.images.iter().map(|i| i.id).collect::<Vec<_>>(), [2]);
        assert!(!history.remove(&crate::items::Action::ClipImg(3)));
        assert_eq!(history.images.len(), 1);
    }

    #[test]
    fn mime_choice() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        assert_eq!(
            pick_mime(
                &v(&["image/png", "text/plain", "text/plain;charset=utf-8"]),
                false
            )
            .as_deref(),
            Some("text/plain;charset=utf-8")
        );
        assert_eq!(pick_mime(&v(&["STRING"]), false).as_deref(), Some("STRING"));
        assert!(pick_mime(&v(&["image/png"]), false).is_none());
    }
    #[test]
    fn password_manager_copies_are_skipped() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        assert!(pick_mime(&v(&["text/plain", "x-kde-passwordManagerHint"]), false).is_none());
        assert!(pick_mime(&v(&["text/plain", "vicinae/concealed"]), false).is_none());
        assert!(pick_image(&v(&["image/png", "vicinae/concealed"]), false).is_none());
    }
    #[test]
    fn password_storage_requires_explicit_opt_in_for_text_html_and_images() {
        for hint in [SENSITIVE_MIME, "vicinae/concealed"] {
            for mime in ["text/plain", "text/html"] {
                let offer = v(&[mime, hint]);
                assert_eq!(pick_mime(&offer, false), None);
                assert_eq!(pick_mime(&offer, true).as_deref(), Some(mime));
                assert_eq!(pick_image(&offer, true), None);
            }
            let mixed = v(&["image/png", "text/plain", hint]);
            assert_eq!(pick_image(&mixed, false), None);
            assert_eq!(pick_mime(&mixed, false), None);
            assert_eq!(pick_image(&mixed, true).as_deref(), Some("image/png"));
            let offer = v(&["image/png", hint]);
            assert_eq!(pick_image(&offer, false), None);
            assert_eq!(pick_image(&offer, true).as_deref(), Some("image/png"));
            assert_eq!(pick_mime(&offer, true), None);
        }
    }

    #[test]
    fn images_are_checked_and_bounded() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let mut png = vec![
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
        ];
        png.extend_from_slice(&[0, 0, 1, 0, 0, 0, 0, 64]);
        assert_eq!(png_size(&png), Some((256, 64)));
        let mut h = History::new(5);
        assert!(h.push_image(&png));
        assert!(!h.push_image(b"not a png at all, just some bytes here"));
        assert!(!h.push_image(&png) && h.images.len() == 1);
        assert_eq!(
            pick_image(&v(&["image/png"]), false).as_deref(),
            Some("image/png")
        );
        assert_eq!(
            pick_image(&v(&["image/png", "text/plain"]), false).as_deref(),
            Some("image/png")
        );
        assert!(pick_image(&v(&["image/png", "x-kde-passwordManagerHint"]), false).is_none());
        h.clear();
        assert!(h.images.is_empty());
    }
    fn texts(h: &History) -> Vec<String> {
        h.entries.iter().map(|e| e.text.clone()).collect()
    }
    #[test]
    fn saved_files_are_private() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        use std::os::unix::fs::PermissionsExt;
        let d = std::env::temp_dir().join(format!("zl-clip-perms-{}", std::process::id()));
        let mut h = History::new(9);
        h.push("secret note");
        let mut png = vec![
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
        ];
        png.extend_from_slice(&[0, 0, 1, 0, 0, 0, 0, 64]);
        assert!(h.push_image(&png));
        h.save_to(&d);
        let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(&d), 0o700);
        assert_eq!(mode(&d.join("state")), 0o600);
        assert_eq!(mode(&d.join(format!("{}.png", h.images[0].id))), 0o600);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn save_and_load_round_trip() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let d = std::env::temp_dir().join(format!("zl-clip-test-{}", std::process::id()));
        let mut h = History::new(9);
        h.push("plain");
        h.push("two\nlines\tand tab");
        h.push("back \\ slash");
        let mut png = vec![
            0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 13, b'I', b'H', b'D', b'R',
        ];
        png.extend_from_slice(&[0, 0, 1, 0, 0, 0, 0, 64]);
        assert!(h.push_image(&png));
        h.save_to(&d);
        let l = History::load_from(&d, 9);
        assert_eq!(texts(&l), texts(&h));
        assert_eq!(l.images.len(), 1);
        assert_eq!(l.images[0].bytes(), h.images[0].bytes());
        assert_eq!(l.images[0].id, h.images[0].id);
        let _ = std::fs::remove_dir_all(&d);
    }
    #[test]
    fn html_copies_become_plain_text() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        assert_eq!(
            pick_mime(&v(&["text/html"]), false).as_deref(),
            Some("text/html")
        );
        assert_eq!(
            pick_mime(&v(&["text/html", "x-kde-passwordManagerHint"]), false).as_deref(),
            None
        );
        assert_eq!(
            pick_mime(&v(&["text/html", "text/plain"]), false).as_deref(),
            Some("text/plain")
        );
        assert_eq!(
            strip_html("<p>Hello <b>big</b> world</p><p>Second &amp; third</p>"),
            "Hello big world\nSecond & third"
        );
        assert_eq!(strip_html("no tags, just text"), "no tags, just text");
        assert_eq!(
            strip_html("fish &lt; chips &gt; salad &quot;always&quot; &#39;fresh&#39;&nbsp;daily"),
            "fish < chips > salad \"always\" 'fresh' daily"
        );
        assert!(strip_html("<div></div>").is_empty());
        assert_eq!(strip_html("AT&T; verizon"), "AT&T; verizon");
    }
    fn png(w: u32, h: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        let mut encoder = png::Encoder::new(&mut bytes, w, h);
        encoder.set_color(png::ColorType::Rgba);
        let mut writer = encoder.write_header().unwrap();
        writer
            .write_image_data(&vec![255; (w * h * 4) as usize])
            .unwrap();
        writer.finish().unwrap();
        bytes
    }
    #[test]
    fn text_limit_counts_bytes_and_rejections_leave_history_unchanged() {
        let mut h = History::new(2);
        let boundary = "é".repeat(MAX_ENTRY / 2);
        assert_eq!(boundary.len(), MAX_ENTRY);
        assert!(h.push("older"));
        assert!(h.push(&boundary));
        let before = h.entries.clone();
        for rejected in [format!("{boundary}a"), " \t\n".into(), String::new()] {
            assert!(!h.push(&rejected));
            assert_eq!(h.entries, before);
        }
    }
    #[test]
    fn zero_limit_rejects_both_kinds_and_trim_removes_existing_entries() {
        let mut h = History::new(2);
        let image = png(1, 1);
        assert!(h.push("kept"));
        assert!(h.push_image(&image));
        h.max = 0;
        h.trim();
        assert!(h.entries.is_empty());
        assert!(h.images.is_empty());
        assert!(!h.push("dropped"));
        assert!(!h.push_image(&image));
    }
    #[test]
    fn image_cap_matches_the_history_limit() {
        // zlaunch bounds images by the same max as text, not a smaller cap.
        let mut h = History::new(3);
        for width in 1..=5 {
            assert!(h.push_image(&png(width, 1)));
        }
        assert_eq!(
            h.images.iter().map(|i| i.w).collect::<Vec<_>>(),
            vec![5, 4, 3]
        );
        let before = h.images.iter().map(|i| i.id).collect::<Vec<_>>();
        assert!(!h.push_image(&vec![0; MAX_IMAGE + 1]));
        assert_eq!(h.images.iter().map(|i| i.id).collect::<Vec<_>>(), before);
    }
    #[test]
    fn history_bound_spans_text_and_images() {
        // zlaunch's single deque drops the globally oldest copy past the
        // limit, whatever its content kind.
        let mut h = History::new(3);
        assert!(h.push("one"));
        assert!(h.push_image(&png(1, 1)));
        assert!(h.push("two"));
        assert!(h.push_image(&png(2, 1)));
        assert_eq!(texts(&h), vec!["two"]);
        assert_eq!(h.images.iter().map(|i| i.w).collect::<Vec<_>>(), vec![2, 1]);
        assert!(!h.push_image(&png(2, 1)));
        assert!(h.push("three"));
        assert_eq!(texts(&h), vec!["three", "two"]);
        assert_eq!(h.images.iter().map(|i| i.w).collect::<Vec<_>>(), vec![2]);
    }
    #[test]
    fn dedup_compares_only_the_latest_copy_across_kinds() {
        let mut h = History::new(9);
        assert!(h.push_image(&png(1, 1)));
        assert!(h.push("break"));
        // The same image again is no longer consecutive, so it is kept.
        assert!(h.push_image(&png(1, 1)));
        assert_eq!(h.images.len(), 2);
        assert!(!h.push_image(&png(1, 1)));
        assert!(h.push_files("/tmp/a"));
        assert!(!h.push_files("/tmp/a"));
        // A file-path copy and plain text are different content kinds.
        assert!(h.push("/tmp/a"));
        assert!(!h.push("/tmp/a"));
        assert_eq!(texts(&h), vec!["/tmp/a", "/tmp/a", "break"]);
    }
    #[test]
    fn stored_images_load_their_bytes_on_demand() {
        let d = tempfile::tempdir().unwrap();
        let mut h = History::new(3);
        assert!(h.push_image(&png(1, 1)));
        h.save_to(d.path());
        let l = History::load_from(d.path(), 3);
        assert_eq!(l.images.len(), 1);
        assert!(l.images[0].png.is_none());
        assert_eq!(l.images[0].bytes(), h.images[0].bytes());
        // A damaged store reads as unavailable instead of loading garbage.
        std::fs::write(d.path().join(format!("{}.png", l.images[0].id)), b"corrupt").unwrap();
        assert_eq!(l.images[0].bytes(), None);
    }
    #[test]
    fn only_consecutive_duplicates_are_dropped() {
        let mut h = History::new(9);
        assert!(h.push("same"));
        assert!(!h.push("same"));
        assert!(h.push("other"));
        assert!(h.push("same"));
        assert_eq!(texts(&h), vec!["same", "other", "same"]);
        let mut h = History::new(9);
        assert!(h.push_image(&png(1, 1)));
        assert!(!h.push_image(&png(1, 1)));
        assert!(h.push_image(&png(2, 1)));
        assert!(h.push_image(&png(1, 1)));
        assert_eq!(
            h.images.iter().map(|i| i.w).collect::<Vec<_>>(),
            vec![1, 2, 1]
        );
    }
    #[test]
    fn concealed_hints_override_every_supported_mime_order() {
        let formats = [
            "text/plain;charset=utf-8",
            "text/plain",
            "UTF8_STRING",
            "STRING",
            "TEXT",
            "text/html",
            "image/png",
        ];
        for hint in [SENSITIVE_MIME, "vicinae/concealed"] {
            for format in formats {
                for mimes in [v(&[hint, format]), v(&[format, hint])] {
                    assert_eq!(pick_mime(&mimes, false), None, "{mimes:?}");
                    assert_eq!(pick_image(&mimes, false), None, "{mimes:?}");
                }
            }
        }
        assert_eq!(pick_mime(&[], false), None);
        assert_eq!(pick_image(&[], false), None);
    }
    #[test]
    fn malformed_text_records_are_skipped_and_load_obeys_limit() {
        let d = tempfile::tempdir().unwrap();
        let text = format!("v1\nt\tbad-time\tbad\nt\t1\t   \nt\t2\t{}\nt\t3\tnew\\nline\nt\t4\tolder\ni\t5\t999\t1\t1\nunknown\t6\tignored\n", "x".repeat(MAX_ENTRY + 1));
        std::fs::write(d.path().join("state"), text).unwrap();
        let h = History::load_from(d.path(), 1);
        assert_eq!(
            h.entries,
            vec![Clip {
                text: "new\nline".into(),
                at: 3,
                files: false,
            }]
        );
        assert!(h.images.is_empty());
        let disabled = History::load_from(d.path(), 0);
        assert!(disabled.entries.is_empty());
        assert!(disabled.images.is_empty());
    }
    #[test]
    fn saving_after_clear_removes_old_images_and_text() {
        let d = tempfile::tempdir().unwrap();
        let mut h = History::new(2);
        assert!(h.push("private"));
        assert!(h.push_image(&png(1, 1)));
        h.save_to(d.path());
        let image = d.path().join(format!("{}.png", h.images[0].id));
        assert!(image.is_file());
        assert!(d.path().join("state").is_file());
        h.clear();
        h.save_to(d.path());
        assert!(!image.exists());
        assert_eq!(std::fs::read(d.path().join("state")).unwrap(), b"v1\n");
        let loaded = History::load_from(d.path(), 2);
        assert!(loaded.entries.is_empty());
        assert!(loaded.images.is_empty());
    }
    #[test]
    fn deleting_one_entry_keeps_other_entries_and_persists_the_change() {
        use crate::items::Action;
        let d = tempfile::tempdir().unwrap();
        let mut h = History::new(4);
        assert!(h.push("first"));
        assert!(h.push("second"));
        assert!(h.push_image(&png(1, 1)));
        assert!(h.push_image(&png(2, 1)));
        let image_ids = h.images.iter().map(|i| i.id).collect::<Vec<_>>();
        h.save_to(d.path());
        assert!(h.remove(&Action::Clip("first".into())));
        assert_eq!(texts(&h), v(&["second"]));
        assert!(!h.remove(&Action::Clip("first".into())));
        assert_eq!(h.images.iter().map(|i| i.id).collect::<Vec<_>>(), image_ids);
        assert!(h.remove(&Action::ClipImg(image_ids[0])));
        assert!(!h.remove(&Action::ClipImg(image_ids[0])));
        assert_eq!(
            h.images.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![image_ids[1]]
        );
        h.save_to(d.path());
        assert!(!d.path().join(format!("{}.png", image_ids[0])).exists());
        assert!(d.path().join(format!("{}.png", image_ids[1])).is_file());
        let loaded = History::load_from(d.path(), 3);
        assert_eq!(loaded.entries, h.entries);
        assert_eq!(
            loaded.images.iter().map(|i| i.id).collect::<Vec<_>>(),
            vec![image_ids[1]]
        );
        assert_eq!(loaded.images[0].bytes(), h.images[0].bytes());
    }
    #[test]
    fn non_clipboard_actions_cannot_remove_history() {
        use crate::items::Action;
        let mut h = History::new(1);
        assert!(h.push("kept"));
        assert!(h.push_image(&png(1, 1)));
        let before = h.entries.clone();
        let image = h.images[0].id;
        for action in [
            Action::App(0),
            Action::View(";".into()),
            Action::ClipImg(u64::MAX),
            Action::Clip("missing".into()),
        ] {
            assert!(!h.remove(&action), "{action:?}");
            assert_eq!(h.entries, before);
            assert_eq!(h.images.len(), 1);
            assert_eq!(h.images[0].id, image);
        }
    }
    #[test]
    fn index_escaping_round_trips_unicode_and_literal_backslashes() {
        let text = "λ\t\n\\n\\t\\\\end\\";
        assert_eq!(esc(text), "λ\\t\\n\\\\n\\\\t\\\\\\\\end\\\\");
        assert_eq!(unesc(&esc(text)), text);
        assert_eq!(unesc("\\q\\"), "\\q\\");
    }
    #[test]
    fn bounded_and_deduped() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let mut h = History::new(3);
        for t in ["a", "b", "c", "d"] {
            assert!(h.push(t));
        }
        assert_eq!(texts(&h), v(&["d", "c", "b"]));
        h.push("b");
        assert_eq!(texts(&h), v(&["b", "d", "c"]));
        assert!(h.entries[0].at > 0);
        assert!(!h.push("   "));
        assert!(!h.push(&"x".repeat(MAX_ENTRY + 1)));
        h.clear();
        assert!(h.entries.is_empty());
    }
}
