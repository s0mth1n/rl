//! Clipboard history: bounded text and PNG copies, persisted under the XDG state directory.

pub const MAX_ENTRY: usize = 16 * 1024;
pub const MAX_IMAGE: usize = 4 * 1024 * 1024;
pub const MAX_IMAGES: usize = 8;
pub const SENSITIVE_MIME: &str = "x-kde-passwordManagerHint";

/// Which text mime type to ask for, if any. Offers with password-manager or concealed hints are refused.
pub fn pick_mime(mimes: &[String]) -> Option<String> {
    if mimes
        .iter()
        .any(|m| m == SENSITIVE_MIME || m == "vicinae/concealed")
    {
        return None;
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

/// Which image mime type to ask for. Text wins when an offer has both, and sensitive copies are refused.
pub fn pick_image(mimes: &[String]) -> Option<String> {
    if mimes
        .iter()
        .any(|m| m == SENSITIVE_MIME || m == "vicinae/concealed")
        || pick_mime(mimes).is_some()
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

pub struct Img {
    pub id: u64,
    pub png: Vec<u8>,
    pub w: u32,
    pub h: u32,
    /// When it was copied, epoch seconds.
    pub at: u64,
}

/// A text copy and when it was taken (epoch seconds).
#[derive(Clone, Debug, PartialEq)]
pub struct Clip {
    pub text: String,
    pub at: u64,
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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

    /// Add a copy at the front. Blank and oversized text is dropped, repeats move to the front and count as copied again.
    pub fn push(&mut self, text: &str) -> bool {
        if text.trim().is_empty() || text.len() > MAX_ENTRY || self.max == 0 {
            return false;
        }
        self.entries.retain(|e| e.text != text);
        self.entries.insert(
            0,
            Clip {
                text: text.to_string(),
                at: now(),
            },
        );
        self.trim();
        true
    }

    /// Drop the oldest entries past the configured limits.
    pub fn trim(&mut self) {
        self.entries.truncate(self.max);
        self.images
            .truncate(if self.max == 0 { 0 } else { MAX_IMAGES });
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
        self.images.retain(|i| i.id != id);
        self.images.insert(
            0,
            Img {
                id,
                png: png.to_vec(),
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
            s.push_str(&format!("t\t{}\t{}\n", e.at, esc(&e.text)));
        }
        for i in &self.images {
            s.push_str(&format!("i\t{}\t{}\t{}\t{}\n", i.at, i.id, i.w, i.h));
            let p = d.join(format!("{}.png", i.id));
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
                    let _ = file.write_all(&i.png);
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
                ["t", at, t] => {
                    if let Ok(at) = at.parse() {
                        let text = unesc(t);
                        if !text.trim().is_empty() && text.len() <= MAX_ENTRY {
                            h.entries.push(Clip { text, at });
                        }
                    }
                }
                ["i", at, id, w, hh] => {
                    if let (Ok(at), Ok(id), Ok(w), Ok(hh)) =
                        (at.parse(), id.parse(), w.parse(), hh.parse())
                    {
                        if let Ok(png) = std::fs::read(d.join(format!("{id}.png"))) {
                            if png.len() <= MAX_IMAGE {
                                h.images.push(Img {
                                    id,
                                    png,
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
    fn mime_choice() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        assert_eq!(
            pick_mime(&v(&["image/png", "text/plain", "text/plain;charset=utf-8"])).as_deref(),
            Some("text/plain;charset=utf-8")
        );
        assert_eq!(pick_mime(&v(&["STRING"])).as_deref(), Some("STRING"));
        assert!(pick_mime(&v(&["image/png"])).is_none());
    }
    #[test]
    fn password_manager_copies_are_skipped() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        assert!(pick_mime(&v(&["text/plain", "x-kde-passwordManagerHint"])).is_none());
        assert!(pick_mime(&v(&["text/plain", "vicinae/concealed"])).is_none());
        assert!(pick_image(&v(&["image/png", "vicinae/concealed"])).is_none());
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
        assert!(h.push_image(&png) && h.images.len() == 1);
        assert_eq!(pick_image(&v(&["image/png"])).as_deref(), Some("image/png"));
        assert!(pick_image(&v(&["image/png", "text/plain"])).is_none());
        assert!(pick_image(&v(&["image/png", "x-kde-passwordManagerHint"])).is_none());
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
        assert_eq!(l.images[0].png, h.images[0].png);
        assert_eq!(l.images[0].id, h.images[0].id);
        let _ = std::fs::remove_dir_all(&d);
    }
    #[test]
    fn html_copies_become_plain_text() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        assert_eq!(pick_mime(&v(&["text/html"])).as_deref(), Some("text/html"));
        assert_eq!(
            pick_mime(&v(&["text/html", "x-kde-passwordManagerHint"])).as_deref(),
            None
        );
        assert_eq!(
            pick_mime(&v(&["text/html", "text/plain"])).as_deref(),
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
