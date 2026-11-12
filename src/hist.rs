//! Usage history: how often and how recently each result was picked. Kept in a small text file.
use std::collections::HashMap;

#[derive(Default, Debug)]
pub struct Hist {
    map: HashMap<String, (u32, u64)>,
    /// Favorites, in the order they show at the empty search. Also protects pinned clipboard entries.
    pins: Vec<String>,
}

/// Key every fresh install starts with, like vicinae's default "clipboard:history" favorite.
const SEED_PIN: &str = "Basic:Clipboard History";

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl Hist {
    pub fn path() -> std::path::PathBuf {
        let base = std::env::var("XDG_STATE_HOME").unwrap_or_else(|_| {
            format!("{}/.local/state", std::env::var("HOME").unwrap_or_default())
        });
        std::path::PathBuf::from(base).join("zl/history")
    }
    pub fn load() -> Hist {
        let mut h = Hist::default();
        if let Ok(s) = std::fs::read_to_string(Self::path()) {
            for l in s.lines() {
                let mut p = l.splitn(3, '\t');
                if let (Some(c), Some(t), Some(k)) = (p.next(), p.next(), p.next()) {
                    if let (Ok(c), Ok(t)) = (c.parse(), t.parse()) {
                        h.map.insert(k.to_string(), (c, t));
                    }
                }
            }
        }
        let pins_path = Self::path().with_file_name("pins");
        match std::fs::read_to_string(&pins_path) {
            Ok(s) => {
                h.pins = s
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(String::from)
                    .collect();
            }
            Err(_) => {
                // No pins file yet: seed the default favorite, like vicinae ships clipboard:history.
                h.pins = vec![SEED_PIN.to_string()];
                if let Some(d) = pins_path.parent() {
                    let _ = std::fs::create_dir_all(d);
                }
                let _ = std::fs::write(&pins_path, format!("{SEED_PIN}\n"));
            }
        }
        h
    }
    /// (times opened, unix seconds of the last open) for a result key.
    pub fn stats(&self, key: &str) -> Option<(u32, u64)> {
        self.map.get(key).copied()
    }
    pub fn is_pinned(&self, key: &str) -> bool {
        self.pins.iter().any(|p| p == key)
    }
    /// Position in the favorites order, when pinned.
    pub fn pin_order(&self, key: &str) -> Option<usize> {
        self.pins.iter().position(|p| p == key)
    }
    fn write_pins(&self) {
        let p = Self::path().with_file_name("pins");
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let body: String = self
            .pins
            .iter()
            .filter(|k| !k.contains('\n'))
            .map(|k| format!("{k}\n"))
            .collect();
        let _ = std::fs::write(&p, body);
    }
    /// Pin or unpin a result. Returns true when it is now pinned.
    pub fn toggle_pin(&mut self, key: &str) -> bool {
        let now = if let Some(i) = self.pins.iter().position(|p| p == key) {
            self.pins.remove(i);
            false
        } else {
            self.pins.push(key.to_string());
            true
        };
        self.write_pins();
        now
    }
    /// Swap two favorites in the order. Returns false unless both are pinned.
    pub fn swap_pins(&mut self, a: &str, b: &str) -> bool {
        let (Some(i), Some(j)) = (self.pin_order(a), self.pin_order(b)) else {
            return false;
        };
        self.pins.swap(i, j);
        self.write_pins();
        true
    }
    /// Forget every recorded pick for a result, so it ranks by title match alone again.
    pub fn reset(&mut self, key: &str) {
        self.map.remove(key);
        self.save();
    }
    pub fn record(&mut self, key: &str) {
        let e = self.map.entry(key.to_string()).or_insert((0, 0));
        e.0 += 1;
        e.1 = now();
        if self.map.len() > 500 {
            let mut v: Vec<_> = self.map.iter().map(|(k, v)| (k.clone(), *v)).collect();
            v.sort_by_key(|x| std::cmp::Reverse(x.1 .1));
            self.map = v.into_iter().take(400).collect();
        }
        self.save();
    }
    fn save(&self) {
        let p = Self::path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        let body: String = self
            .map
            .iter()
            .filter(|(k, _)| !k.contains(['\t', '\n']))
            .map(|(k, (c, t))| format!("{c}\t{t}\t{k}\n"))
            .collect();
        let tmp = p.with_extension("tmp");
        if std::fs::write(&tmp, body).is_ok() {
            let _ = std::fs::rename(&tmp, &p);
        }
    }
}

/// Recently opened files, newest first, one per line.
pub fn file_path() -> std::path::PathBuf {
    Hist::path().with_file_name("file-recent")
}

