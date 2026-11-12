//! Applications, emoji and clipboard rows for the basic launcher.
use crate::{clip, config, config::Config};

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
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
    WebSearch(String),
    System(crate::system_actions::SystemAction),
    Calculate(crate::calculator::CalculatorItem),
    Window(String),
    /// Clipboard text entry: the text and its history timestamp, so delete
    /// removes exactly this copy (emoji copies use timestamp 0).
    Clip(String, u64),
    ClipFiles(String, u64),
    ClipImg(u64, u64),
    View(String),
    Theme(String),
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

pub fn scan_apps(_cfg: &Config) -> Vec<App> {
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
                .file_stem()
                .map(|s| s.to_os_string())
                .unwrap_or_default();
            if desktop_ids.contains(&id) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            if let Some(app) = parse_desktop_path(&path, &text) {
                desktop_ids.insert(id);
                out.push(app);
            }
        }
    }
    out.sort_by_cached_key(|a| a.name.to_lowercase());
    out
}

pub struct Search<'a> {
    pub cfg: &'a Config,
    pub apps: &'a [App],
    pub windows: &'a [crate::windows::Window],
    pub history: &'a clip::History,
}
impl Search<'_> {
    pub fn run(&self, query: &str) -> Vec<Item> {
        self.run_mode(query, true)
    }

    pub fn run_mode(&self, query: &str, route_prefixes: bool) -> Vec<Item> {
        if let Some(filter) = query.strip_prefix('#').filter(|_| route_prefixes) {
            return theme_items(self.cfg, filter);
        }
        let q = query.trim();
        if let Some(filter) = q.strip_prefix(':').filter(|_| route_prefixes) {
            return emoji_items(filter.trim());
        }
        if let Some(filter) = q.strip_prefix(';').filter(|_| route_prefixes) {
            let needle = filter.trim().to_lowercase();
            let now = clip::now();
            let mut dated: Vec<_> = self
                .history
                .entries
                .iter()
                .filter(|c| clipboard_matches(&c.text, &needle))
                .map(|c| {
                    let paths: Vec<String> = if c.files {
                        c.text.lines().map(str::to_string).collect()
                    } else {
                        Vec::new()
                    };
                    (
                        c.at,
                        Item {
                            title: if c.files {
                                clip::file_title(&paths)
                            } else {
                                clip::preview(&c.text)
                            },
                            sub: clip::timestamp(c.at, now),
                            tag: "Clipboard",
                            icon: if c.files {
                                Some(
                                    if paths.len() == 1 {
                                        clip::clip_file_icon(&paths[0])
                                    } else {
                                        "phosphor:file"
                                    }
                                    .to_string(),
                                )
                            } else if clip::parse_color(&c.text).is_some() {
                                None
                            } else {
                                Some(
                                    clip::file_url_path(&c.text)
                                        .map(|p| clip::clip_file_icon(&p))
                                        .unwrap_or("phosphor:clipboard-text")
                                        .to_string(),
                                )
                            },
                            action: if c.files {
                                Action::ClipFiles(c.text.clone(), c.at)
                            } else {
                                Action::Clip(c.text.clone(), c.at)
                            },
                            best: false,
                        },
                    )
                })
                .collect();
            if "[image preview]".contains(&needle) || "[image]".contains(&needle) {
                dated.extend(self.history.images.iter().map(|i| {
                    (
                        i.at,
                        Item {
                            title: format!("Image {} x {}", i.w, i.h),
                            sub: clip::timestamp(i.at, now),
                            tag: "Clipboard",
                            icon: Some("phosphor:image".into()),
                            action: Action::ClipImg(i.id, i.at),
                            best: false,
                        },
                    )
                }));
            }
            dated.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
            let rows: Vec<_> = dated.into_iter().map(|(_, item)| item).collect();
            return rows;
        }
        let scorer = crate::scorer::Scorer::new(&self.cfg.fuzzy_match);
        let mut scored = Vec::new();
        for window in self.windows {
            let score = scorer.score(
                window.title(),
                &window.description(),
                q,
                crate::scorer::Kind::Regular,
            );
            if let Some(score) = score {
                scored.push((
                    score,
                    Item {
                        title: window.title().into(),
                        sub: window.description(),
                        tag: "Window",
                        // Compositor-provided pixels (KWin) win over the
                        // icon-theme lookup, like zlaunch's fetch_windows.
                        icon: Some(if window.icon_data.is_some() {
                            format!("window-data:{}", window.address)
                        } else {
                            format!("window:{}", window.app_id)
                        }),
                        action: Action::Window(window.address.clone()),
                        best: false,
                    },
                ));
            }
        }
        for (i, a) in self
            .apps
            .iter()
            .enumerate()
            .filter(|(_, a)| !self.cfg.disabled_apps.contains(&a.name))
        {
            if let Some(score) = scorer.score(&a.name, &a.comment, q, crate::scorer::Kind::Regular)
            {
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
        for (title, sub, view, icon) in [
            // Submenu icons come from zlaunch's embedded Phosphor assets.
            ("Emoji Picker", "Search emoji by name", ":", "smiley"),
            (
                "Clipboard History",
                "Saved text and images",
                ";",
                "clipboard",
            ),
            ("Themes", "Browse and apply themes", "#", "palette"),
        ] {
            let (title, sub, view, icon) = (title, sub, view, icon);
            if let Some(score) = scorer.score(title, sub, q, crate::scorer::Kind::Submenu) {
                scored.push((
                    score,
                    Item {
                        title: title.into(),
                        sub: sub.into(),
                        tag: "Basic",
                        icon: Some(format!("phosphor:{icon}")),
                        action: Action::View(view.into()),
                        best: false,
                    },
                ));
            }
        }
        for action in crate::system_actions::SystemAction::ALL {
            let (title, sub, icon) = action.details();
            if let Some(score) = scorer.score(title, sub, q, crate::scorer::Kind::Action) {
                scored.push((
                    score,
                    Item {
                        title: title.into(),
                        sub: sub.into(),
                        tag: "System",
                        icon: Some(icon.into()),
                        action: Action::System(action),
                        best: false,
                    },
                ));
            }
        }
        if self
            .cfg
            .combined_modules()
            .contains(&config::ConfigModule::Calculator)
            && q.chars().any(|c| c.is_numeric())
        {
            if let Ok(result) = crate::calculator::evaluate_expression(q) {
                scored.push((
                    0,
                    Item {
                        title: result.display_result.clone(),
                        sub: String::new(),
                        tag: "Calculator",
                        icon: None,
                        action: Action::Calculate(result),
                        best: false,
                    },
                ));
            }
        }
        let modules = self.cfg.combined_modules();
        let search_enabled = modules.contains(&config::ConfigModule::Search);
        let providers = self.cfg.search_providers.as_deref().unwrap_or_default();
        let detection = crate::web_search::detect(q, providers);
        if search_enabled {
            let searches: Vec<_> = match detection {
                crate::web_search::Detection::Triggered(provider, query) => vec![(provider, query)],
                crate::web_search::Detection::Fallback(query) => {
                    providers.iter().map(|provider| (provider, query)).collect()
                }
                crate::web_search::Detection::None => Vec::new(),
            };
            for (provider, query) in searches {
                scored.push((
                    0,
                    Item {
                        title: format!("Search on {}", provider.name),
                        sub: String::new(),
                        tag: "Search",
                        icon: Some(provider.icon_name()),
                        action: Action::WebSearch(provider.build_url(query)),
                        best: false,
                    },
                ));
            }
        }
        scored.sort_by_key(|a| std::cmp::Reverse(a.0));
        group_sections(scored, self.cfg)
    }
}
fn clipboard_matches(text: &str, needle: &str) -> bool {
    needle.is_empty()
        || text.to_lowercase().contains(needle)
        // The preview only adds dots. Unicode lowercase can depend on the truncated context.
        || ((!needle.is_ascii() || needle.contains('.'))
            && clip::preview(text).to_lowercase().contains(needle))
}

fn item_module(item: &Item) -> config::ConfigModule {
    use config::ConfigModule::*;
    match (&item.action, item.tag) {
        (_, "Application") => Applications,
        (_, "Calculator") => Calculator,
        (_, "Search") => Search,
        (_, "Window") => Windows,
        (Action::View(view), _) if view == ":" => Emojis,
        (Action::View(view), _) if view == ";" => Clipboard,
        (Action::View(view), _) if view == "#" => Themes,
        _ => Actions,
    }
}

fn module_section(module: config::ConfigModule) -> &'static str {
    use config::ConfigModule::*;
    match module {
        Applications => "Applications",
        Windows => "Windows",
        Calculator => "Calculator",
        Search => "Search and AI",
        _ => "Commands",
    }
}

