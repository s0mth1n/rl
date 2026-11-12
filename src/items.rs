//! Applications, emoji and clipboard rows for the basic launcher.
use crate::{clip, config::Config};

#[derive(Clone, Debug)]
pub struct App {
    pub name: String,
    pub comment: String,
    pub exec: String,
    pub icon: String,
    pub terminal: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    App(usize),
    Clip(String),
    ClipImg(u64),
    View(String),
    Setting(String),
    Open(String),
    /// A row that only informs, such as "No files found".
    Nothing,
    Exclude(String),
    ClearClip,
    PinToggle(String),
    ResetRank(String),
}
#[derive(Clone, Debug)]
pub struct Item {
    pub title: String,
    pub sub: String,
    pub tag: &'static str,
    pub icon: Option<String>,
    pub action: Action,
    /// Promoted to the top as the best match for the query.
    pub best: bool,
}

pub fn scan_apps(cfg: &Config) -> Vec<App> {
    let home = std::env::var("XDG_DATA_HOME")
        .unwrap_or_else(|_| format!("{}/.local/share", std::env::var("HOME").unwrap_or_default()));
    let mut bases = vec![home];
    bases.extend(
        std::env::var("XDG_DATA_DIRS")
            .unwrap_or("/usr/local/share:/usr/share".into())
            .split(':')
            .filter(|p| !p.is_empty())
            .map(str::to_owned),
    );
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let mut desktop_ids = std::collections::HashSet::new();
    for base in bases {
        let root = std::path::Path::new(&base).join("applications");
        let mut pending = vec![root.clone()];
        let mut paths = Vec::new();
        while let Some(dir) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let Ok(kind) = entry.file_type() else {
                    continue;
                };
                if kind.is_dir() {
                    pending.push(entry.path());
                } else if entry.path().extension().is_some_and(|ext| ext == "desktop") {
                    paths.push(entry.path());
                }
            }
        }
        paths.sort();
        for path in paths {
            let id = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('/', "-");
            if !desktop_ids.insert(id) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(path) else {
                continue;
            };
            if let Some(app) = parse_desktop(&text) {
                if (!app.terminal || !cfg.terminal.is_empty()) && seen.insert(app.name.clone()) {
                    out.push(app);
                }
            }
        }
    }
    out.sort_by_cached_key(|a| a.name.to_lowercase());
    out
}

