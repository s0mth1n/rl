//! Basic settings written to the user's TOML config.
use crate::{
    config::{Config, THEMES},
    items::{Action, Item},
};

pub fn save(cfg: &Config) -> bool {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let path = Config::path();
    let Some(parent) = path.parent() else {
        return false;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return false;
    }
    let Ok(text) = toml::to_string_pretty(cfg) else {
        return false;
    };
    let temp = path.with_extension("toml.tmp");
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(&temp)
    else {
        return false;
    };
    file.write_all(text.as_bytes()).is_ok() && std::fs::rename(temp, path).is_ok()
}

fn on_off(v: bool) -> &'static str {
    if v {
        "On"
    } else {
        "Off"
    }
}

pub fn rows(cfg: &Config) -> Vec<Item> {
    let mut rows = vec![Item {
        title: "Excluded applications".into(),
        sub: format!("{} hidden from search results", cfg.disabled_apps.len()),
        tag: "Open",
        icon: None,
        action: Action::View("settings apps ".into()),
        best: false,
    }];
    let list: [(&str, &str, String, &'static str); 10] = [
        (
            "theme",
            "Theme",
            format!("{} - next theme on Enter", cfg.theme),
            "",
        ),
        (
            "font_size",
            "Font size",
            format!("{} px - larger on Enter", cfg.font_size),
            "",
        ),
        (
            "max_rows",
            "Visible rows",
            format!("{} rows before scrolling", cfg.max_rows),
            "",
        ),
        (
            "width",
            "Panel width",
            format!("{} px - wider on Enter", cfg.width),
            "",
        ),
        (
            "clipboard_history",
            "Clipboard history",
            "Remember what you copy".into(),
            on_off(cfg.clipboard_history),
        ),
        (
            "clipboard_persist",
            "Keep clipboard after restart",
            "Save history to disk".into(),
            on_off(cfg.clipboard_persist),
        ),
        (
            "clipboard_max",
            "Clipboard limit",
            format!("{} entries kept", cfg.clipboard_max),
            "",
        ),
        (
            "single_click",
            "Single-click activation",
            "Run an item with one click".into(),
            on_off(cfg.single_click),
        ),
        (
            "pop_on_backspace",
            "Backspace goes back",
            "Backspace on an empty view steps back".into(),
            on_off(cfg.pop_on_backspace),
        ),
        (
            "escape_key_behavior",
            "Escape key",
            "Escape inside a view: step back or close".into(),
            if cfg.escape_key_behavior == "close_window" {
                "Close window"
            } else {
                "Navigate back"
            },
        ),
    ];
    for (key, title, sub, tag) in list {
        rows.push(Item {
            title: title.into(),
            sub,
            tag: if tag.is_empty() { "Change" } else { tag },
            icon: None,
            action: Action::Setting(key.into()),
            best: false,
        });
    }
    rows
}

/// Move `v` one `unit` up or down inside `lo..=hi`, wrapping round at the ends.
fn cycle(v: f32, lo: f32, hi: f32, unit: f32, dir: i32) -> f32 {
    let n = v + unit * dir.signum() as f32;
    if n > hi {
        lo
    } else if n < lo {
        hi
    } else {
        n
    }
}

pub fn step(cfg: &mut Config, key: &str, dir: i32) -> bool {
    let mut next = cfg.clone();
    match key {
        "theme" => {
            let i = THEMES.iter().position(|n| *n == cfg.theme).unwrap_or(0);
            let n = THEMES.len() as i32;
            next.theme = THEMES[(i as i32 + dir).rem_euclid(n) as usize].into();
        }
        "font_size" => next.font_size = cycle(cfg.font_size, 12.0, 22.0, 1.0, dir),
        "max_rows" => next.max_rows = cycle(cfg.max_rows as f32, 4.0, 12.0, 1.0, dir) as usize,
        "width" => next.width = cycle(cfg.width as f32, 560.0, 960.0, 80.0, dir) as u32,
        "clipboard_history" => next.clipboard_history = !cfg.clipboard_history,
        "clipboard_persist" => next.clipboard_persist = !cfg.clipboard_persist,
        "clipboard_max" => {
            next.clipboard_max = cycle(cfg.clipboard_max as f32, 25.0, 200.0, 25.0, dir) as usize
        }
        "single_click" => next.single_click = !cfg.single_click,
        "pop_on_backspace" => next.pop_on_backspace = !cfg.pop_on_backspace,
        "escape_key_behavior" => {
            next.escape_key_behavior = if cfg.escape_key_behavior == "close_window" {
                "navigate_back".into()
            } else {
                "close_window".into()
            }
        }
        _ => return false,
    }
    if !save(&next) {
        return false;
    }
    *cfg = next;
    true
}

pub fn exclude(cfg: &mut Config, name: &str) -> bool {
    let mut next = cfg.clone();
    if next.disabled_apps.iter().any(|a| a == name) {
        next.disabled_apps.retain(|a| a != name);
    } else {
        next.disabled_apps.push(name.into());
    }
    if !save(&next) {
        return false;
    }
    *cfg = next;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_and_exclusions_survive_reload() {
        let _lock = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let old = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        let mut cfg = Config::default();
        assert!(exclude(&mut cfg, "Hidden App"));
        assert_eq!(Config::load().disabled_apps, vec!["Hidden App"]);
        assert!(step(&mut cfg, "theme", 1));
        assert_eq!(Config::load().theme, THEMES[1]);
        assert!(step(&mut cfg, "clipboard_history", 1));
        assert!(!Config::load().clipboard_history);
        assert!(exclude(&mut cfg, "Hidden App"));
        assert!(Config::load().disabled_apps.is_empty());
        assert!(cfg.pop_on_backspace);
        assert!(step(&mut cfg, "pop_on_backspace", 1));
        assert!(!Config::load().pop_on_backspace);
        assert_eq!(cfg.escape_key_behavior, "navigate_back");
        assert!(step(&mut cfg, "escape_key_behavior", 1));
        assert_eq!(Config::load().escape_key_behavior, "close_window");
        assert!(step(&mut cfg, "escape_key_behavior", 1));
        assert_eq!(cfg.escape_key_behavior, "navigate_back");
        assert!(!step(&mut cfg, "not-a-setting", 1));
        cfg.width = 640;
        assert!(step(&mut cfg, "width", -1));
        assert_eq!(cfg.width, 560);
        assert!(step(&mut cfg, "width", -1));
        assert_eq!(cfg.width, 960);
        let file = Config::path();
        std::fs::remove_file(&file).unwrap();
        std::fs::remove_dir(file.parent().unwrap()).unwrap();
        std::fs::write(file.parent().unwrap(), b"blocked").unwrap();
        let theme = cfg.theme.clone();
        assert!(!step(&mut cfg, "theme", 1));
        assert_eq!(cfg.theme, theme);
        assert!(!exclude(&mut cfg, "Blocked"));
        assert!(cfg.disabled_apps.is_empty());
        match old {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }
}