/// Combined sections follow module order; a best match only moves out of a later section.
fn group_sections(mut scored: Vec<(i64, Item)>, cfg: &Config) -> Vec<Item> {
    let modules = cfg.combined_modules();
    scored.retain(|(_, item)| modules.contains(&item_module(item)));
    scored.sort_by_key(|(_, item)| {
        modules
            .iter()
            .position(|module| *module == item_module(item))
            .unwrap_or(usize::MAX)
    });
    let mut sections = Vec::new();
    for module in modules {
        let section = module_section(module);
        if !sections.contains(&section)
            && scored
                .iter()
                .any(|(_, item)| module_section(item_module(item)) == section)
        {
            sections.push(section);
        }
    }
    let best = if cfg.fuzzy_match.show_best_match {
        scored
            .iter()
            .enumerate()
            .filter(|(_, (_, item))| !matches!(item.tag, "Calculator" | "AI" | "Search"))
            .max_by_key(|(_, (score, _))| *score)
            .filter(|(_, (score, item))| {
                *score > 0
                    && sections
                        .first()
                        .is_some_and(|first| *first != module_section(item_module(item)))
            })
            .map(|(i, _)| i)
    } else {
        None
    };
    let promoted = best.map(|i| {
        let mut item = scored.remove(i).1;
        item.best = true;
        item
    });
    scored.sort_by_key(|(_, item)| {
        (
            sections
                .iter()
                .position(|section| *section == module_section(item_module(item)))
                .unwrap_or(usize::MAX),
            item.tag == "Search",
        )
    });
    promoted
        .into_iter()
        .chain(scored.into_iter().map(|(_, item)| item))
        .collect()
}