pub struct Search<'a> {
    pub cfg: &'a Config,
    pub apps: &'a [App],
    pub files: &'a [String],
    pub history: &'a clip::History,
}
impl Search<'_> {
    pub fn run(&self, query: &str) -> Vec<Item> {
        let q = query.trim();
        if q == "settings" {
            return crate::settings::rows(self.cfg);
        }
        if let Some(filter) = q.strip_prefix("settings apps") {
            return self
                .apps
                .iter()
                .filter(|a| filter.trim().is_empty() || fuzzy(filter.trim(), &a.name).is_some())
                .map(|a| Item {
                    title: a.name.clone(),
                    sub: if self.cfg.disabled_apps.contains(&a.name) {
                        "Hidden from search - Enter to show"
                    } else {
                        "Shown in search - Enter to hide"
                    }
                    .into(),
                    tag: if self.cfg.disabled_apps.contains(&a.name) {
                        "Excluded"
                    } else {
                        "Included"
                    },
                    icon: Some(a.icon.clone()),
                    action: Action::Exclude(a.name.clone()),
                    best: false,
                })
                .collect();
        }
        if let Some(rest) = q.strip_prefix('/') {
            let filter = FILTER.load(std::sync::atomic::Ordering::Relaxed);
            return file_items(self.files, rest.trim(), filter);
        }
        if let Some(filter) = q.strip_prefix(':') {
            return emoji_items(filter.trim());
        }
        if let Some(filter) = q.strip_prefix(';') {
            let mut rows: Vec<_> = self
                .history
                .entries
                .iter()
                .filter(|c| filter.trim().is_empty() || fuzzy(filter.trim(), &c.text).is_some())
                .map(|c| Item {
                    title: c
                        .text
                        .lines()
                        .next()
                        .unwrap_or("")
                        .chars()
                        .take(100)
                        .collect(),
                    sub: "Copy text to clipboard".into(),
                    tag: "Clipboard",
                    icon: None,
                    action: Action::Clip(c.text.clone()),
                    best: false,
                })
                .collect();
            if filter.trim().is_empty() {
                rows.extend(self.history.images.iter().map(|i| Item {
                    title: format!("Image {} x {}", i.w, i.h),
                    sub: "Copy PNG to clipboard".into(),
                    tag: "Clipboard",
                    icon: None,
                    action: Action::ClipImg(i.id),
                    best: false,
                }));
                rows.push(Item {
                    title: "Clear clipboard history".into(),
                    sub: "Remove saved text and images".into(),
                    tag: "Clipboard",
                    icon: None,
                    action: Action::ClearClip,
                    best: false,
                });
            }
            return rows;
        }
        let mut scored = Vec::new();
        for (i, a) in self
            .apps
            .iter()
            .enumerate()
            .filter(|(_, a)| !self.cfg.disabled_apps.contains(&a.name))
        {
            if let Some(score) = if q.is_empty() {
                Some(0)
            } else {
                fuzzy(q, &a.name)
            } {
                scored.push((
                    score,
                    Item {
                        title: a.name.clone(),
                        sub: a.comment.clone(),
                        tag: "Application",
                        icon: Some(a.icon.clone()),
                        action: Action::App(i),
                        best: false,
                    },
                ));
            }
        }
        for (title, sub, view) in [
            ("Search Files", "System files", "/"),
            ("Emoji Picker", "Search emoji by name", ":"),
            ("Clipboard History", "Saved text and images", ";"),
            (
                "Settings",
                "Appearance, clipboard and excluded applications",
                "settings",
            ),
        ] {
            if let Some(score) = if q.is_empty() {
                Some(0)
            } else {
                fuzzy(q, title)
            } {
                scored.push((
                    score,
                    Item {
                        title: title.into(),
                        sub: sub.into(),
                        tag: "Basic",
                        icon: None,
                        action: Action::View(view.into()),
                        best: false,
                    },
                ));
            }
        }
        scored.sort_by_key(|a| std::cmp::Reverse(a.0));
        group_sections(scored)
    }
}
/// Applications first, then commands, each in score order. With a query, the best scoring
/// row moves to a "Best Match" section of its own unless it already leads the list.
fn group_sections(scored: Vec<(i32, Item)>) -> Vec<Item> {
    let best = scored.first().map_or(0, |r| r.0);
    let (mut apps, mut cmds): (Vec<_>, Vec<_>) =
        scored.into_iter().partition(|r| r.1.tag == "Application");
    apps.append(&mut cmds);
    let mut rows: Vec<Item> = apps.iter().map(|r| r.1.clone()).collect();
    let promote = best > 0
        && apps
            .iter()
            .position(|r| r.0 == best)
            .is_some_and(|p| p > 0 && rows[p].tag != rows[0].tag);
    if promote {
        let p = apps.iter().position(|r| r.0 == best).unwrap_or(0);
        let mut it = rows.remove(p);
        it.best = true;
        rows.insert(0, it);
    }
    rows
}

/// When on, the file index includes dotfiles.
pub static SHOW_HIDDEN: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
/// Which kind of file the `/` view lists: 0 all, 1 by name, 2 by size, 3 documents, 4 images,
/// 5 audio, 6 video, 7 archives, 8 folders.
pub static FILTER: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// A short description of a path for the details pane: the kind of file, or "Folder".
pub fn file_kind(f: &str) -> String {
    if std::path::Path::new(f).is_dir() {
        return "Folder".into();
    }
    let ext = f
        .rsplit_once('.')
        .map(|x| x.1.to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "rs" => "Rust source".into(),
        "py" => "Python source".into(),
        "js" | "ts" => "JavaScript source".into(),
        "c" | "h" | "cpp" | "hpp" => "C/C++ source".into(),
        "md" => "Markdown document".into(),
        "toml" => "TOML document".into(),
        "json" => "JSON document".into(),
        "txt" | "log" => "Plain text".into(),
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" => "Image".into(),
        "mp3" | "flac" | "ogg" | "wav" | "m4a" => "Audio".into(),
        "mp4" | "mkv" | "webm" | "avi" | "mov" => "Video".into(),
        "zip" | "tar" | "gz" | "xz" | "bz2" | "7z" => "Archive".into(),
        "pdf" => "PDF document".into(),
        "doc" | "docx" | "odt" | "rtf" => "Word document".into(),
        "xls" | "xlsx" | "ods" | "csv" => "Spreadsheet document".into(),
        "ppt" | "pptx" | "odp" => "Presentation document".into(),
        "" => "File".into(),
        e => format!("{} file", e.to_uppercase()),
    }
}

pub fn file_icon(f: &str) -> String {
    if std::path::Path::new(f).is_dir() {
        return "folder".into();
    }
    let ext = f
        .rsplit_once('.')
        .map(|x| x.1.to_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "svg" | "webp" | "bmp" => "image-x-generic",
        "mp3" | "flac" | "ogg" | "wav" | "m4a" => "audio-x-generic",
        "mp4" | "mkv" | "webm" | "avi" | "mov" => "video-x-generic",
        "zip" | "tar" | "gz" | "xz" | "bz2" | "7z" => "package-x-generic",
        "pdf" | "doc" | "docx" | "odt" => "x-office-document",
        "xls" | "xlsx" | "ods" | "csv" => "x-office-spreadsheet",
        "ppt" | "pptx" | "odp" => "x-office-presentation",
        _ => "text-x-generic",
    }
    .into()
}

