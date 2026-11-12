//! Local script commands, ported from zl wip-script7.
use crate::{
    config::Config,
    items::{fuzzy, Action, Item},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScriptMode {
    /// Runs blind; nothing is shown.
    Silent,
    /// Runs blind, then the first output line shows briefly as a HUD toast.
    Hud,
    /// The script's first output line is the row's subtitle, refreshed on its @refreshInterval.
    Inline,
    /// The script's output shows in the launcher.
    Output,
    /// The script opens in a terminal window.
    Terminal,
}

impl ScriptMode {
    /// vicinae/Raycast @mode values: silent, noView and compact get the HUD toast; inline becomes
    /// the row subtitle; fullOutput shows output; terminal opens a terminal window.
    pub fn parse(v: &str) -> ScriptMode {
        match v.trim() {
            "fullOutput" => ScriptMode::Output,
            "inline" => ScriptMode::Inline,
            "compact" => ScriptMode::Hud,
            "silent" | "noView" => ScriptMode::Hud,
            "terminal" => ScriptMode::Terminal,
            _ => ScriptMode::Silent,
        }
    }
}

/// "@refreshInterval 10s", "5m", "1h" or bare seconds, for inline-mode scripts. 0 when absent.
fn parse_interval(v: &str) -> u64 {
    let v = v.trim();
    let (n, m) = match v.chars().last() {
        Some('s') => (&v[..v.len() - 1], 1),
        Some('m') => (&v[..v.len() - 1], 60),
        Some('h') => (&v[..v.len() - 1], 3600),
        _ => (v, 1),
    };
    n.trim().parse::<u64>().unwrap_or(0).saturating_mul(m)
}

#[derive(Clone, Debug)]
pub struct Script {
    pub name: String,
    pub comment: String,
    pub exec: String,
    pub icon: String,
    pub keywords: String,
    pub terminal: bool,
    pub mode: ScriptMode,
    pub arg: String,
    pub arg_opts: Vec<String>,
    pub arg_secret: bool,
    /// Inline-mode refresh interval in seconds; 0 runs the script once and caches the line.
    pub refresh_secs: u64,
    /// Working directory for the run: the @currentDirectoryPath header, else the script's dir.
    pub cwd: String,
    /// The @needsConfirmation header: ask before running.
    pub confirm: bool,
}

/// The scripts directories: <config>/zl/scripts first, then the config's script_dirs.
/// A "~/" prefix expands to the home directory. First dir wins when names collide.
pub fn script_dirs(cfg: &Config) -> Vec<std::path::PathBuf> {
    let mut v = vec![Config::path()
        .parent()
        .map(|p| p.join("scripts"))
        .unwrap_or_default()];
    for d in &cfg.script_dirs {
        let d = match d.strip_prefix("~/") {
            Some(r) => format!("{}/{r}", std::env::var("HOME").unwrap_or_default()),
            None => d.clone(),
        };
        v.push(std::path::PathBuf::from(d));
    }
    v
}

/// Script commands: executable files in the script directories. Header comments set how they look;
/// the vicinae and Raycast keys work too (`# @vicinae.title ...`). Headers:
///   @title Name   @description Text   @icon name   @keywords a b c
///   @mode silent|noView|compact|fullOutput|inline|terminal (compact/fullOutput/inline show the
///   script's output in the launcher; terminal opens it in a terminal window)
///   @argument1 { "type": "text", "placeholder": "Name" } asks for one argument before running
pub fn scan_scripts(cfg: &Config) -> Vec<Script> {
    use std::os::unix::fs::PermissionsExt;
    let mut out: Vec<Script> = Vec::new();
    // Scripts in a subfolder form a group: the folder name becomes a keyword.
    let mut files: Vec<(std::path::PathBuf, String)> = Vec::new();
    for dir in script_dirs(cfg) {
        let Ok(rd) = std::fs::read_dir(&dir) else {
            continue;
        };
        files.extend(
            rd.flatten()
                .map(|e| e.path())
                .filter(|p| p.is_file())
                .map(|p| (p, String::new())),
        );
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if let Ok(rd2) = std::fs::read_dir(&p) {
                        let g = p
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_default();
                        files.extend(
                            rd2.flatten()
                                .map(|e| e.path())
                                .filter(|p| p.is_file())
                                .map(|p| (p, g.clone())),
                        );
                    }
                }
            }
        }
    }
    let dirs = script_dirs(cfg);
    files.sort_by_key(|(p, _)| {
        (
            dirs.iter()
                .position(|d| p.starts_with(d))
                .unwrap_or(usize::MAX),
            p.clone(),
        )
    });
    for (p, group) in files {
        if std::fs::metadata(&p).map_or(true, |m| m.permissions().mode() & 0o111 == 0) {
            continue;
        }
        let Ok(txt) = std::fs::read_to_string(&p) else {
            continue;
        };
        let stem = p
            .file_stem()
            .map(|s| s.to_string_lossy().replace(['-', '_'], " "))
            .unwrap_or_default();
        let (mut name, mut comment, mut icon, mut keywords, mut arg) = (
            stem,
            String::new(),
            String::new(),
            String::new(),
            String::new(),
        );
        let (mut arg_opts, mut arg_secret) = (Vec::new(), false);
        let mut mode = ScriptMode::Silent;
        let mut refresh_secs = 0;
        // The working directory: @currentDirectoryPath, or the script's own directory.
        let mut cwd = p
            .parent()
            .map(|d| d.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut confirm = false;
        let mut package = String::new();
        for l in txt.lines().take(15) {
            let l = l.trim_start_matches('#').trim_start_matches("//").trim();
            let Some(l) = l.strip_prefix('@') else {
                continue;
            };
            // @vicinae.title, @raycast.title and @title all name the same thing.
            let l = l
                .strip_prefix("vicinae.")
                .or_else(|| l.strip_prefix("raycast."))
                .unwrap_or(l);
            if let Some(v) = l.strip_prefix("title") {
                name = v.trim().into();
            } else if let Some(v) = l.strip_prefix("description") {
                comment = v.trim().into();
            } else if let Some(v) = l.strip_prefix("icon") {
                icon = v.trim().into();
            } else if let Some(v) = l.strip_prefix("keywords") {
                keywords = v.trim().into();
            } else if let Some(v) = l.strip_prefix("packageName") {
                package = v.trim().into();
                if !keywords.is_empty() {
                    keywords.push(' ');
                }
                keywords.push_str(&package);
            } else if let Some(v) = l.strip_prefix("currentDirectoryPath") {
                cwd = v.trim().into();
            } else if let Some(v) = l.strip_prefix("needsConfirmation") {
                confirm = v.trim() == "true";
            } else if let Some(v) = l.strip_prefix("mode") {
                mode = ScriptMode::parse(v);
            } else if let Some(v) = l
                .strip_prefix("refreshInterval")
                .or_else(|| l.strip_prefix("refreshTime"))
            {
                refresh_secs = parse_interval(v);
            } else if let Some(v) = l.strip_prefix("argument") {
                if let Ok(j) = serde_json::from_str::<serde_json::Value>(
                    v.trim()
                        .trim_start_matches(|c: char| c.is_ascii_digit())
                        .trim(),
                ) {
                    arg = j
                        .get("placeholder")
                        .and_then(|p| p.as_str())
                        .unwrap_or("")
                        .to_string();
                    arg_secret = j.get("type").and_then(|t| t.as_str()) == Some("password");
                    if j.get("type").and_then(|t| t.as_str()) == Some("dropdown") {
                        arg_opts = j
                            .get("data")
                            .and_then(|d| d.as_array())
                            .map(|a| {
                                a.iter()
                                    .filter_map(|o| {
                                        o.get("title")
                                            .and_then(|t| t.as_str())
                                            .or_else(|| o.get("value").and_then(|t| t.as_str()))
                                            .map(|t| t.to_string())
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        if arg.is_empty() {
                            arg = "Pick a value".into();
                        }
                    }
                }
            }
        }
        if !group.is_empty() {
            if !keywords.is_empty() {
                keywords.push(' ');
            }
            keywords.push_str(&group);
        }
        if comment.is_empty() {
            comment = if !package.is_empty() {
                package.clone()
            } else if arg.is_empty() {
                "Script".into()
            } else {
                format!("Script, asks for: {arg}")
            };
        }
        if out.iter().any(|a: &Script| a.name == name) {
            continue;
        }
        let exec = format!("'{}'", p.to_string_lossy().replace('\'', "'\\''"));
        // An inline script's subtitle is its cached first output line, when one was run before.
        if mode == ScriptMode::Inline {
            if let Some(l) = inline_cached(&exec) {
                comment = l;
            }
        }
        out.push(Script {
            name,
            comment,
            exec,
            icon,
            keywords,
            terminal: mode == ScriptMode::Terminal,
            mode,
            arg,
            arg_opts,
            arg_secret,
            refresh_secs,
            cwd,
            confirm,
        });
    }
    out
}

fn inline_dir() -> std::path::PathBuf {
    let base = std::env::var("XDG_CACHE_HOME")
        .unwrap_or_else(|_| format!("{}/.cache", std::env::var("HOME").unwrap_or_default()));
    std::path::PathBuf::from(base).join("zl/inline")
}

fn inline_path(exec: &str) -> std::path::PathBuf {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in exec.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    inline_dir().join(format!("{h:x}"))
}

/// The cached first output line of an inline-mode script.
pub fn inline_cached(exec: &str) -> Option<String> {
    let l = std::fs::read_to_string(inline_path(exec))
        .ok()?
        .lines()
        .next()?
        .trim()
        .to_string();
    (!l.is_empty()).then_some(l)
}

/// Re-run inline-mode scripts whose cache is missing or older than their @refreshInterval.
/// Each runs detached; the new subtitle shows the next time the list is read.
pub fn refresh_inline(apps: &[Script]) {
    let _ = std::fs::create_dir_all(inline_dir());
    for a in apps.iter().filter(|a| {
        a.mode == ScriptMode::Inline
            && !a.confirm
            && a.arg.is_empty()
            && !disabled().contains(&format!("cmd:{}", a.name))
    }) {
        let c = inline_path(&a.exec);
        let run = run_with_cwd(&a.exec, &a.cwd);
        let stale = match std::fs::metadata(&c).and_then(|m| m.modified()) {
            Ok(t) => {
                a.refresh_secs > 0
                    && t.elapsed()
                        .map(|e| e.as_secs() >= a.refresh_secs)
                        .unwrap_or(true)
            }
            Err(_) => true,
        };
        if !stale {
            continue;
        }
        let q = |p: &std::path::Path| p.to_string_lossy().replace('\'', "'\\''");
        let tmp = c.with_extension("tmp");
        let sh = format!(
            "timeout 15 sh -c '{}' 2>/dev/null | head -1 >'{}'; mv '{}' '{}'",
            run.replace('\'', "'\\''"),
            q(&tmp),
            q(&tmp),
            q(&c)
        );
        let _ = std::process::Command::new("sh")
            .args(["-c", &sh])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

/// A cheap fingerprint of the scripts dir: changes when a script is added, removed or edited,
/// so the launcher can rescan without a restart.
pub fn scripts_sig(cfg: &Config) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for dir in script_dirs(cfg) {
        scripts_sig_dir(&dir, &mut h);
    }
    h
}

fn scripts_sig_dir(dir: &std::path::Path, h: &mut u64) {
    let mix = |p: &std::path::Path, h: &mut u64| {
        if let Ok(m) = std::fs::metadata(p) {
            let t = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .unwrap_or_default();
            *h ^= (t.as_nanos() as u64) ^ m.len() ^ p.as_os_str().len() as u64;
            *h = h.wrapping_mul(0x100000001b3);
        }
    };
    mix(dir, h);
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let p = e.path();
            mix(&p, h);
            if p.is_dir() {
                if let Ok(rd2) = std::fs::read_dir(&p) {
                    for e2 in rd2.flatten() {
                        mix(&e2.path(), h);
                    }
                }
            }
        }
    }
}

/// Prefix a run with the script's working directory, when it has one.
pub fn run_with_cwd(exec: &str, cwd: &str) -> String {
    if cwd.is_empty() {
        exec.to_string()
    } else {
        format!("cd '{}' && {exec}", cwd.replace('\'', "'\\''"))
    }
}

fn state_path(name: &str) -> std::path::PathBuf {
    let base = std::env::var("XDG_STATE_HOME")
        .unwrap_or_else(|_| format!("{}/.local/state", std::env::var("HOME").unwrap_or_default()));
    let dir = std::path::PathBuf::from(base).join("zl");
    dir.join(name)
}

pub fn disabled() -> std::collections::BTreeSet<String> {
    std::fs::read_to_string(state_path("scripts-disabled.txt"))
        .unwrap_or_default()
        .lines()
        .map(str::to_string)
        .collect()
}

pub fn toggle(name: &str) -> std::io::Result<()> {
    let mut set = disabled();
    let key = format!("cmd:{name}");
    if !set.remove(&key) {
        set.insert(key);
    }
    let p = state_path("scripts-disabled.txt");
    std::fs::create_dir_all(p.parent().unwrap())?;
    std::fs::write(p, set.into_iter().collect::<Vec<_>>().join("\n"))
}

/// Last run of each script command, one "name\tcode\tunix seconds" line per script.
/// Code is -1 while the run's exit code is unknown (blind silent/terminal runs).
fn runs_path() -> std::path::PathBuf {
    state_path("script-runs.txt")
}

pub fn script_runs() -> Vec<(String, i32, u64)> {
    std::fs::read_to_string(runs_path())
        .unwrap_or_default()
        .lines()
        .filter_map(|l| {
            let mut p = l.split('\t');
            Some((
                p.next()?.to_string(),
                p.next()?.parse().ok()?,
                p.next()?.parse().ok()?,
            ))
        })
        .collect()
}

fn write_runs(v: &[(String, i32, u64)]) {
    let body: Vec<String> = v
        .iter()
        .map(|(n, c, t)| format!("{}\t{c}\t{t}", n.replace('\t', " ")))
        .collect();
    let path = runs_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(path, body.join("\n"));
}

pub fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_default()
}

/// A script started; the exit code follows through script_ran_code when it is captured.
pub fn script_ran(name: &str) {
    let mut v = script_runs();
    let t = now();
    match v.iter_mut().find(|(n, _, _)| n == name) {
        Some(e) => *e = (name.to_string(), -1, t),
        None => v.push((name.to_string(), -1, t)),
    }
    write_runs(&v);
}

pub fn script_ran_code(name: &str, code: i32) {
    let mut v = script_runs();
    let t = now();
    match v.iter_mut().find(|(n, _, _)| n == name) {
        Some(e) => *e = (name.to_string(), code, t),
        None => v.push((name.to_string(), code, t)),
    }
    write_runs(&v);
}
/// The Manage Scripts view: every script command, Enter toggles it, the panel edits,
/// reveals or deletes it. Disabled scripts show "(off)".
pub fn script_items(cfg: &Config, apps: &[Script], q: &str) -> Vec<Item> {
    let dirs = script_dirs(cfg);
    let off = disabled();
    let runs = script_runs();
    let now = now();
    let mut v: Vec<Item> = apps
        .iter()
        .filter(|a| {
            let p = a.exec.trim_matches('\'');
            dirs.iter()
                .any(|d| p.starts_with(&d.to_string_lossy().as_ref().to_string()))
        })
        .filter(|a| q.is_empty() || fuzzy(q, &a.name).is_some())
        .map(|a| {
            let on = !off.contains(&format!("cmd:{}", a.name));
            let path = a.exec.trim_matches('\'').to_string();
            let sub = match runs.iter().find(|(n, _, _)| n == &a.name) {
                Some((_, code, ts)) => {
                    let ago = match now.saturating_sub(*ts) {
                        s if s < 60 => format!("{s}s ago"),
                        s if s < 3600 => format!("{}m ago", s / 60),
                        s if s < 86400 => format!("{}h ago", s / 3600),
                        s => format!("{}d ago", s / 86400),
                    };
                    let st = match code {
                        0 => "ok".to_string(),
                        -1 => "ran".to_string(),
                        c => format!("exit {c}"),
                    };
                    format!("{path} · {ago} {st}")
                }
                None => path,
            };
            Item {
                best: false,
                title: if on {
                    a.name.clone()
                } else {
                    format!("{} (off)", a.name)
                },
                sub,
                tag: "Script",
                icon: None,
                action: Action::ScriptToggle(a.name.clone()),
            }
        })
        .collect();
    v.sort_by_key(|i| i.title.to_lowercase());
    if v.is_empty() && q.is_empty() {
        v.push(Item {
            best: false,
            title: "Create sample scripts".into(),
            sub: "Write a few example script commands into the scripts directory".into(),
            tag: "Script",
            icon: None,
            action: Action::ScriptSamples,
        });
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn script_mode_parity() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        assert_eq!(ScriptMode::parse("silent"), ScriptMode::Hud);
        assert_eq!(ScriptMode::parse("noView"), ScriptMode::Hud);
        assert_eq!(ScriptMode::parse("compact"), ScriptMode::Hud);
        assert_eq!(ScriptMode::parse("inline"), ScriptMode::Inline);
        assert_eq!(ScriptMode::parse("fullOutput"), ScriptMode::Output);
        assert_eq!(ScriptMode::parse("terminal"), ScriptMode::Terminal);
        assert_eq!(parse_interval("10s"), 10);
        assert_eq!(parse_interval("5m"), 300);
        assert_eq!(parse_interval("1h"), 3600);
        assert_eq!(parse_interval("90"), 90);
        assert_eq!(parse_interval("soon"), 0);
    }

    #[test]
    fn script_dirs_and_manage_view() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zl-sc4-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let extra = dir.join("extra");
        std::fs::create_dir_all(dir.join("zl/scripts")).unwrap();
        std::fs::create_dir_all(&extra).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        std::env::set_var("XDG_CACHE_HOME", &dir);
        std::env::set_var("XDG_STATE_HOME", &dir);
        let write = |p: std::path::PathBuf, body: &str| {
            std::fs::write(&p, body).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        };
        write(
            dir.join("zl/scripts/one.sh"),
            "#!/bin/sh\n# @title One\necho 1\n",
        );
        write(extra.join("two.sh"), "#!/bin/sh\n# @title Two\necho 2\n");
        write(extra.join("one.sh"), "#!/bin/sh\n# @title One\necho dup\n");
        let mut cfg = Config::default();
        assert!(scan_scripts(&cfg).iter().all(|a| a.name != "Two"));
        cfg.script_dirs = vec![extra.to_string_lossy().into_owned()];
        let apps = scan_scripts(&cfg);
        assert!(apps.iter().any(|a| a.name == "Two"), "extra dir scanned");
        assert_eq!(
            apps.iter().filter(|a| a.name == "One").count(),
            1,
            "first dir wins on name collision"
        );
        // The signature covers both directories.
        let s1 = scripts_sig(&cfg);
        write(extra.join("three.sh"), "#!/bin/sh\necho 3\n");
        assert_ne!(s1, scripts_sig(&cfg));
        // The Manage view lists every script with its path and toggled state.
        let apps = scan_scripts(&cfg);
        let rows = script_items(&cfg, &apps, "");
        assert!(rows.iter().any(|i| i.title == "One"));
        toggle("One").unwrap();
        let rows = script_items(&cfg, &apps, "");
        assert!(rows.iter().any(|i| i.title == "One (off)"));
        toggle("One").unwrap();
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_CACHE_HOME");
        std::env::remove_var("XDG_STATE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn script_items_last_run_and_samples() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zl-sc6-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sd = dir.join("zl/scripts");
        std::fs::create_dir_all(&sd).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        std::env::set_var("XDG_CACHE_HOME", &dir);
        std::env::set_var("XDG_STATE_HOME", &dir);
        let cfg = Config::default();
        // An empty scripts directory offers to write the sample scripts.
        let rows = script_items(&cfg, &[], "");
        assert_eq!(rows.len(), 1);
        assert!(matches!(rows[0].action, Action::ScriptSamples));
        // A filtered view never offers them.
        assert!(script_items(&cfg, &[], "zz").is_empty());
        std::fs::write(sd.join("one.sh"), "#!/bin/sh\n# @title One\necho 1\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(sd.join("one.sh"), std::fs::Permissions::from_mode(0o755))
                .unwrap();
        }
        let apps = scan_scripts(&cfg);
        // No run yet: the subtitle is just the path.
        let rows = script_items(&cfg, &apps, "");
        let one = rows.iter().find(|i| i.title == "One").unwrap();
        assert!(!one.sub.contains("ago"), "no run recorded yet: {}", one.sub);
        // A recorded exit code shows as an ago-plus-status suffix.
        script_ran_code("One", 0);
        let rows = script_items(&cfg, &apps, "");
        let one = rows.iter().find(|i| i.title == "One").unwrap();
        assert!(one.sub.ends_with("ago ok"), "ok run: {}", one.sub);
        script_ran_code("One", 2);
        let rows = script_items(&cfg, &apps, "");
        let one = rows.iter().find(|i| i.title == "One").unwrap();
        assert!(one.sub.ends_with("ago exit 2"), "failed run: {}", one.sub);
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_CACHE_HOME");
        std::env::remove_var("XDG_STATE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn script_headers_and_grouping() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zl-sc3-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sd = dir.join("zl/scripts");
        std::fs::create_dir_all(sd.join("team")).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        std::env::set_var("XDG_CACHE_HOME", &dir);
        let write = |p: std::path::PathBuf, body: &str| {
            std::fs::write(&p, body).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        };
        write(
            sd.join("pick.sh"),
            "#!/bin/sh\n# @vicinae.title Pick One\n# @raycast.packageName Team Tools\n# @icon star\n# @mode inline\n# @refreshInterval 5m\n# @argument1 { \"type\": \"dropdown\", \"placeholder\": \"Which\", \"data\": [{\"title\": \"One\"}, {\"title\": \"Two\"}] }\necho one\n",
        );
        write(
            sd.join("pw.sh"),
            "#!/bin/sh\n# @title Needs Token\n# @argument1 { \"type\": \"password\", \"placeholder\": \"Token\" }\necho t\n",
        );
        write(
            sd.join("team/grp.sh"),
            "#!/bin/sh\n# @title Grouped\necho g\n",
        );
        let apps = scan_scripts(&Config::default());
        let p = apps.iter().find(|a| a.name == "Pick One").unwrap();
        assert_eq!(p.mode, ScriptMode::Inline);
        assert_eq!(p.refresh_secs, 300);
        assert_eq!(p.icon, "star");
        assert!(p.keywords.contains("Team Tools"), "{}", p.keywords);
        assert_eq!(p.arg, "Which");
        assert_eq!(p.arg_opts, vec!["One".to_string(), "Two".to_string()]);
        assert!(!p.arg_secret);
        let pw = apps.iter().find(|a| a.name == "Needs Token").unwrap();
        assert_eq!(pw.arg, "Token");
        assert!(pw.arg_secret);
        let g = apps.iter().find(|a| a.name == "Grouped").unwrap();
        assert!(g.keywords.contains("team"), "{}", g.keywords);
        // An inline script with a cached first line shows it as the subtitle.
        let cache = dir.join("zl/inline");
        std::fs::create_dir_all(&cache).unwrap();
        let c = inline_path(&p.exec);
        std::fs::write(&c, "three up\n").unwrap();
        let apps = scan_scripts(&Config::default());
        let p = apps.iter().find(|a| a.name == "Pick One").unwrap();
        assert_eq!(p.comment, "three up");
        // Stale logic: no interval means run once, an interval re-runs when older.
        assert!(inline_cached(&p.exec).as_deref() == Some("three up"));
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_CACHE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn script_args_cwd_confirm_and_package() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zl-sc5-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sd = dir.join("zl/scripts");
        std::fs::create_dir_all(&sd).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        std::env::set_var("XDG_STATE_HOME", &dir);
        let write = |n: &str, body: &str| {
            let p = sd.join(n);
            std::fs::write(&p, body).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        };
        write(
            "word.sh",
            "#!/bin/sh\n# @title Word Say\n# @argument1 { \"type\": \"text\", \"placeholder\": \"What\" }\necho $1\n",
        );
        write(
            "pick.sh",
            "#!/bin/sh\n# @title Pick Five\n# @argument1 { \"type\": \"dropdown\", \"data\": [{\"title\": \"Red\"}, {\"title\": \"Blue\"}] }\necho $1\n",
        );
        write(
            "nuke.sh",
            "#!/bin/sh\n# @title Nuke It\n# @needsConfirmation true\n# @packageName Danger Zone\n# @currentDirectoryPath /tmp\necho boom\n",
        );
        let cfg = Config::default();
        let apps = scan_scripts(&cfg);
        let nuke = apps.iter().find(|a| a.name == "Nuke It").unwrap();
        assert!(nuke.confirm, "needsConfirmation parsed");
        assert_eq!(nuke.cwd, "/tmp", "currentDirectoryPath wins");
        assert_eq!(nuke.comment, "Danger Zone", "packageName is the subtitle");
        let word = apps.iter().find(|a| a.name == "Word Say").unwrap();
        assert_eq!(
            word.cwd,
            sd.to_string_lossy(),
            "default cwd is the script dir"
        );
        let history = crate::clip::History::new(0);
        let s = crate::items::Search {
            cfg: &cfg,
            scripts: &apps,
            files: &[],
            apps: &[],
            history: &history,
        };
        // A text argument rides the root query.
        let r = s.run("Word Say hello there");
        assert_eq!(r.len(), 1);
        assert!(matches!(&r[0].action, Action::AppArg(_, v) if v == "hello there"));
        // A dropdown argument offers its options, filtered by the rest of the query.
        let r = s.run("Pick Five bl");
        assert_eq!(r.len(), 1);
        assert!(matches!(&r[0].action, Action::AppArg(_, v) if v == "Blue"));
        let r = s.run("Pick Five r");
        assert_eq!(r.len(), 1);
        assert!(matches!(&r[0].action, Action::AppArg(_, v) if v == "Red"));
        // A partial name never hijacks the search.
        assert!(!s
            .run("Word Sa x")
            .iter()
            .any(|i| matches!(i.action, Action::AppArg(..))));
        // The confirm view runs through AppForce, with or without an argument.
        let r = s.run("confirm script-run 2\tNuke It\t");
        assert!(matches!(&r[0].action, Action::AppForce(2, None)));
        let r = s.run("confirm script-run 0\tWord Say\thi");
        assert!(matches!(&r[0].action, Action::AppForce(0, Some(v)) if v == "hi"));
        std::env::remove_var("XDG_CONFIG_HOME");
        std::env::remove_var("XDG_STATE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn script_terminal_mode_and_rescan_sig() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zl-sc2-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let sd = dir.join("zl/scripts");
        std::fs::create_dir_all(&sd).unwrap();
        std::env::set_var("XDG_CONFIG_HOME", &dir);
        let write = |name: &str, body: &str| {
            let p = sd.join(name);
            std::fs::write(&p, body).unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        };
        write(
            "term.sh",
            "#!/bin/sh\n# @mode terminal\n# @title Term Thing\necho hi\n",
        );
        let apps = scan_scripts(&Config::default());
        let t = apps.iter().find(|a| a.name == "Term Thing").unwrap();
        assert_eq!(t.mode, ScriptMode::Terminal);
        assert!(t.terminal);
        // The signature notices a new script and an edit to an existing one.
        let s1 = scripts_sig(&Config::default());
        write("late.sh", "#!/bin/sh\n# @title Late Script\necho late\n");
        let s2 = scripts_sig(&Config::default());
        assert_ne!(s1, s2);
        std::fs::write(
            sd.join("late.sh"),
            "#!/bin/sh\n# @title Later Script\necho later\n",
        )
        .unwrap();
        assert_ne!(s2, scripts_sig(&Config::default()));
        // And the new script is found by a fresh scan without any restart.
        assert!(scan_scripts(&Config::default())
            .iter()
            .any(|a| a.name == "Later Script"));
        std::env::remove_var("XDG_CONFIG_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }
}

pub fn command(script: &Script, arg: Option<&str>) -> String {
    let mut cmd = run_with_cwd(&script.exec, &script.cwd);
    if let Some(v) = arg {
        cmd = format!("{} '{}'", cmd, v.replace('\'', "'\\''"));
    }
    cmd
}

pub struct Run {
    pub name: String,
    pub cmd: String,
    pub text: String,
    pub done: bool,
    pub toast: bool,
    pub finished: Option<std::time::Instant>,
    child: std::process::Child,
    out: std::process::ChildStdout,
    err: std::process::ChildStderr,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

impl Run {
    pub fn start(name: String, cmd: String, toast: bool) -> std::io::Result<Self> {
        use std::os::fd::AsRawFd;
        use std::os::unix::process::CommandExt;
        let mut child = std::process::Command::new("sh")
            .args(["-c", &cmd])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .process_group(0)
            .spawn()?;
        let out = child.stdout.take().unwrap();
        let err = child.stderr.take().unwrap();
        for fd in [out.as_raw_fd(), err.as_raw_fd()] {
            unsafe {
                let flags = libc::fcntl(fd, libc::F_GETFL);
                libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
            }
        }
        script_ran(&name);
        Ok(Self {
            name,
            cmd,
            text: String::new(),
            done: false,
            toast,
            finished: None,
            child,
            out,
            err,
            stdout: Vec::new(),
            stderr: Vec::new(),
        })
    }

    pub fn pump(&mut self) -> bool {
        use std::io::Read;
        if self.done {
            return false;
        }
        let old = self.stdout.len() + self.stderr.len();
        for (pipe, buf) in [
            (&mut self.out as &mut dyn Read, &mut self.stdout),
            (&mut self.err as &mut dyn Read, &mut self.stderr),
        ] {
            let mut chunk = [0u8; 4096];
            for _ in 0..64 {
                match pipe.read(&mut chunk) {
                    Ok(0) => break,
                    Ok(n) => {
                        let room = (1024usize * 1024).saturating_sub(buf.len());
                        buf.extend_from_slice(&chunk[..n.min(room)]);
                    }
                    Err(_) => break,
                }
            }
        }
        self.text = String::from_utf8_lossy(&self.stdout).into_owned();
        if let Ok(Some(status)) = self.child.try_wait() {
            self.done = true;
            self.finished = Some(std::time::Instant::now());
            let code = status.code().unwrap_or(128);
            script_ran_code(&self.name, code);
            if !status.success() {
                self.text = format!(
                    "The script failed (exit {code}): {}\n{}",
                    String::from_utf8_lossy(&self.stderr)
                        .lines()
                        .next()
                        .unwrap_or(""),
                    self.text
                );
            }
            if self.text.is_empty() {
                self.text = "The script printed nothing.".into();
            }
            return true;
        }
        old != self.stdout.len() + self.stderr.len()
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        if !self.done {
            unsafe {
                libc::kill(-(self.child.id() as i32), libc::SIGKILL);
            }
        }
        let _ = self.child.wait();
    }
}

pub fn output_items(run: &Run) -> Vec<Item> {
    run.text
        .lines()
        .map(|line| Item {
            title: line.into(),
            sub: String::new(),
            tag: "Output",
            icon: None,
            action: Action::ScriptOutput,
            best: false,
        })
        .collect()
}

pub fn samples(cfg: &Config) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let dir = script_dirs(cfg).remove(0);
    std::fs::create_dir_all(&dir)?;
    for (n, body) in [
        ("hello-world.sh", "#!/bin/sh\n# @vicinae.title Hello World\n# @vicinae.mode fullOutput\nprintf '\\033[32mHello from zl!\\033[0m This is a fullOutput script.\\n'\n"),
        ("counter.sh", "#!/bin/sh\n# @vicinae.title Count To\n# @vicinae.mode fullOutput\n# @vicinae.argument1 { \"type\": \"text\", \"placeholder\": \"How far\" }\ni=1\nwhile [ \"$i\" -le \"${1:-3}\" ]; do echo \"line $i\"; i=$((i+1)); done\n"),
        ("date-inline.sh", "#!/bin/sh\n# @vicinae.title Current Time\n# @vicinae.mode inline\n# @vicinae.refreshTime 30s\ndate '+%H:%M:%S'\n"),
    ] {
        match std::fs::OpenOptions::new().write(true).create_new(true).mode(0o755).open(dir.join(n)) {
            Ok(mut file) => file.write_all(body.as_bytes())?,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {},
            Err(e) => return Err(e),
        }
    }
    Ok(())
}

pub fn strip_ansi(t: &str) -> String {
    t.lines()
        .map(|l| {
            ansi_spans(l, [0; 3])
                .into_iter()
                .map(|(s, _)| s)
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Split a line into (text, colour) runs at ANSI SGR escape sequences. Only colours are
/// honoured; other attributes are dropped, and a lone "\x1b[" is kept as text.
pub fn ansi_spans(line: &str, default: [u8; 3]) -> Vec<(String, [u8; 3])> {
    const PALETTE: [[u8; 3]; 8] = [
        [0x00, 0x00, 0x00],
        [0xcc, 0x00, 0x00],
        [0x4e, 0x9a, 0x06],
        [0xc4, 0xa0, 0x00],
        [0x34, 0x65, 0xa4],
        [0x75, 0x50, 0x7b],
        [0x06, 0x98, 0x9a],
        [0xd3, 0xd7, 0xcf],
    ];
    const BRIGHT: [[u8; 3]; 8] = [
        [0x55, 0x57, 0x53],
        [0xef, 0x29, 0x29],
        [0x8a, 0xe2, 0x34],
        [0xfc, 0xe9, 0x4f],
        [0x72, 0x9f, 0xcf],
        [0xad, 0x7f, 0xa8],
        [0x34, 0xe2, 0xe2],
        [0xee, 0xee, 0xec],
    ];
    let mut out: Vec<(String, [u8; 3])> = Vec::new();
    let mut col = default;
    let mut cur = String::new();
    let mut rest = line;
    while let Some(i) = rest.find("\x1b[") {
        cur.push_str(&rest[..i]);
        rest = &rest[i + 2..];
        let Some(end) = rest.find('m') else {
            cur.push_str("\x1b[");
            cur.push_str(rest);
            rest = "";
            break;
        };
        let params = &rest[..end];
        rest = &rest[end + 1..];
        if !cur.is_empty() {
            out.push((std::mem::take(&mut cur), col));
        }
        for p in params.split(';') {
            let n: u8 = p.parse().unwrap_or(0);
            match n {
                0 => col = default,
                39 => col = default,
                30..=37 => col = PALETTE[(n - 30) as usize],
                90..=97 => col = BRIGHT[(n - 90) as usize],
                _ => {}
            }
        }
    }
    cur.push_str(rest);
    if !cur.is_empty() {
        out.push((cur, col));
    }
    if out.is_empty() {
        out.push((String::new(), default));
    }
    out
}

#[cfg(test)]
mod run_tests {
    use super::*;

    fn wait(run: &mut Run) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !run.done && std::time::Instant::now() < deadline {
            run.pump();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert!(run.done, "script did not finish");
    }

    #[test]
    fn output_failure_status_and_last_run() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let old = std::env::var_os("XDG_STATE_HOME");
        std::env::set_var("XDG_STATE_HOME", dir.path());
        let mut run = Run::start(
            "Test Output".into(),
            "printf 'first\\nsecond'; printf 'bad news\\n' >&2; exit 7".into(),
            false,
        )
        .unwrap();
        wait(&mut run);
        assert_eq!(
            run.text,
            "The script failed (exit 7): bad news\nfirst\nsecond"
        );
        assert!(script_runs()
            .iter()
            .any(|(name, code, _)| name == "Test Output" && *code == 7));
        assert_eq!(strip_ansi("\x1b[31mred\x1b[0m plain"), "red plain");
        assert_eq!(
            ansi_spans("\x1b[31mred", [255; 3]),
            vec![("red".into(), [204, 0, 0])]
        );
        match old {
            Some(v) => std::env::set_var("XDG_STATE_HOME", v),
            None => std::env::remove_var("XDG_STATE_HOME"),
        }
    }

    #[test]
    fn output_is_bounded_and_quiet_success_finishes() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let old = std::env::var_os("XDG_STATE_HOME");
        std::env::set_var("XDG_STATE_HOME", dir.path());
        let mut run = Run::start(
            "Large".into(),
            "head -c 1100000 /dev/zero | tr '\\000' x".into(),
            false,
        )
        .unwrap();
        wait(&mut run);
        assert_eq!(run.text.len(), 1024 * 1024);
        assert!(run.text.bytes().all(|b| b == b'x'));
        let mut run = Run::start("Quiet".into(), "true".into(), true).unwrap();
        wait(&mut run);
        assert_eq!(run.text, "The script printed nothing.");
        match old {
            Some(v) => std::env::set_var("XDG_STATE_HOME", v),
            None => std::env::remove_var("XDG_STATE_HOME"),
        }
    }

    #[test]
    fn samples_do_not_replace_existing_files() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let old = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        let cfg = Config::default();
        samples(&cfg).unwrap();
        let path = script_dirs(&cfg)[0].join("hello-world.sh");
        std::fs::write(&path, "keep mine").unwrap();
        samples(&cfg).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "keep mine");
        assert_eq!(scan_scripts(&cfg).len(), 3);
        match old {
            Some(v) => std::env::set_var("XDG_CONFIG_HOME", v),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    fn arguments_and_working_directories_are_quoted() {
        let script = Script {
            name: "Quoted".into(),
            comment: String::new(),
            exec: "printf '%s'".into(),
            icon: String::new(),
            keywords: String::new(),
            terminal: false,
            mode: ScriptMode::Output,
            arg: "Text".into(),
            arg_opts: vec![],
            arg_secret: false,
            refresh_secs: 0,
            cwd: "/tmp".into(),
            confirm: false,
        };
        let output = std::process::Command::new("sh")
            .args(["-c", &command(&script, Some("a'b; $(echo bad)"))])
            .output()
            .unwrap();
        assert!(output.status.success());
        assert_eq!(output.stdout, b"a'b; $(echo bad)");
    }
}