#[cfg(test)]
pub fn parse_desktop(txt: &str) -> Option<App> {
    parse_desktop_path(std::path::Path::new("fixture.desktop"), txt)
}

fn parse_desktop_path(path: &std::path::Path, txt: &str) -> Option<App> {
    let entry =
        freedesktop_desktop_entry::DesktopEntry::from_str(path, txt, None::<&[&str]>).ok()?;
    let locales: &[&str] = &[];
    let name = entry.name(locales)?.to_string();
    let exec = entry.exec()?.to_string();
    if entry.no_display() {
        return None;
    }
    Some(App {
        name,
        exec,
        comment: entry.comment(locales).unwrap_or_default().into(),
        icon: entry.icon().unwrap_or_default().into(),
        terminal: entry.terminal(),
    })
}

/// Desktop field codes removed by zlaunch before splitting the command into arguments.
pub fn exec_line(exec: &str) -> String {
    let mut result = exec.to_string();
    for code in [
        "%f", "%F", "%u", "%U", "%d", "%D", "%n", "%N", "%i", "%c", "%k",
    ] {
        result = result.replace(code, "");
    }
    result.trim().into()
}

#[allow(dead_code)]
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

/// The theme picker is a `#` query, optionally followed by a name filter.
pub fn theme_view(query: &str) -> bool {
    query.trim_start().starts_with('#')
}

/// Picker rows for bundled and user themes: the configured one first, then alphabetical,
/// like zlaunch's theme mode. `Action::Theme` previews the row's theme while selected.
pub fn theme_items(cfg: &Config, q: &str) -> Vec<Item> {
    let current = if config::load_theme(&cfg.theme).is_some() {
        cfg.theme.as_str()
    } else {
        "default"
    };
    let mut names = config::available_themes();
    names.retain(|(name, _)| config::load_theme(name).is_some());
    names.sort_by_key(|(name, _)| name != current);
    let q = q.to_lowercase();
    names
        .into_iter()
        .filter(|(name, source)| {
            name.to_lowercase().contains(&q) || source.to_lowercase().contains(&q)
        })
        .map(|(name, source)| Item {
            title: name.clone(),
            sub: source,
            tag: "Theme",
            icon: Some(format!("theme:{name}")),
            action: Action::Theme(name),
            best: false,
        })
        .collect()
}

