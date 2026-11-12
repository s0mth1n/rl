//! Config file (~/.config/zl/config.toml) and colour themes.
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    /// Output name to open on, such as "DP-1". Empty means the compositor's focused output.
    pub monitor: String,
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
    pub colors: Colors,
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
            theme: "dark".into(),
            monitor: String::new(),
            max_rows: 8,
            width: 640,
            font: String::new(),
            emoji_font: String::new(),
            clipboard_history: true,
            clipboard_max: 100,
            icon_theme: String::new(),
            font_size: 15.0,
            terminal: std::env::var("TERMINAL")
                .map(|t| format!("{t} -e"))
                .unwrap_or_default(),
            disabled_apps: Vec::new(),
            clipboard_persist: true,
            single_click: false,
            colors: Colors::default(),
        }
    }
}

pub type Rgb = [u8; 3];

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub bg: Rgb,
    pub border: Rgb,
    pub text: Rgb,
    pub dim: Rgb,
    pub select: Rgb,
    pub accent: Rgb,
}

pub fn hex(s: &str) -> Option<Rgb> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(s, 16).ok()?;
    Some([(n >> 16) as u8, (n >> 8) as u8, n as u8])
}

fn t(bg: &str, border: &str, text: &str, dim: &str, select: &str, accent: &str) -> Theme {
    Theme {
        bg: hex(bg).unwrap(),
        border: hex(border).unwrap(),
        text: hex(text).unwrap(),
        dim: hex(dim).unwrap(),
        select: hex(select).unwrap(),
        accent: hex(accent).unwrap(),
    }
}

pub const THEMES: [&str; 7] = [
    "dark",
    "light",
    "nord",
    "gruvbox",
    "dracula",
    "catppuccin",
    "tokyo-night",
];

pub fn theme(name: &str) -> Theme {
    match name {
        "light" => t(
            "#fafafa", "#d8d8dc", "#1c1c1e", "#6e6e76", "#e8e8ee", "#3b6ef5",
        ),
        "nord" => t(
            "#2e3440", "#434c5e", "#eceff4", "#8f98ab", "#3b4252", "#88c0d0",
        ),
        "gruvbox" => t(
            "#282828", "#3c3836", "#ebdbb2", "#a89984", "#3c3836", "#fabd2f",
        ),
        "dracula" => t(
            "#282a36", "#44475a", "#f8f8f2", "#8b92b8", "#44475a", "#bd93f9",
        ),
        "catppuccin" => t(
            "#1e1e2e", "#313244", "#cdd6f4", "#9399b2", "#313244", "#cba6f7",
        ),
        "tokyo-night" => t(
            "#1a1b26", "#292e42", "#c0caf5", "#7f87ad", "#292e42", "#7aa2f7",
        ),
        _ => t(
            "#131416", "#2c2c30", "#eeeef0", "#85858f", "#24262a", "#6c9cff",
        ),
    }
}

impl Config {
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
                        15.0
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
        for (slot, v) in [
            (&mut th.bg, &c.bg),
            (&mut th.border, &c.border),
            (&mut th.text, &c.text),
            (&mut th.dim, &c.dim),
            (&mut th.select, &c.select),
            (&mut th.accent, &c.accent),
        ] {
            if let Some(x) = v.as_deref().and_then(hex) {
                *slot = x;
            }
        }
        th
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
        for n in THEMES {
            let _ = theme(n);
        }
    }
}
