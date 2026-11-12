//! Result details and quicklink usage stats.
use crate::{
    config::Config,
    dates,
    hist::Hist,
    items::{self, Action, App, Item},
    ui::{self, Canvas},
};
use rusttype::Font;
fn fmt_ts(t: u64) -> String {
    let off = dates::utc_offset_secs();
    let local = t as i64 + off;
    let days = local.div_euclid(86_400);
    let secs = local.rem_euclid(86_400);
    let (y, m, d) = dates::civil_from_days(days);
    let wd = &dates::WEEKDAYS[dates::weekday(days) as usize][..3];
    let mon = &dates::MONTHS[(m - 1) as usize][..3];
    format!(
        "{} {} {} {:02}:{:02}:{:02} {}",
        wd,
        mon,
        d,
        secs / 3600,
        secs % 3600 / 60,
        secs % 60,
        y
    )
}

pub fn text(it: &Item, cfg: &Config, apps: &[App], hist: &Hist) -> Option<String> {
    match &it.action {
        Action::App(i) => {
            let a = apps.get(*i)?;
            Some(format!(
                "{}\n\n{}\n\nRuns: {}{}",
                a.name,
                if a.comment.is_empty() {
                    "No description"
                } else {
                    &a.comment
                },
                a.exec,
                if a.terminal {
                    "\nOpens in a terminal"
                } else {
                    ""
                }
            ))
        }
        Action::Web(url) => {
            let mut out = format!("{}\n\n{}", it.title, url);
            let (n, last) = hist.stats(&items::key_of(it)).unwrap_or((0, 0));
            out.push_str(&format!("\n\nOpened: {n}"));
            out.push_str(&format!(
                "\nLast opened: {}",
                if last == 0 {
                    "Never".into()
                } else {
                    fmt_ts(last)
                }
            ));
            let created = cfg
                .link
                .iter()
                .find(|l| l.name == it.title)
                .map_or(0, |l| l.created);
            if created > 0 {
                out.push_str(&format!("\nCreated: {}", fmt_ts(created)));
            }
            Some(out)
        }
        _ => None,
    }
}

pub fn draw(cv: &mut Canvas, font: &Font, v: &ui::View, text: &str) {
    let s = v.scale as f32;
    let w = v.w as f32 * s;
    let (x, y, pw, ph) = (w * 0.45, ui::HEAD * s, w * 0.55, v.vp * s);
    cv.rrect(x, y, pw, ph, 8.0 * s, v.theme.bg, 1.0);
    cv.rrect_outline(
        x,
        y,
        pw,
        ph,
        8.0 * s,
        s,
        v.theme.border,
        v.theme.border_alpha,
    );
    let fs = 12.5 * s;
    for (i, line) in text.lines().enumerate() {
        let ty = y + 14.0 * s + i as f32 * 20.0 * s;
        if ty + fs > y + ph {
            break;
        }
        cv.text(
            font,
            fs,
            x + 14.0 * s,
            ty,
            line,
            v.theme.text,
            v.theme.text_alpha,
            pw - 28.0 * s,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn never_opened_link_omits_unknown_creation_time() {
        let it = Item {
            title: "Docs".into(),
            sub: String::new(),
            tag: "Link",
            icon: None,
            action: Action::Web("https://example.invalid".into()),
            best: false,
        };
        let cfg: Config =
            toml::from_str("[[link]]\nname = 'Docs'\nurl = 'https://example.invalid'\n").unwrap();
        assert_eq!(cfg.link[0].created, 0);
        assert_eq!(
            text(&it, &cfg, &[], &Hist::default()).unwrap(),
            "Docs\n\nhttps://example.invalid\n\nOpened: 0\nLast opened: Never"
        );
    }
    #[test]
    fn link_creation_and_usage_survive_reload() {
        let _guard = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let config_env = std::env::var_os("XDG_CONFIG_HOME");
        let state_env = std::env::var_os("XDG_STATE_HOME");
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        std::env::set_var("XDG_STATE_HOME", dir.path());
        let mut cfg = Config::default();
        assert!(crate::settings::add_link(
            &mut cfg,
            crate::config::Link {
                name: "Docs".into(),
                url: "https://example.invalid".into(),
                ..Default::default()
            }
        ));
        let saved = Config::load();
        assert!(saved.link[0].created > 0);
        let it = Item {
            title: "Docs".into(),
            sub: String::new(),
            tag: "Link",
            icon: None,
            action: Action::Web(saved.link[0].url.clone()),
            best: false,
        };
        let mut hist = Hist::load();
        hist.record(&items::key_of(&it));
        let hist = Hist::load();
        assert_eq!(hist.stats(&items::key_of(&it)).unwrap().0, 1);
        let body = text(&it, &saved, &[], &hist).unwrap();
        assert!(body.contains("Opened: 1\nLast opened: "));
        assert!(body.contains("\nCreated: "));
        for (key, old) in [
            ("XDG_CONFIG_HOME", config_env),
            ("XDG_STATE_HOME", state_env),
        ] {
            if let Some(value) = old {
                std::env::set_var(key, value);
            } else {
                std::env::remove_var(key);
            }
        }
    }
}