fn file_matches(path: &str, filter: usize) -> bool {
    let kind = file_kind(path);
    match filter {
        3 => kind.ends_with("document") || kind.ends_with("source") || kind == "Plain text",
        4 => kind == "Image",
        5 => kind == "Audio",
        6 => kind == "Video",
        7 => kind == "Archive",
        8 => kind == "Folder",
        _ => true,
    }
}

fn file_items(files: &[String], q: &str, filter: usize) -> Vec<Item> {
    let filtered: Vec<String>;
    let files = if filter >= 3 {
        filtered = files
            .iter()
            .filter(|f| file_matches(f, filter))
            .cloned()
            .collect();
        filtered.as_slice()
    } else {
        files
    };
    if files.is_empty() {
        return vec![Item {
            title: "No files found".into(),
            sub: String::new(),
            tag: "File",
            icon: None,
            action: Action::Nothing,
            best: false,
        }];
    }
    let home = std::env::var("HOME").unwrap_or_default();
    // A query ending in a slash walks into that directory and lists its direct children.
    if q.ends_with('/') {
        let base = if q.starts_with('/') {
            let path = q.trim_end_matches('/');
            if path.is_empty() {
                "/".into()
            } else {
                path.into()
            }
        } else {
            format!("{}/{}", home, q.trim_matches('/'))
        };
        let mut v: Vec<&String> = files
            .iter()
            .filter(|f| f.rsplit_once('/').is_some_and(|(d, _)| d == base))
            .collect();
        // Folders first; the filter box picks the order inside each group.
        match filter {
            1 => v.sort_by_key(|f| (!std::path::Path::new(f.as_str()).is_dir(), f.to_lowercase())),
            2 => v.sort_by_key(|f| {
                (
                    !std::path::Path::new(f.as_str()).is_dir(),
                    std::cmp::Reverse(std::fs::metadata(f.as_str()).map_or(0, |m| m.len())),
                )
            }),
            _ => v.sort_by_key(|f| {
                (
                    !std::path::Path::new(f.as_str()).is_dir(),
                    std::cmp::Reverse(
                        std::fs::metadata(f.as_str())
                            .and_then(|m| m.modified())
                            .ok(),
                    ),
                )
            }),
        }
        let mut out: Vec<Item> = Vec::new();
        if base != home && base != "/" {
            out.push(Item {
                title: "..".into(),
                sub: "Parent folder".into(),
                tag: "File",
                icon: Some("folder".into()),
                action: Action::Open(
                    base.rsplit_once('/')
                        .map_or(home.clone(), |x| x.0.to_string()),
                ),
                best: false,
            });
        }
        if v.is_empty() {
            out.push(Item {
                title: "Empty folder".into(),
                sub: base,
                tag: "File",
                icon: Some("folder".into()),
                action: Action::Nothing,
                best: false,
            });
        }
        out.extend(v.into_iter().take(60).map(|f| {
            let name = f.rsplit('/').next().unwrap_or(f).to_string();
            Item {
                title: name,
                sub: String::new(),
                tag: "File",
                icon: Some(file_icon(f)),
                action: Action::Open(f.clone()),
                best: false,
            }
        }));
        return out;
    }
    let mk = |f: &str| {
        let name = f.rsplit('/').next().unwrap_or(f).to_string();
        let dir = f.rsplit_once('/').map_or("", |x| x.0);
        let sub = if !home.is_empty() && dir.starts_with(&home) {
            format!("~{}", &dir[home.len()..])
        } else {
            dir.to_string()
        };
        Item {
            title: name,
            sub,
            tag: "File",
            icon: Some(file_icon(f)),
            action: Action::Open(f.to_string()),
            best: false,
        }
    };
    let mut v: Vec<(i32, &String)> = files
        .iter()
        .filter_map(|f| {
            if q.is_empty() {
                return Some((0, f));
            }
            let name = f.rsplit('/').next().unwrap_or(f);
            fuzzy(q, name)
                .map(|s| s * 2)
                .or_else(|| if q.contains('/') { fuzzy(q, f) } else { None })
                .map(|s| (s - f.matches('/').count() as i32, f))
        })
        .collect();
    if !q.is_empty() && !matches!(filter, 1 | 2) {
        v.sort_by_key(|x| std::cmp::Reverse(x.0));
    } else {
        match filter {
            // Name, ignoring the folder.
            1 => {
                let mut t: Vec<&String> = v.into_iter().map(|(_, f)| f).collect();
                t.sort_by_key(|f| f.rsplit('/').next().unwrap_or(f).to_lowercase());
                v = t.into_iter().take(60).map(|f| (0, f)).collect();
            }
            // Largest first.
            2 => {
                let mut t: Vec<(u64, &String)> = v
                    .iter()
                    .filter_map(|(_, f)| std::fs::metadata(f.as_str()).ok().map(|m| (m.len(), *f)))
                    .collect();
                t.sort_by_key(|x| std::cmp::Reverse(x.0));
                v = t.into_iter().take(60).map(|(_, f)| (0, f)).collect();
            }
            // Most recently changed first.
            _ => {
                let mut t: Vec<(std::time::SystemTime, &String)> = v
                    .iter()
                    .filter_map(|(_, f)| {
                        std::fs::metadata(f.as_str())
                            .ok()
                            .and_then(|m| m.modified().ok())
                            .map(|m| (m, *f))
                    })
                    .collect();
                t.sort_by_key(|x| std::cmp::Reverse(x.0));
                v = t.into_iter().take(60).map(|(_, f)| (0, f)).collect();
            }
        }
    }
    let mut out: Vec<Item> = Vec::new();
    out.extend(v.into_iter().take(60).map(|(_, f)| mk(f)));
    out
}