pub fn emoji_items(q: &str) -> Vec<Item> {
    // zlaunch's emoji_delegate filter: case-insensitive name substring or a
    // literal emoji match, every skin-tone variant as its own item, original
    // crate order, no scoring, recents or tone filtering.
    let ql = q.to_lowercase();
    emojis::iter()
        .filter(|e| {
            ql.is_empty() || e.name().to_lowercase().contains(&ql) || e.as_str().contains(q)
        })
        .map(|e| Item {
            title: e.name().to_string(),
            sub: format!(":{}:", e.shortcode().unwrap_or("")),
            tag: "Emoji",
            icon: Some(format!("emoji:{}", e.as_str())),
            action: Action::Clip(e.as_str().to_string(), 0),
            best: false,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipboard_filter_keeps_preview_ellipsis_and_unicode_lowercase_matches() {
        let texts = [
            "",
            "short",
            "  line one\nline two",
            "a",
            "Clipboard fixture café https://example.org/4999",
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
            "ΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΑΣx",
            "İstanbul",
            "FOO Σ BAR",
        ];
        for text in texts {
            for needle in [
                "", "line", "missing", "4999", "é", "ς", "σ", "i", "i̇", ".", "...", "xyz...",
                "foo", "\n", " bar",
            ] {
                let expected = needle.is_empty()
                    || text.to_lowercase().contains(needle)
                    || clip::preview(text).to_lowercase().contains(needle);
                assert_eq!(
                    clipboard_matches(text, needle),
                    expected,
                    "{text:?} {needle:?}"
                );
            }
        }
    }

    #[test]
    fn emoji_picker_is_not_limited_to_eighty_results() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let all = emoji_items("");
        // zlaunch lists the emojis crate's iter() output as-is (base emoji
        // only: the crate's iterator excludes non-default skin tones, and
        // zlaunch does not expand them either).
        let expected = emojis::iter().count();
        // No skin-tone expansion, like zlaunch: one waving hand, not six.
        assert_eq!(emoji_items("waving hand").len(), 1);
        assert_eq!(all.len(), expected);
        assert!(all.len() > 80);
        let matches = emoji_items("face");
        assert!(matches.len() > 80);
        for item in &all {
            let Action::Clip(sequence, _) = &item.action else {
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
    fn calculator_and_actions_follow_module_gates_and_section_order() {
        let mut cfg = Config {
            search_providers: None,
            ..Config::default()
        };
        let history = clip::History::new(0);
        let run = |cfg: &Config, q: &str| {
            Search {
                cfg,
                apps: &[],
                windows: &[],
                history: &history,
            }
            .run(q)
        };
        let rows = run(&cfg, "1000 * 1000");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "1,000,000");
        assert_eq!(crate::ui::section_of(&rows[0]), Some("Calculator"));
        let Action::Calculate(result) = &rows[0].action else {
            panic!("not calculator")
        };
        assert_eq!(result.text_for_clipboard(), "1000000");
        let rows = run(&cfg, "1/0");
        let Action::Calculate(result) = &rows[0].action else {
            panic!("not calculator")
        };
        assert!(result.is_error);
        assert_eq!(result.text_for_clipboard(), "Infinity");
        assert!(run(&cfg, "2 +* 2").is_empty());
        assert!(run(&cfg, "sin(pi)").is_empty());
        assert_eq!(run(&cfg, "Shutdown")[0].sub, "Power off the system");
        cfg.disabled_modules = Some(vec![
            config::ConfigModule::Actions,
            config::ConfigModule::Calculator,
        ]);
        assert!(run(&cfg, "Shutdown").is_empty());
        assert!(run(&cfg, "2+2").is_empty());
        cfg.disabled_modules = None;
        cfg.combined_modules = Some(vec![config::ConfigModule::Actions]);
        assert_eq!(
            run(&cfg, "")
                .iter()
                .map(|r| r.title.as_str())
                .collect::<Vec<_>>(),
            ["Shutdown", "Reboot", "Suspend", "Lock Screen", "Log Out"]
        );
    }

    #[test]
    fn root_has_apps_and_basic_views() {
        let cfg = Config {
            search_providers: None,
            ..Config::default()
        };
        let apps = vec![app("Browser")];
        let history = clip::History::new(10);
        let search = Search {
            cfg: &cfg,
            apps: &apps,
            windows: &[],
            history: &history,
        };
        let rows = search.run("");
        assert_eq!(
            rows.iter().map(|r| r.title.as_str()).collect::<Vec<_>>(),
            vec![
                "Emoji Picker",
                "Clipboard History",
                "Shutdown",
                "Reboot",
                "Suspend",
                "Lock Screen",
                "Log Out",
                "Themes",
                "Browser"
            ]
        );
        assert_eq!(rows.last().unwrap().action, Action::App(0));
        for query in [
            "=1+1",
            "notes",
            "processes",
            "quicklinks",
            "currency",
            "windows",
        ] {
            assert!(search.run(query).is_empty(), "{query}");
        }
    }
    #[test]
    fn picker_copies_full_emoji_and_history_filters_text() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let cfg = Config::default();
        let mut history = clip::History::new(10);
        history.push("unique clipboard text");
        let search = Search {
            cfg: &cfg,
            apps: &[],
            windows: &[],
            history: &history,
        };
        let emoji = search.run(":grinning");
        assert!(emoji
            .iter()
            .any(|r| r.action == Action::Clip("😀".into(), 0)));
        assert!(matches!(
            &search.run(";unique")[0].action,
            Action::Clip(t, _) if t == "unique clipboard text"
        ));
        assert_eq!(search.run(";").len(), 1);
        assert!(search.run(";missing").is_empty());
    }
    #[test]
    fn search_orders_matches_by_score_and_keeps_app_indices() {
        let cfg = Config {
            search_providers: None,
            ..Config::default()
        };
        let apps = vec![app("Draw Board"), app("Broad"), app("Browser")];
        let history = clip::History::new(0);
        let search = Search {
            cfg: &cfg,
            apps: &apps,
            windows: &[],
            history: &history,
        };
        assert_eq!(
            search
                .run("br")
                .iter()
                .map(|r| r.action.clone())
                .collect::<Vec<_>>(),
            vec![
                Action::App(2),
                Action::View(";".into()),
                Action::View("#".into()),
                Action::App(1),
                Action::App(0)
            ]
        );
        assert!(search.run("  BROWSER  ").is_empty());
    }
    #[test]
    fn equal_scores_preserve_application_order() {
        let cfg = Config {
            search_providers: None,
            ..Config::default()
        };
        let apps = vec![app("Code B"), app("Code A")];
        let history = clip::History::new(0);
        let search = Search {
            cfg: &cfg,
            apps: &apps,
            windows: &[],
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
            search_providers: None,
            ..Config::default()
        };
        let apps = vec![app("Code"), app("Code - Insiders"), app("code")];
        let history = clip::History::new(0);
        let search = Search {
            cfg: &cfg,
            apps: &apps,
            windows: &[],
            history: &history,
        };
        let actions = search
            .run("code")
            .into_iter()
            .map(|r| r.action)
            .collect::<Vec<_>>();
        assert_eq!(actions, vec![Action::App(2), Action::App(1)]);
    }
    #[test]
    fn clipboard_filter_uses_case_insensitive_substrings_not_fuzzy_matches() {
        let cfg = Config::default();
        let mut history = clip::History::new(3);
        history.push("Alpha beta\nÉCOLE");
        history.push("abc");
        let search = Search {
            cfg: &cfg,
            apps: &[],
            windows: &[],
            history: &history,
        };
        assert!(search.run(";ac").is_empty());
        for q in [";BETA", ";école", ";beta\nÉCOLE"] {
            assert!(matches!(
                &search.run(q)[0].action,
                Action::Clip(t, _) if t == "Alpha beta\nÉCOLE"
            ));
        }
        assert!(search.run(";apbt").is_empty());
    }
    #[test]
    fn clipboard_filter_searches_image_labels_and_has_no_clear_row() {
        let cfg = Config::default();
        let mut history = clip::History::new(2);
        history.push("image notes");
        history.images.push(clip::Img {
            id: 42,
            png: Some(Vec::new()),
            path: std::path::PathBuf::new(),
            w: 1,
            h: 1,
            at: u64::MAX,
        });
        let search = Search {
            cfg: &cfg,
            apps: &[],
            windows: &[],
            history: &history,
        };
        assert_eq!(search.run(";").len(), 2);
        assert!(search
            .run(";")
            .iter()
            .all(|r| matches!(r.action, Action::Clip(..) | Action::ClipImg(..))));
        let rows = search.run(";PREVIEW");
        assert!(matches!(&rows[0].action, Action::ClipImg(42, _)));
        assert_eq!(rows.len(), 1);
        let rows = search.run(";image");
        assert_eq!(rows.len(), 2);
        assert!(matches!(&rows[0].action, Action::ClipImg(42, _)));
        assert!(matches!(&rows[1].action, Action::Clip(t, _) if t == "image notes"));
        assert!(search.run(";iae").is_empty());
        let empty = clip::History::new(0);
        let empty_search = Search {
            windows: &[],
            history: &empty,
            ..search
        };
        assert!(empty_search.run(";").is_empty());
    }
    #[test]
    fn clipboard_rows_interleave_types_by_timestamp_with_stable_ties() {
        let cfg = Config::default();
        let mut history = clip::History::new(4);
        history.entries = vec![
            clip::Clip {
                text: "new text".into(),
                at: 30,
                files: false,
            },
            clip::Clip {
                text: "tied text".into(),
                at: 20,
                files: false,
            },
            clip::Clip {
                text: "old text".into(),
                at: 10,
                files: false,
            },
        ];
        history.images = vec![
            clip::Img {
                id: 1,
                png: Some(Vec::new()),
                path: std::path::PathBuf::new(),
                w: 1,
                h: 1,
                at: 40,
            },
            clip::Img {
                id: 2,
                png: Some(Vec::new()),
                path: std::path::PathBuf::new(),
                w: 1,
                h: 1,
                at: 20,
            },
        ];
        let search = Search {
            cfg: &cfg,
            apps: &[],
            windows: &[],
            history: &history,
        };
        assert_eq!(
            search
                .run(";")
                .into_iter()
                .map(|r| r.action)
                .collect::<Vec<_>>(),
            vec![
                Action::ClipImg(1, 40),
                Action::Clip("new text".into(), 30),
                Action::Clip("tied text".into(), 20),
                Action::ClipImg(2, 20),
                Action::Clip("old text".into(), 10),
            ]
        );
        assert_eq!(
            search
                .run(";text")
                .into_iter()
                .map(|r| r.action)
                .collect::<Vec<_>>(),
            vec![
                Action::Clip("new text".into(), 30),
                Action::Clip("tied text".into(), 20),
                Action::Clip("old text".into(), 10),
            ]
        );
    }
    #[test]
    fn clipboard_filter_keeps_full_multiline_text_and_omits_clear() {
        let cfg = Config::default();
        let mut history = clip::History::new(3);
        assert!(history.push("first line\nneedle\tλ"));
        let search = Search {
            cfg: &cfg,
            apps: &[],
            windows: &[],
            history: &history,
        };
        let rows = search.run("; needle");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "first line");
        assert!(matches!(
            &rows[0].action,
            Action::Clip(t, _) if t == "first line\nneedle\tλ"
        ));
        let rows = search.run(";");
        assert_eq!(rows.len(), 1);
    }
    #[test]
    fn theme_picker_pins_the_current_theme_then_alphabetical() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let cfg = Config {
            theme: "nord".into(),
            ..Config::default()
        };
        let history = clip::History::new(10);
        let search = Search {
            cfg: &cfg,
            apps: &[],
            windows: &[],
            history: &history,
        };
        let missing = Config {
            theme: "no-such-theme".into(),
            ..Config::default()
        };
        assert_eq!(theme_items(&missing, "")[0].title, "default");
        let rows = search.run("#");
        assert_eq!(rows.len(), config::THEMES.len());
        assert_eq!(rows[0].title, "nord");
        assert_eq!(rows[0].action, Action::Theme("nord".into()));
        assert_eq!(rows[0].icon.as_deref(), Some("theme:nord"));
        assert_eq!(rows[0].sub, "bundled");
        let rest: Vec<_> = rows[1..].iter().map(|r| r.title.as_str()).collect();
        let mut sorted = rest.clone();
        sorted.sort_unstable();
        assert_eq!(rest, sorted);
        assert!(rest.contains(&"default"));
        // The filter narrows by name, keeping the pinned order.
        let rows = search.run("#catppuccin");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].title, "catppuccin-latte");
        assert!(search.run("#zzzz").is_empty());
        assert!(search.run("#nord ").is_empty());
        assert!(search.run("# nord").is_empty());
        assert_eq!(search.run("#NORD")[0].title, "nord");
        assert!(theme_view("#nord"));
        assert!(theme_view(" #"));
        assert!(!theme_view(""));
    }
    #[test]
    fn desktop_entry_uses_only_main_application_section() {
        let entry = "[Desktop Entry]\nType=Application\nName=Editor\nExec=editor %U\n[Desktop Action Other]\nName=Other\nExec=wrong";
        let app = parse_desktop(entry).unwrap();
        assert_eq!(app.name, "Editor");
        assert_eq!(exec_line(&app.exec), "editor");
        assert!(parse_desktop(&format!("{entry}\n[Desktop Entry]\nHidden=true")).is_some());
        assert!(parse_desktop("[Desktop Entry]\nType=Link\nName=Link\nExec=wrong").is_some());
        assert_eq!(fuzzy("br", "Browser"), Some(13));
        assert_eq!(fuzzy("zzz", "Browser"), None);
    }
}