pub fn file_recent() -> Vec<String> {
    std::fs::read_to_string(file_path())
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

pub fn file_add(p: &str) {
    let mut v: Vec<String> = file_recent();
    v.retain(|x| x != p);
    v.insert(0, p.to_string());
    v.truncate(40);
    let f = file_path();
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(f, v.join("\n") + "\n");
}

/// Recently launched apps: one name per line, newest first.
pub fn app_path() -> std::path::PathBuf {
    Hist::path().with_file_name("app-recent")
}

pub fn app_recent() -> Vec<String> {
    std::fs::read_to_string(app_path())
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.is_empty())
        .map(String::from)
        .collect()
}

pub fn app_add(name: &str) {
    let mut v: Vec<String> = app_recent();
    v.retain(|x| x != name);
    v.insert(0, name.to_string());
    v.truncate(40);
    let f = app_path();
    if let Some(d) = f.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(f, v.join("\n") + "\n");
}

/// Calculator history: one "expression = result" line per copied answer, newest last.
/// New lines carry a "<unix seconds>\\t" prefix so the history view can group by day;
/// lines written before that still parse, with no date.
pub fn calc_path() -> std::path::PathBuf {
    Hist::path().with_file_name("calc-history")
}

/// Split a stored line into (unix seconds, "expr = result"). Undated legacy lines give None.
fn calc_split(line: &str) -> (Option<u64>, &str) {
    if let Some((t, rest)) = line.split_once('\t') {
        if !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()) {
            if let Ok(t) = t.parse() {
                return (Some(t), rest);
            }
        }
    }
    (None, line)
}

/// Pin key for a saved calculation: stable across the day-section tags.
pub fn calc_pin_key(expr: &str, result: &str) -> String {
    format!("CalcHist:{expr} = {result}")
}

fn calc_line_is_pinned(line: &str, h: &Hist) -> bool {
    let (_, body) = calc_split(line);
    body.rsplit_once(" = ")
        .is_some_and(|(e, r)| h.is_pinned(&calc_pin_key(e, r)))
}

pub fn calc_add(expr: &str, result: &str) {
    let p = calc_path();
    let mut lines: Vec<String> = std::fs::read_to_string(&p)
        .unwrap_or_default()
        .lines()
        .map(String::from)
        .collect();
    lines.push(format!(
        "{}\t{} = {}",
        now(),
        expr.trim().trim_end_matches('=').trim(),
        result
    ));
    if lines.len() > 50 {
        // Trim the oldest unpinned entries; pinned ones stay.
        let h = Hist::load();
        let mut excess = lines.len() - 50;
        lines.retain(|l| {
            if excess > 0 && !calc_line_is_pinned(l, &h) {
                excess -= 1;
                false
            } else {
                true
            }
        });
    }
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(p, lines.join("\n") + "\n");
}

pub fn calc_del(expr: &str, result: &str) {
    let p = calc_path();
    let lines: Vec<String> = std::fs::read_to_string(&p)
        .unwrap_or_default()
        .lines()
        .filter(|l| {
            let (_, body) = calc_split(l);
            body.rsplit_once(" = ")
                .map(|(a, b)| !(a == expr && b == result))
                .unwrap_or(true)
        })
        .map(String::from)
        .collect();
    let _ = std::fs::write(
        p,
        lines.join("\n") + if lines.is_empty() { "" } else { "\n" },
    );
}

pub fn calc_clear() {
    // Pinned entries survive, like pinned clipboard entries do.
    let h = Hist::load();
    let kept: String = std::fs::read_to_string(calc_path())
        .unwrap_or_default()
        .lines()
        .filter(|l| calc_line_is_pinned(l, &h))
        .map(|l| format!("{l}\n"))
        .collect();
    let _ = std::fs::write(calc_path(), kept);
}

/// Newest first: (expression, result, unix seconds when known).
pub fn calc_list() -> Vec<(String, String, Option<u64>)> {
    std::fs::read_to_string(calc_path())
        .unwrap_or_default()
        .lines()
        .rev()
        .filter_map(|l| {
            let (t, body) = calc_split(l);
            body.rsplit_once(" = ")
                .map(|(a, b)| (a.to_string(), b.to_string(), t))
        })
        .collect()
}

/// Recent search queries, newest first, one per line. Used by Up-arrow recall in the empty search.
pub fn queries() -> Vec<String> {
    let p = Hist::path().with_file_name("queries");
    std::fs::read_to_string(p)
        .map(|s| {
            s.lines()
                .filter(|l| !l.is_empty())
                .map(String::from)
                .collect()
        })
        .unwrap_or_default()
}