#[derive(Default)]
pub struct FileIndex(Option<Vec<String>>);

impl FileIndex {
    pub fn load(&mut self, roots: &[String], excludes: &[String]) {
        self.0.get_or_insert_with(|| scan_files(roots, excludes));
    }
    pub fn clear(&mut self) {
        self.0 = None;
    }
    pub fn files(&self) -> &[String] {
        self.0.as_deref().unwrap_or(&[])
    }
}

/// Files under the roots, a few levels deep, skipping hidden folders and build output.
pub fn scan_files(roots: &[String], excludes: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let roots: Vec<String> = if roots.is_empty() {
        vec![std::env::var("HOME").unwrap_or_default()]
    } else {
        roots.to_vec()
    };
    for r in roots {
        walk(std::path::Path::new(&r), 0, &mut out, excludes);
    }
    out
}

fn walk(dir: &std::path::Path, depth: usize, out: &mut Vec<String>, excludes: &[String]) {
    if depth > 5
        || out.len() >= 40000
        || excludes.iter().any(|p| !p.is_empty() && dir.starts_with(p))
    {
        return;
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        if out.len() >= 40000 {
            break;
        }
        if excludes
            .iter()
            .any(|p| !p.is_empty() && e.path().starts_with(p))
        {
            continue;
        }
        let name = e.file_name();
        let name = name.to_string_lossy();
        if (!SHOW_HIDDEN.load(std::sync::atomic::Ordering::Relaxed) && name.starts_with('.'))
            || matches!(&*name, "node_modules" | "target" | "__pycache__")
        {
            continue;
        }
        let Ok(ft) = e.file_type() else { continue };
        if ft.is_dir() {
            walk(&e.path(), depth + 1, out, excludes);
        }
        if (ft.is_file() || ft.is_dir()) && out.len() < 40000 {
            out.push(e.path().to_string_lossy().into_owned());
        }
    }
}

pub fn parse_desktop(txt: &str) -> Option<App> {
    let (mut name, mut comment, mut exec, mut icon) =
        (String::new(), String::new(), String::new(), String::new());
    let (mut hidden, mut app, mut term, mut in_main) = (false, false, false, false);
    for l in txt.lines() {
        if l.starts_with('[') {
            in_main = l == "[Desktop Entry]";
            continue;
        }
        if !in_main {
            continue;
        }
        if let Some(v) = l.strip_prefix("Name=") {
            name = v.into();
        } else if let Some(v) = l.strip_prefix("Comment=") {
            comment = v.into();
        } else if let Some(v) = l.strip_prefix("Exec=") {
            exec = v.into();
        } else if let Some(v) = l.strip_prefix("Icon=") {
            icon = v.into();
        } else if l == "NoDisplay=true" || l == "Hidden=true" {
            hidden = true;
        } else if l == "Terminal=true" {
            term = true;
        } else if l == "Type=Application" {
            app = true;
        }
    }
    if app && !hidden && !name.is_empty() && !exec.is_empty() {
        Some(App {
            name,
            comment,
            exec,
            icon,
            terminal: term,
        })
    } else {
        None
    }
}

