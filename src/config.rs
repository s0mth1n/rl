//! Config file (~/.config/zl/config.toml).
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
    pub default_modes: Option<Vec<String>>,
    pub combined_modules: Option<Vec<ConfigModule>>,
    pub disabled_modules: Option<Vec<ConfigModule>>,
    pub fuzzy_match: FuzzyMatchConfig,
    pub search_providers: Option<Vec<ConfigSearchProvider>>,
    /// Output name to open on, such as "DP-1". Empty means the compositor's focused output.
    #[serde(skip)]
    pub monitor: String,
    pub launcher_size: Option<(f32, f32)>,
    pub enable_transparency: bool,
    pub hyprland_auto_blur: bool,
    pub store_passwords: bool,
    pub max_clipboard_history: Option<usize>,
    pub enable_backdrop: bool,
    pub window_size: Option<(f32, f32)>,
    pub layer_shell_layer: SurfaceLayer,
    #[serde(skip)]
    pub max_rows: usize,
    #[serde(skip)]
    pub width: u32,
    pub font: FontConfig,
    #[serde(skip)]
    pub font_path: String,
    /// Colour emoji font file (CBDT or sbix, such as Noto Color Emoji). Empty uses fontconfig's choice.
    #[serde(skip)]
    pub emoji_font: String,
    /// Collect clipboard history (type `;` to browse it).
    #[serde(skip)]
    pub clipboard_history: bool,
    /// Icon theme name looked up first, such as "Papirus". hicolor and common themes follow.
    #[serde(skip)]
    pub icon_theme: String,
    #[serde(skip)]
    pub disabled_apps: Vec<String>,
    #[serde(skip)]
    pub clipboard_persist: bool,
    #[serde(skip)]
    pub single_click: bool,
    /// Backspace on an empty view (emoji, clipboard) steps back one level.
    #[serde(skip)]
    pub pop_on_backspace: bool,
    /// "navigate_back" steps back one level per Escape, "close_window" hides at once.
    #[serde(skip)]
    pub escape_key_behavior: String,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct ConfigSearchProvider {
    pub name: String,
    pub trigger: String,
    pub url: String,
    #[serde(default)]
    pub icon: String,
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ConfigModule {
    Applications,
    Emojis,
    Calculator,
    Clipboard,
    Actions,
    Search,
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
            Applications,
            Search,
        ]
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct FuzzyMatchConfig {
    pub exact_match_bonus: i64,
    pub prefix_match_bonus: i64,
    pub word_prefix_bonus: i64,
    pub contiguity_bonus: i64,
    pub description_penalty: f64,
    pub action_score_multiplier: f64,
    pub submenu_score_multiplier: f64,
    pub show_best_match: bool,
}

impl Default for FuzzyMatchConfig {
    fn default() -> Self {
        Self {
            exact_match_bonus: 100_000,
            prefix_match_bonus: 50_000,
            word_prefix_bonus: 25_000,
            contiguity_bonus: 10_000,
            description_penalty: 0.3,
            action_score_multiplier: 0.8,
            submenu_score_multiplier: 0.9,
            show_best_match: true,
        }
    }
}

pub type FuzzyConfig = FuzzyMatchConfig;

impl Default for Config {
    fn default() -> Self {
        Config {
            default_modes: None,
            combined_modules: None,
            disabled_modules: None,
            fuzzy_match: FuzzyMatchConfig::default(),
            search_providers: Some(
                [
                    (
                        "Google",
                        "!g",
                        "https://www.google.com/search?q={query}",
                        "magnifying-glass",
                    ),
                    (
                        "DuckDuckGo",
                        "!d",
                        "https://duckduckgo.com/?q={query}",
                        "globe",
                    ),
                    (
                        "Wikipedia",
                        "!wiki",
                        "https://en.wikipedia.org/wiki/Special:Search?search={query}",
                        "book-open",
                    ),
                    (
                        "YouTube",
                        "!yt",
                        "https://www.youtube.com/results?search_query={query}",
                        "youtube-logo",
                    ),
                ]
                .map(|(name, trigger, url, icon)| ConfigSearchProvider {
                    name: name.into(),
                    trigger: trigger.into(),
                    url: url.into(),
                    icon: icon.into(),
                })
                .into(),
            ),
            monitor: String::new(),
            launcher_size: None,
            enable_transparency: true,
            hyprland_auto_blur: true,
            store_passwords: false,
            max_clipboard_history: None,
            enable_backdrop: true,
            window_size: None,
            layer_shell_layer: SurfaceLayer::Overlay,
            max_rows: 6,
            width: 600,
            font: FontConfig::default(),
            font_path: String::new(),
            emoji_font: String::new(),
            clipboard_history: true,
            icon_theme: String::new(),
            disabled_apps: Vec::new(),
            clipboard_persist: true,
            single_click: true,
            pop_on_backspace: true,
            escape_key_behavior: "navigate_back".into(),
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

/// Layout values in logical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ThemeMetrics {
    pub emoji_columns: usize,
    pub action_indicator_width: f32,
    pub action_indicator_right_position: f32,
    pub action_indicator_key_padding_x: f32,
    pub action_indicator_key_padding_top: f32,
    pub action_indicator_key_padding_bottom: f32,
    pub action_indicator_key_border_radius: f32,
    pub action_indicator_key_font_size: f32,
    pub action_indicator_key_line_height: f32,
    pub emoji_cell_size: f32,
    pub emoji_font_size: f32,
    pub emoji_cell_border_radius: f32,
    pub emoji_cell_gap: f32,
    pub ai_user_bubble_padding_x: f32,
    pub ai_user_bubble_padding_y: f32,
    pub ai_user_bubble_border_radius: f32,
    pub ai_message_gap: f32,
    pub markdown_paragraph_line_height: f32,
    pub markdown_heading_line_height: f32,
    pub markdown_code_block_radius: f32,
    pub markdown_code_line_height: f32,
    pub clipboard_color_icon_size: f32,
    pub clipboard_preview_padding: f32,
    pub clipboard_color_swatch_size: f32,
    pub clipboard_color_preview_gap: f32,
    pub clipboard_color_code_gap: f32,
    pub clipboard_color_label_width: f32,
    pub item_margin_x: f32,
    pub item_margin_y: f32,
    pub item_padding_x: f32,
    pub item_padding_y: f32,
    pub item_border_radius: f32,
    pub item_title_line_height: f32,
    pub item_content_height: f32,
    pub icon_size: f32,
    pub icon_border_radius: f32,
    pub empty_state_height: f32,
    pub layout_item_description_height: f32,
    pub layout_separator_width: f32,
    pub section_header_margin_top: f32,
    pub section_header_margin_bottom: f32,
    pub section_header_padding_y: f32,
}
impl Default for ThemeMetrics {
    fn default() -> Self {
        Self {
            emoji_columns: 8,
            action_indicator_width: 64.0,
            action_indicator_right_position: 8.0,
            action_indicator_key_padding_x: 4.0,
            action_indicator_key_padding_top: 2.0,
            action_indicator_key_padding_bottom: 1.0,
            action_indicator_key_border_radius: 3.0,
            action_indicator_key_font_size: 10.0,
            action_indicator_key_line_height: 10.0,
            emoji_cell_size: 64.0,
            emoji_font_size: 28.0,
            emoji_cell_border_radius: 6.0,
            emoji_cell_gap: 2.0,
            ai_user_bubble_padding_x: 12.0,
            ai_user_bubble_padding_y: 8.0,
            ai_user_bubble_border_radius: 12.0,
            ai_message_gap: 16.0,
            markdown_paragraph_line_height: 20.0,
            markdown_heading_line_height: 22.0,
            markdown_code_block_radius: 6.0,
            markdown_code_line_height: 18.0,
            clipboard_color_icon_size: 16.0,
            clipboard_preview_padding: 16.0,
            clipboard_color_swatch_size: 120.0,
            clipboard_color_preview_gap: 20.0,
            clipboard_color_code_gap: 8.0,
            clipboard_color_label_width: 60.0,
            item_margin_x: 8.0,
            item_margin_y: 1.0,
            item_padding_x: 8.0,
            item_padding_y: 7.0,
            item_border_radius: 6.0,
            item_title_line_height: 16.0,
            item_content_height: 34.0,
            icon_size: 24.0,
            icon_border_radius: 4.0,
            empty_state_height: 200.0,
            layout_item_description_height: 18.0,
            layout_separator_width: 1.0,
            section_header_margin_top: 8.0,
            section_header_margin_bottom: 4.0,
            section_header_padding_y: 6.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
    pub metrics: ThemeMetrics,
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
    pub calculator_background: Rgba,
    pub calculator_icon: Rgba,
    pub calculator_error: Rgba,
    pub ai_error_background: Rgba,
    pub ai_error_title: Rgba,
    pub ai_error_message: Rgba,
    pub ai_user_padding_x: f32,
    pub ai_user_padding_y: f32,
    pub ai_user_radius: f32,
    pub ai_message_gap: f32,
    pub markdown_code_radius: f32,
}

#[allow(dead_code)]
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
            metrics: ThemeMetrics::default(),
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
            calculator_background: Rgba::new([51, 128, 204], 0.15),
            calculator_icon: Rgba::new([125, 179, 232], 1.0),
            calculator_error: Rgba::new([224, 117, 82], 1.0),
            ai_error_background: Rgba::new([130, 23, 23], 1.0),
            ai_error_title: Rgba::new([255, 102, 102], 1.0),
            ai_error_message: Rgba::new([255, 168, 168], 1.0),
            ai_user_padding_x: 12.0,
            ai_user_padding_y: 8.0,
            ai_user_radius: 12.0,
            ai_message_gap: 16.0,
            markdown_code_radius: 6.0,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct FontConfig {
    pub font_family: Option<String>,
    pub mono_font_family: Option<String>,
    pub font_size: Option<f32>,
}

impl Config {
    pub fn startup_fonts(&self) -> FontConfig {
        FontConfig {
            font_family: self.font.font_family.clone(),
            mono_font_family: self.font.mono_font_family.clone(),
            font_size: self
                .font
                .font_size
                .filter(|size| size.is_finite() && *size > 0.0 && *size <= 1024.0),
        }
    }

    pub fn startup_font_size(&self) -> f32 {
        self.startup_fonts().font_size.unwrap_or(16.0)
    }

    pub fn font_file(&self, bold: bool) -> Option<String> {
        if !self.font_path.is_empty() {
            let path = std::path::Path::new(&self.font_path);
            if bold {
                if let (Some(stem), Some(ext)) = (path.file_stem(), path.extension()) {
                    let sibling = path.with_file_name(format!(
                        "{}-Bold.{}",
                        stem.to_string_lossy(),
                        ext.to_string_lossy()
                    ));
                    if sibling.is_file() {
                        return Some(sibling.to_string_lossy().into_owned());
                    }
                }
            } else {
                return Some(self.font_path.clone());
            }
        }
        let family = self
            .startup_fonts()
            .font_family
            .unwrap_or_else(|| "sans".into());
        let pattern = if bold {
            format!("{family}:weight=bold")
        } else {
            family
        };
        let output = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", &pattern])
            .output()
            .ok()?;
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
            .filter(|path| !path.is_empty())
    }

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

    pub fn panel_size(&self) -> (u32, u32) {
        let (w, h) = self.launcher_size.unwrap_or((600.0, 400.0));
        let pixels = |value: f32, fallback: u32| {
            if value.is_finite() && value > 0.0 {
                value.round().clamp(1.0, 8192.0) as u32
            } else {
                fallback
            }
        };
        (pixels(w, 600), pixels(h, 400))
    }

    pub fn validation_warnings(&self) -> Vec<(&'static str, String)> {
        let mut warnings = Vec::new();
        let (width, height) = self.launcher_size.unwrap_or((600.0, 400.0));
        for (value, label, low, high) in [
            (width, "Width", 300.0, 2000.0),
            (height, "Height", 200.0, 1500.0),
        ] {
            if value < low {
                warnings.push(("launcher_size", format!("{label} {value} is below minimum ({low}). Consider increasing for usability.")));
            } else if value > high {
                warnings.push((
                    "launcher_size",
                    format!(
                        "{label} {value} exceeds maximum ({high}). This may cause display issues."
                    ),
                ));
            }
        }
        if self.enable_backdrop {
            if let Some((w, h)) = self.window_size {
                for (value, panel, label) in [(w, width, "width"), (h, height, "height")] {
                    if value < panel {
                        warnings.push(("window_size", format!("window_size {label} ({value}) is smaller than launcher_size {label} ({panel}). Will use {panel}.")));
                    }
                }
            }
        }
        warnings
    }

    pub fn clipboard_limit(&self) -> usize {
        self.max_clipboard_history.filter(|&v| v > 0).unwrap_or(500)
    }

    pub fn path() -> std::path::PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .unwrap_or_else(|_| format!("{}/.config", std::env::var("HOME").unwrap_or_default()));
        std::path::PathBuf::from(base).join("zl/config.toml")
    }
    /// Load the config. A missing file gives defaults; a broken one is reported and defaults are used.
    pub fn load() -> Config {
        match read_source(Self::path()) {
            Ok(s) => match toml::from_str::<Config>(&s) {
                Ok(mut c) => {
                    for (field, warning) in c.validation_warnings() {
                        eprintln!("zl: Config validation: {field} - {warning}");
                    }
                    c.width = c.panel_size().0;
                    c.max_rows = c.max_rows.clamp(1, 20);
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
        let mut theme = Theme::default();
        if !self.enable_transparency {
            theme.bg_alpha = 1.0;
        }
        theme
    }
}

fn read_source(path: impl AsRef<std::path::Path>) -> std::io::Result<String> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    const MAX_SOURCE: u64 = 1024 * 1024;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_SOURCE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "config source is not a regular file up to 1 MiB",
        ));
    }
    let mut text = String::new();
    file.take(MAX_SOURCE + 1).read_to_string(&mut text)?;
    if text.len() as u64 > MAX_SOURCE {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "config source exceeds 1 MiB",
        ));
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_partial_config_and_ignores_legacy_theme_key() {
        let c: Config =
            toml::from_str("theme = \"nord\"\nmax_rows = 5\n[colors]\naccent = \"#ff0000\"\n")
                .unwrap();
        assert_eq!(c.max_rows, 6);
        assert!(c.disabled_apps.is_empty());
        assert_eq!(c.theme(), Theme::default());
    }

    #[test]
    fn default_matches_zlaunch() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let default = Config::default();
        let th = default.theme();
        assert_eq!(th.bg, [15; 3]);
        assert_eq!(th.bg_alpha, 0.7);
        assert_eq!(th.border, [255; 3]);
        assert_eq!(th.border_alpha, 0.094);
        assert_eq!(th.text_alpha, 0.9);
        assert_eq!(th.dim_alpha, 0.4);
        assert_eq!(th.select_alpha, 0.07);
        assert_eq!(th.emoji_selected.alpha, 0.1);
    }

    #[test]
    fn font_settings_are_global_and_legacy_theme_is_ignored() {
        let cfg: Config =
            toml::from_str("theme = 'nord'\n[font]\nfont_family = 'DejaVu Sans'\nfont_size = 15.0")
                .unwrap();
        assert_eq!(
            cfg.startup_fonts().font_family.as_deref(),
            Some("DejaVu Sans")
        );
        assert_eq!(cfg.startup_font_size(), 15.0);
        let saved = toml::to_string(&cfg).unwrap();
        assert!(!saved.contains("theme"));
    }

    #[test]
    fn dimension_warnings_are_nonfatal() {
        let _lock = crate::testutil::ENV_LOCK.lock().unwrap();
        assert!(Config::default().validation_warnings().is_empty());
        let mut cfg = Config {
            launcher_size: Some((250.0, 1600.0)),
            window_size: Some((200.0, 1200.0)),
            ..Config::default()
        };
        assert_eq!(cfg.validation_warnings(), vec![
            ("launcher_size", "Width 250 is below minimum (300). Consider increasing for usability.".into()),
            ("launcher_size", "Height 1600 exceeds maximum (1500). This may cause display issues.".into()),
            ("window_size", "window_size width (200) is smaller than launcher_size width (250). Will use 250.".into()),
            ("window_size", "window_size height (1200) is smaller than launcher_size height (1600). Will use 1600.".into()),
        ]);
        assert_eq!(cfg.panel_size(), (250, 1600));
        cfg.enable_backdrop = false;
        assert!(!cfg
            .validation_warnings()
            .iter()
            .any(|(field, _)| *field == "window_size"));
        cfg.launcher_size = Some((300.0, 1500.0));
        assert!(cfg.validation_warnings().is_empty());
        cfg.launcher_size = Some((2000.0, 200.0));
        assert!(cfg.validation_warnings().is_empty());
    }

    #[test]
    fn launcher_size_and_clipboard_defaults_follow_reference() {
        let cfg: Config = toml::from_str("").unwrap();
        assert_eq!(cfg.launcher_size, None);
        assert_eq!(cfg.panel_size(), (600, 400));
        assert_eq!(cfg.max_clipboard_history, None);
        assert_eq!(cfg.clipboard_limit(), 500);
        assert!(!cfg.store_passwords);
        assert!(cfg.enable_transparency);
        assert!(cfg.hyprland_auto_blur);
        for (value, expected) in [(0, 500), (1, 1), (600, 600)] {
            let cfg: Config = toml::from_str(&format!("max_clipboard_history = {value}")).unwrap();
            assert_eq!(cfg.clipboard_limit(), expected);
        }
    }

    #[test]
    fn launcher_size_drives_loaded_width_and_round_trips() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let old = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        std::fs::create_dir_all(Config::path().parent().unwrap()).unwrap();
        let source = "launcher_size = [720.0, 480.0]\nmax_clipboard_history = 601\nstore_passwords = true\nenable_transparency = false\nhyprland_auto_blur = false\n";
        std::fs::write(Config::path(), source).unwrap();
        let cfg = Config::load();
        assert_eq!(cfg.panel_size(), (720, 480));
        assert_eq!(cfg.width, 720);
        assert_eq!(cfg.clipboard_limit(), 601);
        assert!(cfg.store_passwords);
        assert!(!cfg.enable_transparency);
        assert!(!cfg.hyprland_auto_blur);
        let saved = toml::to_string(&cfg).unwrap();
        let loaded: Config = toml::from_str(&saved).unwrap();
        assert_eq!(loaded.panel_size(), (720, 480));
        assert_eq!(loaded.clipboard_limit(), 601);
        match old {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    fn removed_config_keys_are_ignored_and_not_saved() {
        let keys = [
            "monitor",
            "max_rows",
            "width",
            "emoji_font",
            "clipboard_history",
            "clipboard_max",
            "icon_theme",
            "terminal",
            "disabled_apps",
            "clipboard_persist",
            "single_click",
            "close_on_focus_loss",
            "pop_on_backspace",
            "escape_key_behavior",
            "colors",
        ];
        let mut table = toml::Table::new();
        for key in keys {
            table.insert(
                key.into(),
                toml::Value::String("ignored regardless of type".into()),
            );
        }
        table.insert("unknown_future_option".into(), toml::Value::Boolean(true));
        let cfg: Config = toml::from_str(&toml::to_string(&table).unwrap()).unwrap();
        let defaults = Config::default();
        assert_eq!(cfg.monitor, defaults.monitor);
        assert_eq!(cfg.max_rows, defaults.max_rows);
        assert_eq!(cfg.width, defaults.width);
        assert_eq!(cfg.clipboard_limit(), defaults.clipboard_limit());
        assert!(cfg.clipboard_history && cfg.clipboard_persist && cfg.single_click);
        assert!(cfg.pop_on_backspace);
        assert!(cfg.disabled_apps.is_empty());
        assert_eq!(cfg.theme(), defaults.theme());
        let saved: toml::Table = toml::from_str(&toml::to_string(&cfg).unwrap()).unwrap();
        for key in keys {
            assert!(!saved.contains_key(key), "{key}");
        }
        assert!(!saved.contains_key("unknown_future_option"));
    }

    #[test]
    fn transparency_changes_only_panel_background_alpha() {
        let mut cfg = Config::default();
        let transparent = cfg.theme();
        assert_eq!(transparent.bg_alpha, 0.7);
        cfg.enable_transparency = false;
        let opaque = cfg.theme();
        assert_eq!(opaque.bg_alpha, 1.0);
        assert_eq!(
            opaque,
            Theme {
                bg_alpha: 1.0,
                ..transparent
            }
        );
    }

    #[test]
    fn panel_size_rejects_nonfinite_and_nonpositive_buffers() {
        for (input, output) in [
            ((f32::NAN, f32::INFINITY), (600, 400)),
            ((0.0, -1.0), (600, 400)),
            ((700.4, 500.6), (700, 501)),
            ((9000.0, 10000.0), (8192, 8192)),
        ] {
            assert_eq!(
                Config {
                    launcher_size: Some(input),
                    ..Config::default()
                }
                .panel_size(),
                output
            );
        }
    }

    #[test]
    fn explicit_window_size_round_trips() {
        let cfg: Config = toml::from_str("window_size = [900.5, 700.0]").unwrap();
        assert_eq!(cfg.window_size, Some((900.5, 700.0)));
        let loaded: Config = toml::from_str(&toml::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(loaded.window_size, cfg.window_size);
        assert_eq!(Config::default().window_size, None);
    }

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
