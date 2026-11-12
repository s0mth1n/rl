//! Config file (~/.config/zl/config.toml) and colour themes.
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceLayer {
    Top,
    #[default]
    Overlay,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    pub combined_modules: Option<Vec<ConfigModule>>,
    pub disabled_modules: Option<Vec<ConfigModule>>,
    pub fuzzy_match: FuzzyMatchConfig,
    /// Output name to open on, such as "DP-1". Empty means the compositor's focused output.
    pub monitor: String,
    pub enable_backdrop: bool,
    pub layer_shell_layer: SurfaceLayer,
    pub max_rows: usize,
    pub width: u32,
    pub font: String,
    /// Colour emoji font file (CBDT or sbix, such as Noto Color Emoji). Empty uses fontconfig's choice.
    pub emoji_font: String,
    /// Collect clipboard history (type `;` to browse it).
    pub clipboard_history: bool,
    pub clipboard_max: usize,
    /// Icon theme name looked up first, such as "Papirus". hicolor and common themes follow.
    pub icon_theme: String,
    pub font_size: f32,
    /// Terminal prefix for Terminal=true apps, such as "foot -e". Empty hides those apps.
    pub terminal: String,
    pub disabled_apps: Vec<String>,
    pub clipboard_persist: bool,
    pub single_click: bool,
    /// Hide the panel when the keyboard focus moves away from it.
    pub close_on_focus_loss: bool,
    /// Backspace on an empty view (emoji, clipboard) steps back one level.
    pub pop_on_backspace: bool,
    /// "navigate_back" steps back one level per Escape, "close_window" hides at once.
    pub escape_key_behavior: String,
    pub colors: Colors,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConfigModule {
    Applications,
    Ai,
    Emojis,
    Calculator,
    Clipboard,
    Actions,
    Search,
    Themes,
    Windows,
}

impl ConfigModule {
    pub fn all() -> Vec<Self> {
        use ConfigModule::*;
        vec![
            Calculator,
            Windows,
            Emojis,
            Clipboard,
            Actions,
            Themes,
            Applications,
            Ai,
            Search,
        ]
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct FuzzyMatchConfig {
    pub show_best_match: bool,
}

impl Default for FuzzyMatchConfig {
    fn default() -> Self {
        Self {
            show_best_match: true,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Colors {
    pub bg: Option<String>,
    pub border: Option<String>,
    pub text: Option<String>,
    pub dim: Option<String>,
    pub select: Option<String>,
    pub accent: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            theme: "default".into(),
            combined_modules: None,
            disabled_modules: None,
            fuzzy_match: FuzzyMatchConfig::default(),
            monitor: String::new(),
            enable_backdrop: true,
            layer_shell_layer: SurfaceLayer::Overlay,
            max_rows: 6,
            width: 600,
            font: String::new(),
            emoji_font: String::new(),
            clipboard_history: true,
            clipboard_max: 100,
            icon_theme: String::new(),
            font_size: 14.0,
            terminal: std::env::var("TERMINAL")
                .map(|t| format!("{t} -e"))
                .unwrap_or_default(),
            disabled_apps: Vec::new(),
            clipboard_persist: true,
            single_click: true,
            close_on_focus_loss: true,
            pop_on_backspace: true,
            escape_key_behavior: "navigate_back".into(),
            colors: Colors::default(),
        }
    }
}

pub type Rgb = [u8; 3];

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub rgb: Rgb,
    pub alpha: f32,
}

impl Rgba {
    const fn new(rgb: Rgb, alpha: f32) -> Self {
        Self { rgb, alpha }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub bg: Rgb,
    pub border: Rgb,
    pub text: Rgb,
    pub dim: Rgb,
    pub select: Rgb,
    pub accent: Rgb,
    pub bg_alpha: f32,
    pub border_alpha: f32,
    pub text_alpha: f32,
    pub dim_alpha: f32,
    pub select_alpha: f32,
    pub accent_alpha: f32,
    pub window_border_radius: f32,
    pub item_background: Rgba,
    pub icon_placeholder_background: Rgba,
    pub icon_placeholder_color: Rgba,
    pub empty_state_color: Rgba,
    pub section_header_color: Rgba,
    pub action_label: Rgba,
    pub key_background: Rgba,
    pub key_border: Rgba,
    pub key_color: Rgba,
    pub emoji_selected: Rgba,
}

pub fn rgba(s: &str) -> Option<Rgba> {
    let s = s.trim().strip_prefix('#').unwrap_or(s.trim());
    if !s.bytes().all(|b| b.is_ascii_hexdigit()) || !matches!(s.len(), 3 | 4 | 6 | 8) {
        return None;
    }
    let expanded;
    let s = if s.len() <= 4 {
        expanded = s.chars().flat_map(|c| [c, c]).collect::<String>();
        expanded.as_str()
    } else {
        s
    };
    let alpha = if s.len() == 8 {
        u8::from_str_radix(&s[6..], 16).ok()? as f32 / 255.0
    } else {
        1.0
    };
    Some(Rgba::new(hex(&s[..6])?, alpha))
}

pub fn hex(s: &str) -> Option<Rgb> {
    let s = s.trim().strip_prefix('#').unwrap_or(s.trim());
    if s.len() != 6 || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

impl Default for Theme {
    fn default() -> Self {
        let white = [255; 3];
        Self {
            bg: [15; 3],
            border: white,
            text: white,
            dim: white,
            select: white,
            accent: [125, 179, 232],
            bg_alpha: 0.7,
            border_alpha: 0.094,
            text_alpha: 0.9,
            dim_alpha: 0.4,
            select_alpha: 0.07,
            accent_alpha: 1.0,
            window_border_radius: 12.0,
            item_background: Rgba::new([0; 3], 0.0),
            icon_placeholder_background: Rgba::new(white, 0.04),
            icon_placeholder_color: Rgba::new(white, 0.25),
            empty_state_color: Rgba::new(white, 0.25),
            section_header_color: Rgba::new(white, 0.4),
            action_label: Rgba::new(white, 0.4),
            key_background: Rgba::new(white, 0.06),
            key_border: Rgba::new(white, 0.12),
            key_color: Rgba::new(white, 0.6),
            emoji_selected: Rgba::new(white, 0.1),
        }
    }
}

pub const THEMES: [&str; 16] = [
    "default",
    "ayu-dark",
    "catppuccin-latte",
    "catppuccin-mocha",
    "dracula",
    "everforest",
    "gruvbox-dark",
    "kanagawa",
    "material",
    "monokai",
    "nord",
    "one-dark",
    "rose-pine",
    "solarized-dark",
    "synthwave",
    "tokyo-night",
];

const THEME_FILES: [&str; 15] = [
    include_str!("../assets/themes/ayu-dark.toml"),
    include_str!("../assets/themes/catppuccin-latte.toml"),
    include_str!("../assets/themes/catppuccin-mocha.toml"),
    include_str!("../assets/themes/dracula.toml"),
    include_str!("../assets/themes/everforest.toml"),
    include_str!("../assets/themes/gruvbox-dark.toml"),
    include_str!("../assets/themes/kanagawa.toml"),
    include_str!("../assets/themes/material.toml"),
    include_str!("../assets/themes/monokai.toml"),
    include_str!("../assets/themes/nord.toml"),
    include_str!("../assets/themes/one-dark.toml"),
    include_str!("../assets/themes/rose-pine.toml"),
    include_str!("../assets/themes/solarized-dark.toml"),
    include_str!("../assets/themes/synthwave.toml"),
    include_str!("../assets/themes/tokyo-night.toml"),
];

fn theme_color(value: &toml::Value) -> Option<Rgba> {
    if let Some(hex) = value.as_str() {
        return rgba(hex);
    }
    let fields = value.as_table()?;
    if fields.contains_key("h") {
        let n = |key: &str| {
            fields
                .get(key)?
                .as_float()
                .or_else(|| fields.get(key)?.as_integer().map(|v| v as f64))
        };
        let (h, s, l, a) = (
            n("h")? as f32,
            n("s")? as f32,
            n("l")? as f32,
            n("a")? as f32,
        );
        let chroma = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let hue = h.rem_euclid(1.0) * 6.0;
        let x = chroma * (1.0 - (hue % 2.0 - 1.0).abs());
        let rgb = match hue as u8 {
            0 => [chroma, x, 0.0],
            1 => [x, chroma, 0.0],
            2 => [0.0, chroma, x],
            3 => [0.0, x, chroma],
            4 => [x, 0.0, chroma],
            _ => [chroma, 0.0, x],
        };
        return Some(Rgba::new(
            rgb.map(|v| ((v + l - chroma / 2.0) * 255.0).round() as u8),
            a,
        ));
    }
    let n = |key: &str| u8::try_from(fields.get(key)?.as_integer()?).ok();
    Some(Rgba::new(
        [n("r")?, n("g")?, n("b")?],
        if fields.contains_key("a") {
            n("a")? as f32 / 255.0
        } else {
            1.0
        },
    ))
}

fn parse_theme(source: &str) -> Option<Theme> {
    let table: toml::Value = toml::from_str(source).ok()?;
    table.as_table()?;
    let mut theme = Theme::default();
    macro_rules! color {
        ($key:literal, $rgb:ident, $alpha:ident) => {
            if let Some(value) = table.get($key) {
                let color = theme_color(value)?;
                theme.$rgb = color.rgb;
                theme.$alpha = color.alpha;
            }
        };
        ($key:literal, $field:ident) => {
            if let Some(value) = table.get($key) {
                theme.$field = theme_color(value)?;
            }
        };
        ($section:literal, $key:literal, $field:ident) => {
            if let Some(section) = table.get($section) {
                section.as_table()?;
                if let Some(value) = section.get($key) {
                    theme.$field = theme_color(value)?;
                }
            }
        };
    }
    color!("window_background", bg, bg_alpha);
    color!("window_border", border, border_alpha);
    color!("item_title_color", text, text_alpha);
    color!("item_description_color", dim, dim_alpha);
    color!("item_background_selected", select, select_alpha);
    color!("item_background", item_background);
    color!("icon_placeholder_background", icon_placeholder_background);
    color!("icon_placeholder_color", icon_placeholder_color);
    color!("empty_state_color", empty_state_color);
    color!("section_header", "color", section_header_color);
    color!("action_indicator", "label_color", action_label);
    color!("action_indicator", "key_background", key_background);
    color!("action_indicator", "key_border", key_border);
    color!("action_indicator", "key_color", key_color);
    color!("emoji", "cell_selected_bg", emoji_selected);
    if let Some(section) = table.get("calculator") {
        section.as_table()?;
        if let Some(value) = section.get("icon_color") {
            let color = theme_color(value)?;
            theme.accent = color.rgb;
            theme.accent_alpha = color.alpha;
        }
    }
    if let Some(radius) = table.get("window_border_radius") {
        theme.window_border_radius = if let Some(text) = radius.as_str() {
            text.trim_start_matches("Pixels(")
                .trim_end_matches(')')
                .parse()
                .ok()?
        } else {
            radius
                .as_float()
                .or_else(|| radius.as_integer().map(|v| v as f64))? as f32
        };
    }
    Some(theme)
}

pub fn load_theme(name: &str) -> Option<Theme> {
    if name == "default" {
        return Some(Theme::default());
    }
    static LOADED: std::sync::OnceLock<[Theme; 15]> = std::sync::OnceLock::new();
    if let Some(index) = THEMES[1..].iter().position(|candidate| *candidate == name) {
        return Some(
            LOADED.get_or_init(|| {
                THEME_FILES.map(|source| parse_theme(source).expect("bundled theme"))
            })[index],
        );
    }
    // A theme name is a filename stem, never a path outside the themes directory.
    if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
        return None;
    }
    parse_theme(
        &std::fs::read_to_string(
            Config::path()
                .parent()?
                .join("themes")
                .join(format!("{name}.toml")),
        )
        .ok()?,
    )
}

pub fn theme(name: &str) -> Theme {
    load_theme(name).unwrap_or_default()
}

pub fn available_themes() -> Vec<(String, String)> {
    let mut themes: Vec<_> = THEMES
        .iter()
        .map(|name| (name.to_string(), "bundled".to_string()))
        .collect();
    if let Some(dir) = Config::path().parent().map(|p| p.join("themes")) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                if let Some(filename) = entry.file_name().to_str() {
                    if let Some(name) = filename.strip_suffix(".toml") {
                        if !themes.iter().any(|(n, _)| n == name) && load_theme(name).is_some() {
                            themes.push((name.into(), filename.into()));
                        }
                    }
                }
            }
        }
    }
    themes.sort_by(|a, b| a.0.cmp(&b.0));
    themes
}

impl Config {
    pub fn combined_modules(&self) -> Vec<ConfigModule> {
        if let Some(disabled) = &self.disabled_modules {
            ConfigModule::all()
                .into_iter()
                .filter(|m| !disabled.contains(m))
                .collect()
        } else {
            self.combined_modules
                .clone()
                .unwrap_or_else(ConfigModule::all)
        }
    }

    pub fn path() -> std::path::PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .unwrap_or_else(|_| format!("{}/.config", std::env::var("HOME").unwrap_or_default()));
        std::path::PathBuf::from(base).join("zl/config.toml")
    }
    /// Load the config. A missing file gives defaults; a broken one is reported and defaults are used.
    pub fn load() -> Config {
        match std::fs::read_to_string(Self::path()) {
            Ok(s) => match toml::from_str::<Config>(&s) {
                Ok(mut c) => {
                    c.width = c.width.clamp(400, 1200);
                    c.max_rows = c.max_rows.clamp(1, 20);
                    c.font_size = if c.font_size.is_finite() {
                        c.font_size.clamp(11.0, 24.0)
                    } else {
                        14.0
                    };
                    c.clipboard_max = c.clipboard_max.min(500);
                    c
                }
                Err(e) => {
                    eprintln!("zl: config error in {}: {e}", Self::path().display());
                    Config::default()
                }
            },
            Err(_) => Config::default(),
        }
    }
    pub fn theme(&self) -> Theme {
        let mut th = theme(&self.theme);
        let c = &self.colors;
        for (slot, alpha, v) in [
            (&mut th.bg, &mut th.bg_alpha, &c.bg),
            (&mut th.border, &mut th.border_alpha, &c.border),
            (&mut th.text, &mut th.text_alpha, &c.text),
            (&mut th.dim, &mut th.dim_alpha, &c.dim),
            (&mut th.select, &mut th.select_alpha, &c.select),
            (&mut th.accent, &mut th.accent_alpha, &c.accent),
        ] {
            if let Some(x) = v.as_deref().and_then(rgba) {
                *slot = x.rgb;
                *alpha = x.alpha;
            }
        }
        th
    }
}

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