/// Exec line with the %f %u style field codes removed.
pub fn exec_line(exec: &str) -> String {
    exec.split_whitespace()
        .filter(|a| !(a.starts_with('%') && a.len() == 2))
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn fuzzy(q: &str, s: &str) -> Option<i32> {
    if q.is_ascii() && s.is_ascii() {
        let s = s.as_bytes();
        let mut pos = 0;
        let mut score = 0;
        let mut last = None;
        for c in q.bytes().filter(|c| !(*c as char).is_whitespace()) {
            let i = s[pos..].iter().position(|x| x.eq_ignore_ascii_case(&c))? + pos;
            let word_start = i == 0 || !s[i - 1].is_ascii_alphanumeric();
            score +=
                1 + if last.is_some_and(|l| i == l + 1) {
                    3
                } else {
                    0
                } + if word_start { 4 } else { 0 }
                    + if i == 0 { 4 } else { 0 };
            last = Some(i);
            pos = i + 1;
        }
        return Some(score - (s.len() as i32 / 8));
    }
    let s: Vec<char> = s.to_lowercase().chars().collect();
    let mut pos = 0usize;
    let mut score = 0i32;
    let mut last: Option<usize> = None;
    for c in q.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let i = s[pos..].iter().position(|&x| x == c)? + pos;
        let word_start = i == 0 || !s[i - 1].is_alphanumeric();
        score +=
            1 + if last.is_some_and(|l| i == l + 1) {
                3
            } else {
                0
            } + if word_start { 4 } else { 0 }
                + if i == 0 { 4 } else { 0 };
        last = Some(i);
        pos = i + 1;
    }
    Some(score - (s.len() as i32 / 8))
}