#[cfg(test)]
mod scan_tests {
    use super::*;
    #[test]
    fn scanner_preserves_same_name_and_terminal_entries_and_collapses_basename_ids() {
        let _lock = crate::testutil::ENV_LOCK.lock().unwrap();
        let root = tempfile::tempdir().unwrap();
        let user = root.path().join("user");
        let system = root.path().join("system");
        std::fs::create_dir_all(user.join("applications")).unwrap();
        std::fs::create_dir_all(system.join("applications/nested")).unwrap();
        let entry = "[Desktop Entry]\nType=Application\nName=Same\nExec=true\n";
        std::fs::write(
            user.join("applications/first.desktop"),
            format!("{entry}Terminal=true\n"),
        )
        .unwrap();
        std::fs::write(system.join("applications/second.desktop"), entry).unwrap();
        std::fs::write(
            system.join("applications/nested/first.desktop"),
            entry.replace("Same", "Overridden"),
        )
        .unwrap();
        std::fs::write(
            user.join("applications/hidden.desktop"),
            format!("{entry}NoDisplay=true\n"),
        )
        .unwrap();
        std::fs::write(
            system.join("applications/hidden.desktop"),
            entry.replace("Same", "Visible fallback"),
        )
        .unwrap();
        let old_home = std::env::var_os("XDG_DATA_HOME");
        let old_dirs = std::env::var_os("XDG_DATA_DIRS");
        std::env::set_var("XDG_DATA_HOME", &user);
        std::env::set_var("XDG_DATA_DIRS", &system);
        let apps = scan_apps(&Config::default());
        for (key, value) in [("XDG_DATA_HOME", old_home), ("XDG_DATA_DIRS", old_dirs)] {
            match value {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        assert_eq!(
            apps.iter().map(|a| a.name.as_str()).collect::<Vec<_>>(),
            ["Same", "Same", "Visible fallback"]
        );
        assert_eq!(apps.iter().filter(|app| app.terminal).count(), 1);
    }

    #[test]
    fn reference_parser_keeps_hidden_entries_and_basename_ids_win() {
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
            vec!["Nested Editor", "System Browser"]
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
    if let Action::ClipImg(id, _) = &it.action {
        return format!("clipimg:{id}");
    }
    format!("{}:{}", it.tag, it.title)
}

#[cfg(test)]
mod strict_script_tests {
    use super::*;

    #[test]
    fn script_commands_are_not_available() {
        let cfg: Config =
            toml::from_str("search_providers = []\nscript_dirs = ['/tmp/not-scanned']").unwrap();
        let history = clip::History::new(0);
        let search = Search {
            cfg: &cfg,
            apps: &[],
            windows: &[],
            history: &history,
        };
        for query in [
            "scripts",
            "scripts test",
            "confirm script-run 0\tCommand\t",
            "confirm script-del /tmp/test",
        ] {
            assert!(search.run(query).is_empty(), "{query}");
        }
        assert!(search.run("").iter().all(|it| it.title != "Manage Scripts"));
        assert!(!toml::to_string(&cfg).unwrap().contains("script_dirs"));
    }
}

#[cfg(test)]
mod combined_section_tests {
    use super::*;
    fn row(title: &str, tag: &'static str, action: Action) -> Item {
        Item {
            title: title.into(),
            sub: String::new(),
            tag,
            action,
            icon: None,
            best: false,
        }
    }
    fn fixture() -> Vec<(i64, Item)> {
        vec![
            (100, row("App", "Application", Action::App(0))),
            (40, row("Window", "Window", Action::App(1))),
            (30, row("Emoji", "Basic", Action::View(":".into()))),
            (20, row("Clipboard", "Basic", Action::View(";".into()))),
        ]
    }
    #[test]
    fn default_order_promotes_only_from_a_later_section() {
        let rows = group_sections(fixture(), &Config::default());
        assert_eq!(
            rows.iter().map(|it| it.title.as_str()).collect::<Vec<_>>(),
            ["App", "Window", "Emoji", "Clipboard"]
        );
        assert!(rows[0].best);
        assert_eq!(crate::ui::section_of(&rows[0]), Some("Best Match"));
        assert_eq!(crate::ui::section_of(&rows[1]), Some("Windows"));
        let rows = group_sections(
            vec![(100, fixture()[1].1.clone()), (40, fixture()[0].1.clone())],
            &Config::default(),
        );
        assert_eq!(rows[0].title, "Window");
        assert!(!rows.iter().any(|it| it.best));
    }
    #[test]
    fn explicit_module_order_filters_items_and_keeps_one_commands_section() {
        let cfg: Config = toml::from_str("combined_modules = ['clipboard', 'applications', 'emojis']\n[fuzzy_match]\nshow_best_match = false").unwrap();
        let rows = group_sections(fixture(), &cfg);
        assert_eq!(
            rows.iter().map(|it| it.title.as_str()).collect::<Vec<_>>(),
            ["Clipboard", "Emoji", "App"]
        );
        assert!(!rows.iter().any(|it| it.best));
        assert_eq!(
            crate::ui::slots(&rows, 0, 400.0)
                .iter()
                .filter(|slot| slot.item.is_none())
                .map(|slot| slot.header)
                .collect::<Vec<_>>(),
            ["Commands", "Applications"]
        );
        let cfg: Config = toml::from_str("combined_modules = []").unwrap();
        assert!(group_sections(fixture(), &cfg).is_empty());
    }
    #[test]
    fn last_tied_maximum_is_promoted_after_module_sort() {
        let mut input = fixture();
        input[1].0 = 100;
        let rows = group_sections(input, &Config::default());
        assert_eq!(rows[0].title, "App");
        assert!(rows[0].best);
        let cfg: Config = toml::from_str(
            "disabled_modules = ['windows', 'applications']\ncombined_modules = ['applications']",
        )
        .unwrap();
        let rows = group_sections(fixture(), &cfg);
        assert_eq!(
            rows.iter().map(|it| it.title.as_str()).collect::<Vec<_>>(),
            ["Emoji", "Clipboard"]
        );
        assert!(!rows.iter().any(|it| it.best));
    }
}

#[cfg(test)]
mod web_search_tests {
    use super::*;

    fn rows(cfg: &Config, query: &str) -> Vec<Item> {
        let history = clip::History::new(0);
        Search {
            cfg,
            apps: &[],
            windows: &[],
            history: &history,
        }
        .run(query)
    }

    #[test]
    fn defaults_custom_empty_and_invalid_config_match_reference() {
        let cfg: Config = toml::from_str("").unwrap();
        let providers = cfg.search_providers.as_ref().unwrap();
        assert_eq!(
            providers
                .iter()
                .map(|p| (p.name.as_str(), p.trigger.as_str(), p.icon.as_str()))
                .collect::<Vec<_>>(),
            [
                ("Google", "!g", "magnifying-glass"),
                ("DuckDuckGo", "!d", "globe"),
                ("Wikipedia", "!wiki", "book-open"),
                ("YouTube", "!yt", "youtube-logo")
            ]
        );
        let encoded = toml::to_string(&cfg).unwrap();
        assert_eq!(
            toml::from_str::<Config>(&encoded).unwrap().search_providers,
            cfg.search_providers
        );
        let custom: Config = toml::from_str("[[search_providers]]\nname='Docs'\ntrigger='!doc'\nurl='https://example.invalid/?q={query}'\n").unwrap();
        assert_eq!(custom.search_providers.as_ref().unwrap().len(), 1);
        assert_eq!(
            rows(&custom, "!doc hello")[0].icon.as_deref(),
            Some("phosphor:magnifying-glass")
        );
        assert!(rows(
            &toml::from_str::<Config>("search_providers=[]").unwrap(),
            "unmatched words"
        )
        .is_empty());
        assert!(toml::from_str::<Config>("[[search_providers]]\nname='Incomplete'").is_err());
        let cfg = Config {
            search_providers: None,
            ..cfg
        };
        assert!(rows(&cfg, "unmatched words").is_empty());
    }

    #[test]
    fn fallback_rows_coexist_with_apps_but_are_never_best_match() {
        let cfg = Config::default();
        let apps = [App {
            name: "Needle".into(),
            comment: String::new(),
            exec: "true".into(),
            icon: String::new(),
            terminal: false,
        }];
        let history = clip::History::new(0);
        let result = Search {
            cfg: &cfg,
            apps: &apps,
            windows: &[],
            history: &history,
        }
        .run("Needle");
        assert_eq!(
            result.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(),
            [
                "Needle",
                "Search on Google",
                "Search on DuckDuckGo",
                "Search on Wikipedia",
                "Search on YouTube"
            ]
        );
        assert!(result.iter().filter(|i| i.tag == "Search").all(|i| !i.best
            && i.sub.is_empty()
            && crate::ui::section_of(i) == Some("Search and AI")));
        assert_eq!(
            rows(&cfg, "!wiki Rust & C")[0].action,
            Action::WebSearch(
                "https://en.wikipedia.org/wiki/Special:Search?search=Rust%20%26%20C".into()
            )
        );
        assert!(rows(&cfg, "!g").is_empty());
    }

    #[test]
    fn search_has_no_ai_trigger_or_rows() {
        let mut cfg = Config::default();
        for query in ["!ai rust", "!ai", "ordinary query"] {
            let result = rows(&cfg, query);
            assert_eq!(
                result.iter().map(|r| r.tag).collect::<Vec<_>>(),
                ["Search"; 4]
            );
        }
        cfg.combined_modules = Some(vec![config::ConfigModule::Search]);
        assert!(rows(&cfg, "").is_empty());
        assert_eq!(
            rows(&cfg, "!d rust")[0].action,
            Action::WebSearch("https://duckduckgo.com/?q=rust".into())
        );
        cfg.combined_modules = Some(vec![config::ConfigModule::Applications]);
        assert!(rows(&cfg, "!g rust").is_empty());
    }
}