/// Remember a search query. Internal views, one-letter probes and clipboard searches are skipped.
pub fn add_query(q: &str) {
    let q = q.trim();
    if q.len() < 2
        || q.starts_with("zl settings")
        || q.starts_with("confirm ")
        || q.starts_with(';')
        || q.starts_with('>')
        || q == "fallback"
        || q.starts_with("fallback ")
        || q.contains(['\t', '\n'])
    {
        return;
    }
    let mut v = vec![q.to_string()];
    v.extend(queries().into_iter().filter(|x| x != q));
    v.truncate(50);
    let p = Hist::path().with_file_name("queries");
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let body: String = v.iter().map(|x| format!("{x}\n")).collect();
    use std::os::unix::fs::OpenOptionsExt;
    if std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(&p)
        .and_then(|mut f| std::io::Write::write_all(&mut f, body.as_bytes()))
        .is_err()
    {
        let _ = std::fs::write(&p, body);
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn calc_pin_survives_trim_and_clear() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zlcp{}", std::process::id()));
        std::fs::create_dir_all(dir.join("zl")).unwrap();
        std::env::set_var("XDG_STATE_HOME", &dir);
        for i in 0..50 {
            super::calc_add(&format!("{i} + 0"), &format!("{i}"));
        }
        let mut h = super::Hist::load();
        h.toggle_pin(&super::calc_pin_key("0 + 0", "0"));
        super::calc_add("51 + 0", "51");
        let v = super::calc_list();
        assert_eq!(v.len(), 50);
        assert!(v.iter().any(|(e, _, _)| e == "0 + 0"));
        assert!(!v.iter().any(|(e, _, _)| e == "1 + 0"));
        super::calc_clear();
        let v = super::calc_list();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].0, "0 + 0");
        std::env::remove_var("XDG_STATE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn query_history_skips_internal_and_dedupes() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = std::env::temp_dir().join(format!("zlqh{}", std::process::id()));
        std::fs::create_dir_all(dir.join("zl")).unwrap();
        std::env::set_var("XDG_STATE_HOME", &dir);
        super::add_query("");
        super::add_query("x");
        super::add_query("zl settings hotkeys ");
        super::add_query(";clip");
        super::add_query("firefox");
        super::add_query("terminal");
        super::add_query("firefox");
        let q = super::queries();
        assert_eq!(q, vec!["firefox".to_string(), "terminal".to_string()]);
        let mode = std::os::unix::fs::PermissionsExt::mode(
            &std::fs::metadata(super::Hist::path().with_file_name("queries"))
                .unwrap()
                .permissions(),
        );
        assert_eq!(mode & 0o777, 0o600);
        std::env::remove_var("XDG_STATE_HOME");
        let _ = std::fs::remove_dir_all(&dir);
    }
    #[test]
    fn pins_seed_order_swap_and_calc_timestamps() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let d = std::env::temp_dir().join(format!("zlhist{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::env::set_var("XDG_STATE_HOME", &d);
        // No pins file yet: the default favorite is seeded, like vicinae's clipboard:history.
        let mut h = super::Hist::load();
        assert!(h.is_pinned(super::SEED_PIN));
        h.toggle_pin("Command:World Clock");
        assert_eq!(h.pin_order(super::SEED_PIN), Some(0));
        assert_eq!(h.pin_order("Command:World Clock"), Some(1));
        assert!(h.swap_pins(super::SEED_PIN, "Command:World Clock"));
        assert_eq!(h.pin_order(super::SEED_PIN), Some(1));
        assert!(!h.swap_pins(super::SEED_PIN, "Command:Nope"));
        // An existing pins file is respected, even after removals.
        h.toggle_pin("Command:World Clock");
        let h2 = super::Hist::load();
        assert!(!h2.is_pinned("Command:World Clock"));
        // New calc lines carry a timestamp; lines from before still parse without one.
        super::calc_add("1 + 1", "2");
        let p = super::calc_path();
        let cur = std::fs::read_to_string(&p).unwrap();
        std::fs::write(&p, format!("3 * 3 = 9\n{cur}")).unwrap();
        let v = super::calc_list();
        assert_eq!(v[0].0, "1 + 1");
        assert!(v[0].2.is_some());
        assert_eq!(v[1], ("3 * 3".to_string(), "9".to_string(), None));
        super::calc_del("1 + 1", "2");
        assert_eq!(super::calc_list().len(), 1);
        std::env::remove_var("XDG_STATE_HOME");
        let _ = std::fs::remove_dir_all(&d);
    }
}