pub fn emoji_items(q: &str) -> Vec<Item> {
    let mut v: Vec<(i32, &'static emojis::Emoji)> = Vec::new();
    for e in emojis::iter().filter(|e| e.skin_tone().is_none_or(|t| t == emojis::SkinTone::Default))
    {
        if q.is_empty() {
            v.push((0, e));
            continue;
        }
        let best = std::iter::once(e.name())
            .chain(e.shortcodes())
            .filter_map(|n| fuzzy(q, n))
            .max();
        if let Some(s) = best {
            v.push((s, e));
        }
    }
    if !q.is_empty() {
        v.sort_by_key(|a| std::cmp::Reverse(a.0));
    }
    v.into_iter()
        .map(|(_, e)| Item {
            title: e.name().to_string(),
            sub: format!(":{}:", e.shortcode().unwrap_or("")),
            tag: "Emoji",
            icon: Some(format!("emoji:{}", e.as_str())),
            action: Action::Clip(e.as_str().to_string()),
            best: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn emoji_picker_is_not_limited_to_eighty_results() {
        let all = emoji_items("");
        let expected = emojis::iter()
            .filter(|e| e.skin_tone().is_none_or(|t| t == emojis::SkinTone::Default))
            .count();
        assert_eq!(all.len(), expected);
        assert!(all.len() > 80);
        let matches = emoji_items("face");
        assert!(matches.len() > 80);
        for item in &all {
            let Action::Clip(sequence) = &item.action else {
                panic!("not a clipboard action")
            };
            assert_eq!(
                item.icon.as_deref(),
                Some(format!("emoji:{sequence}").as_str())
            );
        }
        assert!(all.iter().any(|it| it
            .icon
            .as_deref()
            .is_some_and(|icon| icon.contains('\u{200d}'))));
    }

    fn app(name: &str) -> App {
        App {
            name: name.into(),
            comment: "Test app".into(),
            exec: "true %U".into(),
            icon: String::new(),
            terminal: false,
        }
    }
    fn reference(q: &str, s: &str) -> Option<i32> {
        let s: Vec<char> = s.to_lowercase().chars().collect();
        let mut pos = 0usize;
        let mut score = 0i32;
        let mut last: Option<usize> = None;
        for c in q.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
            let i = s[pos..].iter().position(|&x| x == c)? + pos;
            let word_start = i == 0 || !s[i - 1].is_alphanumeric();
            score +=
                1 + if last.is_some_and(|l| i == l + 1) {
                    3
                } else {
                    0
                } + if word_start { 4 } else { 0 }
                    + if i == 0 { 4 } else { 0 };
            last = Some(i);
            pos = i + 1;
        }
        Some(score - (s.len() as i32 / 8))
    }

    #[test]
    fn fuzzy_matches_reference_for_ascii_and_unicode() {
        let candidates = [
            "",
            "Browser",
            "Code-Editor 42",
            "a_aA.z",
            "foo bar",
            "Straße",
            "İstanbul",
            "Ελληνικά",
            "日本語",
            "éclair",
        ];
        let queries = [
            "", "br", "BR", "b r", "bb", "Code42", "z", "  ", "\t\n", "\u{b}", "\u{c}", "\u{1c}",
            "é", "İ", "日", "ss", "Σ",
        ];
        for q in queries {
            for s in candidates {
                assert_eq!(fuzzy(q, s), reference(q, s), "q={q:?}, s={s:?}");
            }
        }
        for byte in 0..128u8 {
            let q = (byte as char).to_string();
            for s in candidates {
                assert_eq!(fuzzy(&q, s), reference(&q, s), "q={q:?}, s={s:?}");
            }
        }
    }
    #[test]
    fn root_has_only_apps_and_four_basic_views() {
        let cfg = Config::default();
        let apps = vec![app("Browser")];
        let history = clip::History::new(10);
        let search = Search {
            cfg: &cfg,
            apps: &apps,
            files: &[],
            history: &history,
        };
        let rows = search.run("");
        assert_eq!(
            rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
            vec![
                "Browser",
                "Search Files",
                "Emoji Picker",
                "Clipboard History",
                "Settings"
            ]
        );
        assert_eq!(rows[0].action, Action::App(0));
        for query in [
            "=1+1",
            "notes",
            "processes",
            "quicklinks",
            "currency",
            "windows",
            "scripts",
        ] {
            assert!(search.run(query).is_empty(), "{query}");
        }
    }
    #[test]
    fn excluded_apps_stay_in_the_exclusion_editor() {
        let cfg = Config {
            disabled_apps: vec!["Browser".into()],
            ..Config::default()
        };
        let apps = vec![app("Browser"), app("Editor")];
        let history = clip::History::new(10);
        let search = Search {
            cfg: &cfg,
            apps: &apps,
            files: &[],
            history: &history,
        };
        assert!(search.run("Browser").is_empty());
        let rows = search.run("settings apps Browser");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, Action::Exclude("Browser".into()));
        assert_eq!(rows[0].tag, "Excluded");
        assert_eq!(search.run("Editor")[0].action, Action::App(1));
    }
    #[test]
    fn picker_copies_full_emoji_and_history_filters_text() {
        let cfg = Config::default();
        let mut history = clip::History::new(10);
        history.push("unique clipboard text");
        let search = Search {
            cfg: &cfg,
            apps: &[],
            files: &[],
            history: &history,
        };
        let emoji = search.run(":grinning");
        assert!(emoji.iter().any(|r| r.action == Action::Clip("😀".into())));
        assert_eq!(
            search.run(";unique")[0].action,
            Action::Clip("unique clipboard text".into())
        );
        assert!(matches!(
            search.run(";").last().unwrap().action,
            Action::ClearClip
        ));
        assert!(search.run(";missing").is_empty());
        assert!(matches!(search.run("settings")[0].action, Action::View(_)));
    }
    #[test]
    fn search_orders_matches_by_score_and_keeps_app_indices() {
        let cfg = Config::default();
        let apps = vec![app("Draw Board"), app("Broad"), app("Browser")];
        let history = clip::History::new(0);
        let search = Search {
            cfg: &cfg,
            apps: &apps,
            files: &[],
            history: &history,
        };
        assert_eq!(
            search
                .run("br")
                .iter()
                .map(|r| r.action.clone())
                .collect::<Vec<_>>(),
            vec![
                Action::App(1),
                Action::App(2),
                Action::App(0),
                Action::View(";".into())
            ]
        );
        assert_eq!(search.run("  BROWSER  ")[0].action, Action::App(2));
    }
    #[test]
    fn equal_scores_preserve_application_order() {
        let cfg = Config::default();
        let apps = vec![app("Code B"), app("Code A")];
        let history = clip::History::new(0);
        let search = Search {
            cfg: &cfg,
            apps: &apps,
            files: &[],
            history: &history,
        };
        assert_eq!(
            search
                .run("code")
                .iter()
                .map(|r| r.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Code B", "Code A"]
        );
    }
    #[test]
    fn fuzzy_handles_unicode_whitespace_and_repeated_letters() {
        assert_eq!(fuzzy("éD", "Éditor"), Some(13));
        assert_eq!(fuzzy(" b\tr\n", "Browser"), fuzzy("br", "Browser"));
        assert_eq!(fuzzy("oo", "Book"), Some(5));
        assert_eq!(fuzzy("ooo", "Book"), None);
        assert_eq!(fuzzy("", ""), Some(0));
        assert_eq!(fuzzy("x", ""), None);
        assert_eq!(fuzzy("rb", "Browser"), None);
        assert!(fuzzy("br", "Browser").unwrap() > fuzzy("br", "Draw Board").unwrap());
    }
    #[test]
    fn exclusions_match_full_names_and_preserve_launch_indices() {
        let cfg = Config {
            disabled_apps: vec!["Code".into(), "Code".into(), "Missing".into()],
            ..Config::default()
        };
        let apps = vec![app("Code"), app("Code - Insiders"), app("code")];
        let history = clip::History::new(0);
        let search = Search {
            cfg: &cfg,
            apps: &apps,
            files: &[],
            history: &history,
        };
        let actions = search
            .run("code")
            .into_iter()
            .map(|r| r.action)
            .collect::<Vec<_>>();
        assert_eq!(actions, vec![Action::App(2), Action::App(1)]);
        let rows = search.run("settings apps code");
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].sub, "Hidden from search - Enter to show");
        assert_eq!(rows[1].sub, "Shown in search - Enter to hide");
        assert_eq!(rows[0].action, Action::Exclude("Code".into()));
    }
    #[test]
    fn clipboard_filter_keeps_full_multiline_text_and_omits_clear() {
        let cfg = Config::default();
        let mut history = clip::History::new(3);
        assert!(history.push("first line\nneedle\tλ"));
        let search = Search {
            cfg: &cfg,
            apps: &[],
            files: &[],
            history: &history,
        };
        let rows = search.run("; needle");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "first line");
        assert_eq!(rows[0].action, Action::Clip("first line\nneedle\tλ".into()));
        let rows = search.run(";");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].action, Action::ClearClip);
    }
    #[test]
    fn desktop_entry_uses_only_main_application_section() {
        let entry = "[Desktop Entry]\nType=Application\nName=Editor\nExec=editor %U\n[Desktop Action Other]\nName=Other\nExec=wrong";
        let app = parse_desktop(entry).unwrap();
        assert_eq!(app.name, "Editor");
        assert_eq!(exec_line(&app.exec), "editor");
        assert!(parse_desktop(&format!("{entry}\n[Desktop Entry]\nHidden=true")).is_none());
        assert!(parse_desktop("[Desktop Entry]\nType=Link\nName=Link\nExec=wrong").is_none());
        assert_eq!(fuzzy("br", "Browser"), Some(13));
        assert_eq!(fuzzy("zzz", "Browser"), None);
    }
}