/// Apply a theme from the picker and save it. Returns false when the write fails.
pub fn set_theme(cfg: &mut Config, name: &str) -> bool {
    let mut next = cfg.clone();
    next.theme = name.into();
    if !save(&next) {
        return false;
    }
    *cfg = next;
    true
}

#[cfg(test)]
mod save_tests {
    use super::*;
    #[test]
    fn saved_theme_survives_reload() {
        let _lock = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let old = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        let mut cfg = Config::default();
        assert!(set_theme(&mut cfg, "nord"));
        assert_eq!(Config::load().theme, "nord");
        let file = Config::path();
        std::fs::remove_file(&file).unwrap();
        std::fs::remove_dir(file.parent().unwrap()).unwrap();
        std::fs::write(file.parent().unwrap(), b"blocked").unwrap();
        let theme = cfg.theme.clone();
        assert!(!set_theme(&mut cfg, "dracula"));
        assert_eq!(cfg.theme, theme);
        match old {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_partial_config() {
        let c: Config =
            toml::from_str("theme = \"nord\"\nmax_rows = 5\n[colors]\naccent = \"#ff0000\"\n")
                .unwrap();
        assert_eq!(c.max_rows, 5);
        assert!(c.disabled_apps.is_empty());
        assert_eq!(c.theme().accent, [255, 0, 0]);
        assert_eq!(c.theme().bg, theme("nord").bg);
    }
    #[test]
    fn bad_hex_is_ignored() {
        let c: Config = toml::from_str("[colors]\nbg = \"nope\"\n").unwrap();
        assert_eq!(c.theme().bg, theme("dark").bg);
    }
    #[test]
    fn all_themes_exist() {
        assert_eq!(THEMES.len(), THEME_FILES.len() + 1);
        for (name, source) in THEMES[1..].iter().zip(THEME_FILES) {
            let table: toml::Value = toml::from_str(source).unwrap();
            assert_eq!(table["name"].as_str(), Some(*name));
            let parsed = parse_theme(source).unwrap();
            assert_eq!(theme(name), parsed);
            assert!(parsed.bg_alpha > 0.0 && parsed.bg_alpha <= 1.0);
            assert_eq!(parsed.item_background, Rgba::new([0; 3], 0.0));
        }
        let nord = theme("nord");
        assert_eq!(nord.bg, [46, 52, 64]);
        assert_eq!(nord.bg_alpha, 179.0 / 255.0);
        assert_eq!(nord.border, [136, 192, 208]);
        assert_eq!(nord.border_alpha, 38.0 / 255.0);
        assert_eq!(nord.select_alpha, 26.0 / 255.0);
        assert_eq!(theme("dracula").window_border_radius, 14.0);
        assert_eq!(theme("catppuccin-latte").bg_alpha, 230.0 / 255.0);
    }

    #[test]
    fn default_matches_zlaunch() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let default = Config::default();
        assert_eq!(default.theme, "default");
        let th = default.theme();
        assert_eq!(th.bg, [15; 3]);
        assert_eq!(th.bg_alpha, 0.7);
        assert_eq!(th.border, [255; 3]);
        assert_eq!(th.border_alpha, 0.094);
        assert_eq!(th.text_alpha, 0.9);
        assert_eq!(th.dim_alpha, 0.4);
        assert_eq!(th.select_alpha, 0.07);
        assert_eq!(th.emoji_selected.alpha, 0.1);
        assert_eq!(theme("unknown"), th);
        assert_eq!(theme("dark"), th);
        assert_eq!(theme("light"), th);
        assert_eq!(theme("gruvbox"), th);
        assert_eq!(theme("catppuccin"), th);
    }

    #[test]
    fn user_themes_load_with_defaults_and_bundled_names_win() {
        let _lock = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let old = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        let themes = Config::path().parent().unwrap().join("themes");
        std::fs::create_dir_all(&themes).unwrap();
        std::fs::write(
            themes.join("custom.toml"),
            "window_background = '#12345680'\nwindow_border_radius = 'Pixels(9.0)'\n",
        )
        .unwrap();
        std::fs::write(themes.join("empty.toml"), "").unwrap();
        std::fs::write(themes.join("broken.toml"), "window_background = false").unwrap();
        std::fs::write(themes.join("nord.toml"), "window_background = '#ffffff'").unwrap();
        let custom = load_theme("custom").unwrap();
        assert_eq!(custom.bg, [18, 52, 86]);
        assert_eq!(custom.bg_alpha, 128.0 / 255.0);
        assert_eq!(custom.window_border_radius, 9.0);
        assert_eq!(custom.text, Theme::default().text);
        assert_eq!(load_theme("empty"), Some(Theme::default()));
        assert!(load_theme("broken").is_none());
        assert!(load_theme("missing").is_none());
        assert!(load_theme("../escape").is_none());
        assert_ne!(load_theme("nord").unwrap().bg, [255; 3]);
        let listed = available_themes();
        assert!(listed.contains(&("custom".into(), "custom.toml".into())));
        assert!(listed.contains(&("nord".into(), "bundled".into())));
        assert!(!listed.iter().any(|(name, _)| name == "broken"));
        for alias in ["dark", "light", "gruvbox", "catppuccin"] {
            assert!(load_theme(alias).is_none());
        }
        let cfg = Config {
            theme: "custom".into(),
            ..Config::default()
        };
        let rows = crate::items::theme_items(&cfg, "");
        assert_eq!(rows[0].title, "custom");
        assert_eq!(rows[0].sub, "custom.toml");
        assert_eq!(crate::items::theme_items(&cfg, ".toml").len(), 2);
        assert!(set_theme(&mut cfg.clone(), "custom"));
        assert_eq!(Config::load().theme(), custom);
        match old {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    fn theme_colors_accept_reference_table_formats() {
        for (source, expected) in [
            (
                "window_background = { r = 12, g = 34, b = 56 }",
                Rgba::new([12, 34, 56], 1.0),
            ),
            (
                "window_background = { r = 12, g = 34, b = 56, a = 128 }",
                Rgba::new([12, 34, 56], 128.0 / 255.0),
            ),
            (
                "window_background = { h = 0.0, s = 1.0, l = 0.5, a = 0.25 }",
                Rgba::new([255, 0, 0], 0.25),
            ),
        ] {
            let parsed = parse_theme(source).unwrap();
            assert_eq!(Rgba::new(parsed.bg, parsed.bg_alpha), expected);
        }
        for source in [
            "window_background = { r = 256, g = 0, b = 0 }",
            "window_border_radius = false",
            "window_background = { h = 0.0 }",
        ] {
            assert!(parse_theme(source).is_none(), "{source}");
        }
    }

    #[test]
    fn hex_formats_keep_alpha() {
        for value in ["#1234", "#11223344", "11223344"] {
            assert_eq!(rgba(value), Some(Rgba::new([17, 34, 51], 68.0 / 255.0)));
        }
        for value in ["#123", "#112233", " 112233 "] {
            assert_eq!(rgba(value), Some(Rgba::new([17, 34, 51], 1.0)));
        }
        for value in [
            "",
            "#",
            "#12",
            "#12345",
            "#1234567",
            "#123456789",
            "##123456",
            "+12345",
            "#gggggg",
            "#ééé",
        ] {
            assert_eq!(rgba(value), None, "{value}");
        }
    }

    #[test]
    fn overrides_replace_rgb_and_alpha_together() {
        let c: Config = toml::from_str(
            "[colors]\nbg = \"#1234\"\ntext = \"#aabbcc\"\nborder = \"bad color\"\n",
        )
        .unwrap();
        let th = c.theme();
        assert_eq!(th.bg, [17, 34, 51]);
        assert_eq!(th.bg_alpha, 68.0 / 255.0);
        assert_eq!(th.text, [170, 187, 204]);
        assert_eq!(th.text_alpha, 1.0);
        assert_eq!(th.border, theme("default").border);
        assert_eq!(th.border_alpha, 0.094);
    }

    #[test]
    fn broken_theme_source_is_rejected() {
        assert_eq!(parse_theme(""), Some(Theme::default()));
        assert!(parse_theme("not TOML").is_none());
        assert!(parse_theme(&THEME_FILES[0].replace("#0a0e14b3", "invalid")).is_none());
    }
    #[test]
    fn theme_loader_defaults_missing_and_rejects_wrong_typed_fields() {
        let source: toml::Value = toml::from_str(THEME_FILES[0]).unwrap();
        for key in [
            "window_background",
            "window_border",
            "item_title_color",
            "item_description_color",
            "item_background_selected",
            "window_border_radius",
            "item_background",
            "icon_placeholder_background",
            "icon_placeholder_color",
            "empty_state_color",
            "calculator",
            "section_header",
            "action_indicator",
            "emoji",
        ] {
            let mut missing = source.clone();
            missing.as_table_mut().unwrap().remove(key).unwrap();
            assert!(
                parse_theme(&toml::to_string(&missing).unwrap()).is_some(),
                "missing {key}"
            );
            let mut wrong_type = source.clone();
            wrong_type[key] = toml::Value::Boolean(false);
            assert!(
                parse_theme(&toml::to_string(&wrong_type).unwrap()).is_none(),
                "wrong type {key}"
            );
        }
        for (section, key) in [
            ("calculator", "icon_color"),
            ("section_header", "color"),
            ("action_indicator", "label_color"),
            ("action_indicator", "key_background"),
            ("action_indicator", "key_border"),
            ("action_indicator", "key_color"),
            ("emoji", "cell_selected_bg"),
        ] {
            let mut missing = source.clone();
            missing[section]
                .as_table_mut()
                .unwrap()
                .remove(key)
                .unwrap();
            assert!(
                parse_theme(&toml::to_string(&missing).unwrap()).is_some(),
                "missing {section}.{key}"
            );
        }
    }
    #[test]
    fn invalid_color_overrides_leave_the_whole_palette_unchanged() {
        let mut cfg = Config {
            theme: "nord".into(),
            ..Config::default()
        };
        for invalid in ["", "#12", "#zzzzzz", "##123456", "#ééé", "#123456789"] {
            cfg.colors = Colors {
                bg: Some(invalid.into()),
                border: Some(invalid.into()),
                text: Some(invalid.into()),
                dim: Some(invalid.into()),
                select: Some(invalid.into()),
                accent: Some(invalid.into()),
            };
            assert_eq!(cfg.theme(), theme("nord"), "{invalid}");
        }
    }
    #[test]
    fn color_alpha_endpoints_and_uppercase_hex_are_exact() {
        assert_eq!(rgba("#ABC0"), Some(Rgba::new([170, 187, 204], 0.0)));
        assert_eq!(rgba("#ABCf"), Some(Rgba::new([170, 187, 204], 1.0)));
        assert_eq!(rgba("#AABBCC00"), Some(Rgba::new([170, 187, 204], 0.0)));
        assert_eq!(rgba("#AABBCCFF"), Some(Rgba::new([170, 187, 204], 1.0)));
        assert_eq!(rgba("#00000000"), Some(Rgba::new([0; 3], 0.0)));
        assert_eq!(rgba("#FFFFFFFF"), Some(Rgba::new([255; 3], 1.0)));
    }
}

#[cfg(test)]
mod surface_config_tests {
    use super::*;

    #[test]
    fn surface_options_use_zlaunch_defaults() {
        let cfg: Config = toml::from_str("").unwrap();
        assert!(cfg.enable_backdrop);
        assert_eq!(cfg.layer_shell_layer, SurfaceLayer::Overlay);
    }

    #[test]
    fn surface_options_round_trip_and_reject_other_layers() {
        let cfg: Config = toml::from_str(
            r#"enable_backdrop = false
layer_shell_layer = "top""#,
        )
        .unwrap();
        assert!(!cfg.enable_backdrop);
        assert_eq!(cfg.layer_shell_layer, SurfaceLayer::Top);
        let saved = toml::to_string(&cfg).unwrap();
        let loaded: Config = toml::from_str(&saved).unwrap();
        assert!(!loaded.enable_backdrop);
        assert_eq!(loaded.layer_shell_layer, SurfaceLayer::Top);
        assert!(toml::from_str::<Config>(r#"layer_shell_layer = "background""#).is_err());
    }
}
