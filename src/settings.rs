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

pub fn rows(cfg: &Config) -> Vec<Item> {
    let mut rows = vec![Item {
        title: "Excluded applications".into(),
        sub: format!(
            "{} excluded - choose applications to hide",
            cfg.disabled_apps.len()
        ),
        tag: "Settings",
        icon: None,
        action: Action::View("settings apps ".into()),
    }];
    for (key, title, value) in [
        ("theme", "Theme", cfg.theme.clone()),
        ("font_size", "Font size", format!("{} px", cfg.font_size)),
        ("max_rows", "Visible rows", cfg.max_rows.to_string()),
        ("width", "Panel width", format!("{} px", cfg.width)),
        (
            "clipboard_history",
            "Clipboard history",
            cfg.clipboard_history.to_string(),
        ),
        (
            "clipboard_persist",
            "Save clipboard between restarts",
            cfg.clipboard_persist.to_string(),
        ),
        (
            "clipboard_max",
            "Clipboard limit",
            cfg.clipboard_max.to_string(),
        ),
        (
            "single_click",
            "Single-click activation",
            cfg.single_click.to_string(),
        ),
    ] {
        rows.push(Item {
            title: title.into(),
            sub: format!("{value} - Enter to change"),
            tag: "Settings",
            icon: None,
            action: Action::Setting(key.into()),
        });
    }
    rows
}

pub fn step(cfg: &mut Config, key: &str) -> bool {
    let mut next = cfg.clone();
    match key {
        "theme" => {
            let i = THEMES.iter().position(|n| *n == cfg.theme).unwrap_or(0);
            next.theme = THEMES[(i + 1) % THEMES.len()].into();
        }
        "font_size" => {
            next.font_size = if cfg.font_size >= 22.0 {
                12.0
            } else {
                cfg.font_size + 1.0
            }
        }
        "max_rows" => {
            next.max_rows = if cfg.max_rows >= 12 {
                4
            } else {
                cfg.max_rows + 1
            }
        }
        "width" => {
            next.width = if cfg.width >= 960 {
                560
            } else {
                cfg.width + 80
            }
        }
        "clipboard_history" => next.clipboard_history = !cfg.clipboard_history,
        "clipboard_persist" => next.clipboard_persist = !cfg.clipboard_persist,
        "clipboard_max" => {
            next.clipboard_max = if cfg.clipboard_max >= 200 {
                25
            } else {
                cfg.clipboard_max + 25
            }
        }
        "single_click" => next.single_click = !cfg.single_click,
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
        assert!(step(&mut cfg, "theme"));
        assert_eq!(Config::load().theme, "light");
        assert!(step(&mut cfg, "clipboard_history"));
        assert!(!Config::load().clipboard_history);
        assert!(exclude(&mut cfg, "Hidden App"));
        assert!(Config::load().disabled_apps.is_empty());
        assert!(!step(&mut cfg, "not-a-setting"));
        let file = Config::path();
        std::fs::remove_file(&file).unwrap();
        std::fs::remove_dir(file.parent().unwrap()).unwrap();
        std::fs::write(file.parent().unwrap(), b"blocked").unwrap();
        let theme = cfg.theme.clone();
        assert!(!step(&mut cfg, "theme"));
        assert_eq!(cfg.theme, theme);
        assert!(!exclude(&mut cfg, "Blocked"));
        assert!(cfg.disabled_apps.is_empty());
        match old {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }
}
