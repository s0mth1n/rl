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
    Exclude(String),
    ClearClip,
}
#[derive(Clone, Debug)]
pub struct Item {
    pub title: String,
    pub sub: String,
    pub tag: &'static str,
    pub icon: Option<String>,
    pub action: Action,
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
                        "Excluded - Enter to include"
                    } else {
                        "Included - Enter to exclude"
                    }
                    .into(),
                    tag: "Application",
                    icon: Some(a.icon.clone()),
                    action: Action::Exclude(a.name.clone()),
                })
                .collect();
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
                })
                .collect();
            if filter.trim().is_empty() {
                rows.extend(self.history.images.iter().map(|i| Item {
                    title: format!("Image {} x {}", i.w, i.h),
                    sub: "Copy PNG to clipboard".into(),
                    tag: "Clipboard",
                    icon: None,
                    action: Action::ClipImg(i.id),
                }));
                rows.push(Item {
                    title: "Clear clipboard history".into(),
                    sub: "Remove saved text and images".into(),
                    tag: "Clipboard",
                    icon: None,
                    action: Action::ClearClip,
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
                    },
                ));
            }
        }
        for (title, sub, view) in [
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
                    },
                ));
            }
        }
        scored.sort_by_key(|a| std::cmp::Reverse(a.0));
        scored.into_iter().map(|(_, row)| row).collect()
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
            if v.len() >= 80 {
                break;
            }
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
        v.truncate(80);
    }
    v.into_iter()
        .map(|(_, e)| Item {
            title: e.name().to_string(),
            sub: format!(":{}:", e.shortcode().unwrap_or("")),
            tag: "Emoji",
            icon: Some(format!("emoji:{}", e.as_str())),
            action: Action::Clip(e.as_str().to_string()),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app(name: &str) -> App {
        App {
            name: name.into(),
            comment: "Test app".into(),
            exec: "true %U".into(),
            icon: String::new(),
            terminal: false,
        }
    }
    #[test]
    fn root_has_only_apps_and_three_basic_views() {
        let cfg = Config::default();
        let apps = vec![app("Browser")];
        let history = clip::History::new(10);
        let search = Search {
            cfg: &cfg,
            apps: &apps,
            history: &history,
        };
        let rows = search.run("");
        assert_eq!(
            rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
            vec!["Browser", "Emoji Picker", "Clipboard History", "Settings"]
        );
        assert_eq!(rows[0].action, Action::App(0));
        for query in [
            "=1+1",
            "/tmp/",
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
            history: &history,
        };
        assert!(search.run("Browser").is_empty());
        let rows = search.run("settings apps Browser");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].action, Action::Exclude("Browser".into()));
        assert!(rows[0].sub.starts_with("Excluded"));
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