#[cfg(test)]
mod scan_tests {
    use super::*;
    #[test]
    fn user_desktop_entries_override_system_entries_even_when_hidden() {
        let _lock = crate::testutil::ENV_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let user = root.path().join("user");
        let system = root.path().join("system");
        std::fs::create_dir_all(user.join("applications")).unwrap();
        std::fs::create_dir_all(system.join("applications/nested")).unwrap();
        let entry = "[Desktop Entry]\nType=Application\nName=System Browser\nExec=true\n";
        std::fs::write(system.join("applications/browser.desktop"), entry).unwrap();
        std::fs::write(
            user.join("applications/browser.desktop"),
            format!("{entry}Hidden=true\n"),
        )
        .unwrap();
        std::fs::write(
            system.join("applications/nested/editor.desktop"),
            entry.replace("System Browser", "Nested Editor"),
        )
        .unwrap();
        let old_home = std::env::var_os("XDG_DATA_HOME");
        let old_dirs = std::env::var_os("XDG_DATA_DIRS");
        std::env::set_var("XDG_DATA_HOME", &user);
        std::env::set_var("XDG_DATA_DIRS", &system);
        let apps = scan_apps(&Config::default());
        assert_eq!(
            apps.iter().map(|app| app.name.as_str()).collect::<Vec<_>>(),
            vec!["Nested Editor"]
        );
        for (key, value) in [("XDG_DATA_HOME", old_home), ("XDG_DATA_DIRS", old_dirs)] {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
    }
}

/// History key of a result.
pub fn key_of(it: &Item) -> String {
    if let Action::ClipImg(id) = &it.action {
        return format!("clipimg:{id}");
    }
    format!("{}:{}", it.tag, it.title)
}

/// What Ctrl+B offers for a result.
pub fn actions_for(it: &Item, pinned: bool, _cfg: &Config) -> Vec<Item> {
    let mk = |title: &str, sub: &str, action: Action| Item {
        title: title.into(),
        sub: sub.into(),
        tag: "Action",
        icon: None,
        action,
        best: false,
    };
    let verb = match it.tag {
        "Clipboard" | "Emoji" => "Copy",
        _ => "Open",
    };
    let mut v = vec![mk(verb, &it.title, it.action.clone())];
    if let (Action::Clip(t), "Emoji") = (&it.action, it.tag) {
        v.push(mk("Copy name", &it.title, Action::Clip(it.title.clone())));
        let cps = t
            .chars()
            .map(|c| format!("U+{:04X}", c as u32))
            .collect::<Vec<_>>()
            .join(" ");
        v.push(mk(
            "Copy unicode codepoint",
            &cps.clone(),
            Action::Clip(cps),
        ));
    }
    if matches!(it.tag, "Application" | "Basic" | "Emoji" | "Clipboard") {
        v.push(mk(
            if pinned { "Unpin" } else { "Pin" },
            "Shown first in search",
            Action::PinToggle(key_of(it)),
        ));
        v.push(mk(
            "Reset ranking",
            "Forget recents and usage counts",
            Action::ResetRank(key_of(it)),
        ));
    }
    v
}

#[cfg(test)]
mod action_tests {
    use super::*;
    #[test]
    fn emoji_actions_keep_the_payload_and_unicode() {
        let it = Item {
            title: "waving hand".into(),
            sub: String::new(),
            tag: "Emoji",
            icon: None,
            action: Action::Clip("👋".into()),
            best: false,
        };
        let actions = actions_for(&it, false, &Config::default());
        assert_eq!(actions[0].action, Action::Clip("👋".into()));
        assert_eq!(actions[1].action, Action::Clip("waving hand".into()));
        assert_eq!(actions[2].action, Action::Clip("U+1F44B".into()));
        assert_eq!(
            actions[3].action,
            Action::PinToggle("Emoji:waving hand".into())
        );
        assert_eq!(actions_for(&it, true, &Config::default())[3].title, "Unpin");
        assert_eq!(
            actions[4].action,
            Action::ResetRank("Emoji:waving hand".into())
        );
    }
}

#[cfg(test)]
mod file_tests {
    use super::*;
    #[test]
    fn empty_file_index_is_cached_until_clear() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zl-empty-index-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let roots = vec![dir.to_string_lossy().into_owned()];
        let mut index = FileIndex::default();
        index.load(&roots, &[]);
        assert!(index.files().is_empty());
        let file = dir.join("new.txt");
        std::fs::write(&file, "new").unwrap();
        index.load(&roots, &[]);
        assert!(index.files().is_empty());
        index.clear();
        index.load(&roots, &[]);
        assert_eq!(index.files(), &[file.to_string_lossy().to_string()]);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn file_exclusions_and_entry_limit() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zl-exclusions-{}", std::process::id()));
        let excluded = dir.join("private");
        let sibling = dir.join("private-notes");
        std::fs::create_dir_all(&excluded).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(excluded.join("secret.txt"), "secret").unwrap();
        std::fs::write(sibling.join("public.txt"), "public").unwrap();
        let omit = dir.join("omit.txt");
        std::fs::write(&omit, "omit").unwrap();
        let roots = vec![dir.to_string_lossy().into_owned()];
        let excludes = vec![
            excluded.to_string_lossy().into_owned(),
            omit.to_string_lossy().into_owned(),
        ];
        let files = scan_files(&roots, &excludes);
        assert!(files
            .iter()
            .any(|p| p.ends_with("private-notes/public.txt")));
        assert!(
            !files
                .iter()
                .any(|p| std::path::Path::new(p).starts_with(&excluded)
                    || p == omit.to_str().unwrap())
        );
        assert!(scan_files(&roots, &roots).is_empty());
        let mut near_cap = vec![String::new(); 39999];
        walk(&dir, 0, &mut near_cap, &[]);
        assert_eq!(near_cap.len(), 40000);
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn file_categories_and_absolute_directory() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zl-categories-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let files: Vec<String> = ["note.txt", "pic.PNG", "song.mp3", "movie.mp4", "pack.zip"]
            .iter()
            .map(|n| {
                let p = dir.join(n);
                std::fs::write(&p, "x").unwrap();
                p.to_string_lossy().into_owned()
            })
            .collect();
        for (filter, name) in [
            (3, "note.txt"),
            (4, "pic.PNG"),
            (5, "song.mp3"),
            (6, "movie.mp4"),
            (7, "pack.zip"),
        ] {
            let rows = file_items(&files, "", filter);
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].title, name);
        }
        for name in ["letter.docx", "table.csv", "talk.odp", "paper.PDF"] {
            assert!(file_matches(name, 3), "{name}");
        }
        assert!(file_matches(dir.to_str().unwrap(), 8));
        assert!(!file_matches(&files[0], 8));
        let rows = file_items(&files, &format!("{}/", dir.display()), 1);
        assert_eq!(rows.len(), 6);
        assert_eq!(rows[0].title, "..");
        assert_eq!(rows[1].title, "movie.mp4");
        std::fs::remove_dir_all(&dir).unwrap();
    }
    #[test]
    fn file_search_ranks_name_over_depth() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let cfg = Config::default();
        let files = vec![
            "/h/a/b/c/notes.txt".to_string(),
            "/h/notes.md".to_string(),
            "/h/other".to_string(),
        ];
        let history = clip::History::new(10);
        let s = Search {
            cfg: &cfg,
            apps: &[],
            files: &files,
            history: &history,
        };
        let r = s.run("/notes");
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].title, "notes.md");
        assert_eq!(r[0].action, Action::Open("/h/notes.md".into()));
        assert_eq!(s.run("/o").len(), 3);
        assert_eq!(file_items(&[], "x", 0)[0].title, "No files found");
    }
}
