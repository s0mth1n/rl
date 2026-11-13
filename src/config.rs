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
            theme: "default".into(),
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

/// Layout values loaded from the theme file, in logical pixels.
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
        let hex = hex.trim_start_matches('#');
        if hex.bytes().any(|b| !b.is_ascii_hexdigit()) {
            return None;
        }
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

fn pixels(value: &toml::Value) -> Option<f32> {
    let value = if let Some(text) = value.as_str() {
        text.trim_start_matches("Pixels(")
            .trim_end_matches(')')
            .parse::<f32>()
            .ok()?
    } else {
        value
            .as_float()
            .or_else(|| value.as_integer().map(|v| v as f64))? as f32
    };
    (value.is_finite() && value.abs() <= 8192.0).then_some(value)
}

fn valid_theme_schema(table: &toml::Value) -> Option<()> {
    table.as_table()?;
    if let Some(value) = table.get("calculator") {
        value.as_table()?;
    }
    if let Some(value) = table.get("action_indicator") {
        value.as_table()?;
    }
    if let Some(value) = table.get("emoji") {
        value.as_table()?;
    }
    if let Some(value) = table.get("ai") {
        value.as_table()?;
    }
    if let Some(value) = table.get("markdown") {
        value.as_table()?;
    }
    if let Some(value) = table.get("clipboard") {
        value.as_table()?;
    }
    if let Some(value) = table.get("section_header") {
        value.as_table()?;
    }
    if let Some(value) = table.get("layout") {
        value.as_table()?;
    }
    for (section, key, kind) in [
        ("", "name", "string"),
        ("", "window_background", "color"),
        ("", "window_border", "color"),
        ("", "window_border_radius", "pixels"),
        ("", "item_margin_x", "pixels"),
        ("", "item_margin_y", "pixels"),
        ("", "item_padding_x", "pixels"),
        ("", "item_padding_y", "pixels"),
        ("", "item_border_radius", "pixels"),
        ("", "item_background", "color"),
        ("", "item_background_selected", "color"),
        ("", "item_title_color", "color"),
        ("", "item_description_color", "color"),
        ("", "item_title_line_height", "pixels"),
        ("", "item_content_height", "pixels"),
        ("", "icon_size", "pixels"),
        ("", "icon_border_radius", "pixels"),
        ("", "icon_placeholder_background", "color"),
        ("", "icon_placeholder_color", "color"),
        ("", "empty_state_height", "pixels"),
        ("", "empty_state_color", "color"),
        ("calculator", "icon_background", "color"),
        ("calculator", "icon_color", "color"),
        ("calculator", "error_color", "color"),
        ("action_indicator", "width", "pixels"),
        ("action_indicator", "right_position", "pixels"),
        ("action_indicator", "label_color", "color"),
        ("action_indicator", "key_background", "color"),
        ("action_indicator", "key_border", "color"),
        ("action_indicator", "key_color", "color"),
        ("action_indicator", "key_padding_x", "pixels"),
        ("action_indicator", "key_padding_top", "pixels"),
        ("action_indicator", "key_padding_bottom", "pixels"),
        ("action_indicator", "key_border_radius", "pixels"),
        ("action_indicator", "key_font_size", "pixels"),
        ("action_indicator", "key_line_height", "pixels"),
        ("emoji", "columns", "usize"),
        ("emoji", "cell_size", "pixels"),
        ("emoji", "font_size", "pixels"),
        ("emoji", "cell_selected_bg", "color"),
        ("emoji", "cell_border_radius", "pixels"),
        ("emoji", "cell_gap", "pixels"),
        ("ai", "error_background", "color"),
        ("ai", "error_title_color", "color"),
        ("ai", "error_message_color", "color"),
        ("ai", "user_bubble_background", "color"),
        ("ai", "user_bubble_text_color", "color"),
        ("ai", "user_bubble_padding_x", "pixels"),
        ("ai", "user_bubble_padding_y", "pixels"),
        ("ai", "user_bubble_border_radius", "pixels"),
        ("ai", "message_gap", "pixels"),
        ("markdown", "paragraph_line_height", "pixels"),
        ("markdown", "heading_line_height", "pixels"),
        ("markdown", "code_block_radius", "pixels"),
        ("markdown", "code_line_height", "pixels"),
        ("clipboard", "color_icon_size", "pixels"),
        ("clipboard", "preview_padding", "pixels"),
        ("clipboard", "color_swatch_size", "pixels"),
        ("clipboard", "color_preview_gap", "pixels"),
        ("clipboard", "color_code_gap", "pixels"),
        ("clipboard", "color_label_width", "pixels"),
        ("section_header", "color", "color"),
        ("section_header", "margin_top", "pixels"),
        ("section_header", "margin_bottom", "pixels"),
        ("section_header", "padding_y", "pixels"),
        ("layout", "separator_width", "pixels"),
        ("layout", "item_description_height", "pixels"),
    ] {
        let parent = if section.is_empty() {
            Some(table)
        } else {
            table.get(section)
        };
        if let Some(value) = parent.and_then(|p| p.get(key)) {
            match kind {
                "color" => {
                    theme_color(value)?;
                }
                "pixels" => {
                    let size = pixels(value)?;
                    if key.ends_with("font_size") && !(0.0 < size && size <= 1024.0) {
                        return None;
                    }
                }
                "string" => {
                    value.as_str()?;
                }
                "usize" => {
                    usize::try_from(value.as_integer()?).ok()?;
                }
                _ => unreachable!(),
            }
        }
    }
    if let Some(font) = table.get("font") {
        font.as_table()?;
        for key in ["font_family", "mono_font_family"] {
            if let Some(value) = font.get(key) {
                value.as_str()?;
            }
        }
        if let Some(value) = font.get("font_size") {
            let size = value
                .as_float()
                .or_else(|| value.as_integer().map(|v| v as f64))?;
            if !size.is_finite() || size <= 0.0 || size > 1024.0 {
                return None;
            }
        }
    }
    Some(())
}

fn parse_theme(source: &str) -> Option<Theme> {
    let table: toml::Value = toml::from_str(source).ok()?;
    valid_theme_schema(&table)?;
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
    color!("calculator", "icon_background", calculator_background);
    color!("calculator", "icon_color", calculator_icon);
    color!("calculator", "error_color", calculator_error);
    color!("ai", "error_background", ai_error_background);
    color!("ai", "error_title_color", ai_error_title);
    color!("ai", "error_message_color", ai_error_message);
    if let Some(radius) = table
        .get("markdown")
        .and_then(|m| m.get("code_block_radius"))
    {
        theme.markdown_code_radius = radius
            .as_float()
            .or_else(|| radius.as_integer().map(|n| n as f64))?
            as f32;
    }
    if let Some(ai) = table.get("ai") {
        for (key, field) in [
            ("user_bubble_padding_x", &mut theme.ai_user_padding_x),
            ("user_bubble_padding_y", &mut theme.ai_user_padding_y),
            ("user_bubble_border_radius", &mut theme.ai_user_radius),
            ("message_gap", &mut theme.ai_message_gap),
        ] {
            if let Some(v) = ai.get(key) {
                *field = v.as_float().or_else(|| v.as_integer().map(|v| v as f64))? as f32;
            }
        }
    }
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
    if let Some(value) = table.get("item_margin_x") {
        theme.metrics.item_margin_x = pixels(value)?;
    }
    if let Some(value) = table.get("item_margin_y") {
        theme.metrics.item_margin_y = pixels(value)?;
    }
    if let Some(value) = table.get("item_padding_x") {
        theme.metrics.item_padding_x = pixels(value)?;
    }
    if let Some(value) = table.get("item_padding_y") {
        theme.metrics.item_padding_y = pixels(value)?;
    }
    if let Some(value) = table.get("item_border_radius") {
        theme.metrics.item_border_radius = pixels(value)?;
    }
    if let Some(value) = table.get("item_title_line_height") {
        theme.metrics.item_title_line_height = pixels(value)?;
    }
    if let Some(value) = table.get("item_content_height") {
        theme.metrics.item_content_height = pixels(value)?;
    }
    if let Some(value) = table.get("icon_size") {
        theme.metrics.icon_size = pixels(value)?;
    }
    if let Some(value) = table.get("icon_border_radius") {
        theme.metrics.icon_border_radius = pixels(value)?;
    }
    if let Some(value) = table.get("empty_state_height") {
        theme.metrics.empty_state_height = pixels(value)?;
    }
    if let Some(value) = table
        .get("layout")
        .and_then(|v| v.get("item_description_height"))
    {
        theme.metrics.layout_item_description_height = pixels(value)?;
    }
    if let Some(value) = table.get("layout").and_then(|v| v.get("separator_width")) {
        theme.metrics.layout_separator_width = pixels(value)?;
    }
    if let Some(value) = table
        .get("section_header")
        .and_then(|v| v.get("margin_top"))
    {
        theme.metrics.section_header_margin_top = pixels(value)?;
    }
    if let Some(value) = table
        .get("section_header")
        .and_then(|v| v.get("margin_bottom"))
    {
        theme.metrics.section_header_margin_bottom = pixels(value)?;
    }
    if let Some(value) = table.get("section_header").and_then(|v| v.get("padding_y")) {
        theme.metrics.section_header_padding_y = pixels(value)?;
    }
    if let Some(value) = table.get("action_indicator").and_then(|v| v.get("width")) {
        theme.metrics.action_indicator_width = pixels(value)?;
    }
    if let Some(value) = table
        .get("action_indicator")
        .and_then(|v| v.get("right_position"))
    {
        theme.metrics.action_indicator_right_position = pixels(value)?;
    }
    if let Some(value) = table
        .get("action_indicator")
        .and_then(|v| v.get("key_padding_x"))
    {
        theme.metrics.action_indicator_key_padding_x = pixels(value)?;
    }
    if let Some(value) = table
        .get("action_indicator")
        .and_then(|v| v.get("key_padding_top"))
    {
        theme.metrics.action_indicator_key_padding_top = pixels(value)?;
    }
    if let Some(value) = table
        .get("action_indicator")
        .and_then(|v| v.get("key_padding_bottom"))
    {
        theme.metrics.action_indicator_key_padding_bottom = pixels(value)?;
    }
    if let Some(value) = table
        .get("action_indicator")
        .and_then(|v| v.get("key_border_radius"))
    {
        theme.metrics.action_indicator_key_border_radius = pixels(value)?;
    }
    if let Some(value) = table
        .get("action_indicator")
        .and_then(|v| v.get("key_font_size"))
    {
        theme.metrics.action_indicator_key_font_size = pixels(value)?;
    }
    if let Some(value) = table
        .get("action_indicator")
        .and_then(|v| v.get("key_line_height"))
    {
        theme.metrics.action_indicator_key_line_height = pixels(value)?;
    }
    if let Some(value) = table.get("emoji").and_then(|v| v.get("cell_size")) {
        theme.metrics.emoji_cell_size = pixels(value)?;
    }
    if let Some(value) = table.get("emoji").and_then(|v| v.get("font_size")) {
        theme.metrics.emoji_font_size = pixels(value)?;
    }
    if let Some(value) = table.get("emoji").and_then(|v| v.get("cell_border_radius")) {
        theme.metrics.emoji_cell_border_radius = pixels(value)?;
    }
    if let Some(value) = table.get("emoji").and_then(|v| v.get("cell_gap")) {
        theme.metrics.emoji_cell_gap = pixels(value)?;
    }
    if let Some(value) = table.get("ai").and_then(|v| v.get("user_bubble_padding_x")) {
        theme.metrics.ai_user_bubble_padding_x = pixels(value)?;
    }
    if let Some(value) = table.get("ai").and_then(|v| v.get("user_bubble_padding_y")) {
        theme.metrics.ai_user_bubble_padding_y = pixels(value)?;
    }
    if let Some(value) = table
        .get("ai")
        .and_then(|v| v.get("user_bubble_border_radius"))
    {
        theme.metrics.ai_user_bubble_border_radius = pixels(value)?;
    }
    if let Some(value) = table.get("ai").and_then(|v| v.get("message_gap")) {
        theme.metrics.ai_message_gap = pixels(value)?;
    }
    if let Some(value) = table
        .get("markdown")
        .and_then(|v| v.get("paragraph_line_height"))
    {
        theme.metrics.markdown_paragraph_line_height = pixels(value)?;
    }
    if let Some(value) = table
        .get("markdown")
        .and_then(|v| v.get("heading_line_height"))
    {
        theme.metrics.markdown_heading_line_height = pixels(value)?;
    }
    if let Some(value) = table
        .get("markdown")
        .and_then(|v| v.get("code_block_radius"))
    {
        theme.metrics.markdown_code_block_radius = pixels(value)?;
    }
    if let Some(value) = table
        .get("markdown")
        .and_then(|v| v.get("code_line_height"))
    {
        theme.metrics.markdown_code_line_height = pixels(value)?;
    }
    if let Some(value) = table
        .get("clipboard")
        .and_then(|v| v.get("color_icon_size"))
    {
        theme.metrics.clipboard_color_icon_size = pixels(value)?;
    }
    if let Some(value) = table
        .get("clipboard")
        .and_then(|v| v.get("preview_padding"))
    {
        theme.metrics.clipboard_preview_padding = pixels(value)?;
    }
    if let Some(value) = table
        .get("clipboard")
        .and_then(|v| v.get("color_swatch_size"))
    {
        theme.metrics.clipboard_color_swatch_size = pixels(value)?;
    }
    if let Some(value) = table
        .get("clipboard")
        .and_then(|v| v.get("color_preview_gap"))
    {
        theme.metrics.clipboard_color_preview_gap = pixels(value)?;
    }
    if let Some(value) = table.get("clipboard").and_then(|v| v.get("color_code_gap")) {
        theme.metrics.clipboard_color_code_gap = pixels(value)?;
    }
    if let Some(value) = table
        .get("clipboard")
        .and_then(|v| v.get("color_label_width"))
    {
        theme.metrics.clipboard_color_label_width = pixels(value)?;
    }
    if let Some(value) = table.get("emoji").and_then(|v| v.get("columns")) {
        theme.metrics.emoji_columns = usize::try_from(value.as_integer()?).ok()?;
    }
    Some(theme)
}

#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct FontConfig {
    pub font_family: Option<String>,
    pub mono_font_family: Option<String>,
    pub font_size: Option<f32>,
}

pub fn theme_fonts(name: &str) -> Option<FontConfig> {
    if name == "default" {
        return Some(FontConfig::default());
    }
    let source = if let Some(index) = THEMES[1..].iter().position(|candidate| *candidate == name) {
        THEME_FILES[index].to_string()
    } else {
        custom_theme_source(name)?
    };
    parse_theme(&source)?;
    let table: toml::Value = toml::from_str(&source).ok()?;
    table.get("font").map_or_else(
        || Some(FontConfig::default()),
        |font| font.clone().try_into().ok(),
    )
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

fn custom_theme_source(name: &str) -> Option<String> {
    // A theme name is a filename stem, never a path outside the themes directory.
    if name.is_empty() || name.contains(['/', '\\']) || name == "." || name == ".." {
        return None;
    }
    read_source(
        Config::path()
            .parent()?
            .join("themes")
            .join(format!("{name}.toml")),
    )
    .ok()
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
    parse_theme(&custom_theme_source(name)?)
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
                        if !themes.iter().any(|(n, _)| n == name) {
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
    pub fn startup_fonts(&self) -> FontConfig {
        let theme = theme_fonts(&self.theme).unwrap_or_default();
        FontConfig {
            font_family: theme.font_family.or_else(|| self.font.font_family.clone()),
            mono_font_family: theme
                .mono_font_family
                .or_else(|| self.font.mono_font_family.clone()),
            font_size: theme
                .font_size
                .or(self.font.font_size)
                .filter(|size| size.is_finite() && *size > 0.0 && *size <= 1024.0),
        }
    }

    pub fn startup_font_size(&self) -> f32 {
        self.startup_fonts().font_size.unwrap_or(14.0)
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
        if !self.theme.is_empty()
            && self.theme != "default"
            && !available_themes()
                .iter()
                .any(|(name, _)| name == &self.theme)
        {
            warnings.push((
                "theme",
                format!(
                    "Theme '{}' not found. Will fall back to default theme.",
                    self.theme
                ),
            ));
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
        let mut th = theme(&self.theme);
        if !self.enable_transparency {
            th.bg_alpha = 1.0;
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
        .write(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_NOFOLLOW)
        .mode(0o600)
        .open(&temp)
    else {
        return false;
    };
    file.metadata().is_ok_and(|metadata| metadata.is_file())
        && file.set_len(0).is_ok()
        && file.write_all(text.as_bytes()).is_ok()
        && std::fs::rename(temp, path).is_ok()
}

/// Update the selected theme and save only when a config file already exists.
pub fn set_theme(cfg: &mut Config, name: &str) -> bool {
    cfg.theme = name.into();
    if Config::path().exists() && !save(cfg) {
        eprintln!("zl: could not save selected theme");
    }
    true
}

#[cfg(test)]
mod save_tests {
    use super::*;
    #[test]
    fn invalid_theme_metrics_and_font_sizes_are_rejected() {
        for value in [
            "nan",
            "inf",
            "-inf",
            "1e100",
            "1e30",
            "8193",
            "'Pixels(inf)'",
        ] {
            assert!(
                parse_theme(&format!("icon_size = {value}")).is_none(),
                "{value}"
            );
        }
        for value in ["nan", "inf", "-inf", "0", "-1", "1025", "1e100"] {
            assert!(
                parse_theme(&format!("[font]\nfont_size = {value}")).is_none(),
                "{value}"
            );
            let config: Config = toml::from_str(&format!("[font]\nfont_size = {value}")).unwrap();
            assert_eq!(config.startup_font_size(), 14.0, "{value}");
        }
        for value in ["nan", "inf", "0", "-1", "1025", "1e30"] {
            assert!(
                parse_theme(&format!("[emoji]\nfont_size = {value}")).is_none(),
                "{value}"
            );
            assert!(
                parse_theme(&format!("[action_indicator]\nkey_font_size = {value}")).is_none(),
                "{value}"
            );
        }
        assert_eq!(pixels(&toml::Value::Float(-2.0)), Some(-2.0));
        assert!(parse_theme("[font]\nfont_size = 1024").is_some());
    }

    #[test]
    fn markdown_code_radius_uses_the_theme_table() {
        assert_eq!(
            parse_theme("[markdown]\ncode_block_radius = 11.0")
                .unwrap()
                .markdown_code_radius,
            11.0
        );
        assert_eq!(
            parse_theme("[markdown]\ncode_block_radius = 4")
                .unwrap()
                .markdown_code_radius,
            4.0
        );
        assert!(parse_theme("[markdown]\ncode_block_radius = false").is_none());
    }
    #[test]
    fn saved_theme_survives_reload() {
        let _lock = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let old = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        let mut cfg = Config::default();
        assert!(set_theme(&mut cfg, "nord"));
        assert_eq!(cfg.theme, "nord");
        assert!(!Config::path().exists());
        std::fs::create_dir_all(Config::path().parent().unwrap()).unwrap();
        std::fs::write(Config::path(), "theme = 'default'").unwrap();
        assert!(set_theme(&mut cfg, "nord"));
        assert_eq!(Config::load().theme, "nord");
        let file = Config::path();
        std::fs::remove_file(&file).unwrap();
        std::fs::remove_dir(file.parent().unwrap()).unwrap();
        std::fs::write(file.parent().unwrap(), b"blocked").unwrap();
        assert!(set_theme(&mut cfg, "dracula"));
        assert_eq!(cfg.theme, "dracula");
        // An existing directory at the file path forces a write error, not the missing-file case.
        std::fs::remove_file(file.parent().unwrap()).unwrap();
        std::fs::create_dir_all(&file).unwrap();
        assert!(set_theme(&mut cfg, "monokai"));
        assert_eq!(cfg.theme, "monokai");
        assert!(file.is_dir());
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
        assert_eq!(c.max_rows, 6);
        assert!(c.disabled_apps.is_empty());
        assert_eq!(c.theme().accent, theme("nord").accent);
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
        std::fs::write(themes.join("fonts.toml"), "[font]\nfont_family = 'DejaVu Sans'\nmono_font_family = 'DejaVu Sans Mono'\nfont_size = 19.5").unwrap();
        assert_eq!(
            theme_fonts("fonts"),
            Some(FontConfig {
                font_family: Some("DejaVu Sans".into()),
                mono_font_family: Some("DejaVu Sans Mono".into()),
                font_size: Some(19.5)
            })
        );
        assert_eq!(theme_fonts("empty"), Some(FontConfig::default()));
        assert_eq!(theme_fonts("default"), Some(FontConfig::default()));
        assert_eq!(theme_fonts("broken"), None);
        assert_eq!(theme_fonts("../escape"), None);
        for name in THEMES {
            assert!(theme_fonts(name).is_some(), "{name}");
        }
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
        assert!(listed.iter().any(|(name, _)| name == "broken"));
        assert!(!crate::items::theme_items(&Config::default(), "")
            .iter()
            .any(|item| item.title == "broken"));
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
        assert_eq!(crate::items::theme_items(&cfg, ".toml").len(), 3);
        std::fs::write(Config::path(), "theme = 'default'").unwrap();
        assert!(set_theme(&mut cfg.clone(), "custom"));
        assert_eq!(Config::load().theme(), custom);
        match old {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    fn full_theme_schema_checks_even_non_rendered_fields() {
        for source in [
            "item_padding_x = false",
            "name = 5",
            "icon_size = []",
            "[emoji]\ncolumns = -1",
            "[emoji]\nfont_size = false",
            "[font]\nfont_family = 3",
            "font = false",
            "[ai]\nuser_bubble_text_color = false",
            "[markdown]\ncode_block_radius = false",
            "[clipboard]\npreview_padding = []",
            "[layout]\nseparator_width = 'bad'",
            "[calculator]\nerror_color = false",
            "[action_indicator]\nwidth = false",
        ] {
            assert!(parse_theme(source).is_none(), "{source}");
        }
        assert!(
            parse_theme("unknown = true\n[font]\nfont_family = 'Sans'\nfont_size = 15").is_some()
        );
        let parsed = parse_theme("item_padding_x = 'Pixels(13.0)'\nicon_size = 32\nempty_state_height = 240\n[layout]\nitem_description_height = 21").unwrap();
        assert_eq!(parsed.metrics.item_padding_x, 13.0);
        assert_eq!(parsed.metrics.icon_size, 32.0);
        assert_eq!(parsed.metrics.empty_state_height, 240.0);
        assert_eq!(parsed.metrics.layout_item_description_height, 21.0);
    }

    #[test]
    fn dimension_and_theme_warnings_are_nonfatal() {
        let _lock = crate::testutil::ENV_LOCK.lock().unwrap();
        assert!(Config::default().validation_warnings().is_empty());
        let mut cfg = Config {
            launcher_size: Some((250.0, 1600.0)),
            window_size: Some((200.0, 1200.0)),
            theme: "no-such-theme".into(),
            ..Config::default()
        };
        assert_eq!(cfg.validation_warnings(), vec![
            ("launcher_size", "Width 250 is below minimum (300). Consider increasing for usability.".into()),
            ("launcher_size", "Height 1600 exceeds maximum (1500). This may cause display issues.".into()),
            ("theme", "Theme 'no-such-theme' not found. Will fall back to default theme.".into()),
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
        cfg.theme = "default".into();
        assert!(cfg.validation_warnings().is_empty());
        cfg.launcher_size = Some((2000.0, 200.0));
        assert!(cfg.validation_warnings().is_empty());
    }

    #[test]
    fn startup_fonts_merge_theme_over_global_and_serialize_as_a_table() {
        let _lock = crate::testutil::ENV_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        let old = std::env::var_os("XDG_CONFIG_HOME");
        std::env::set_var("XDG_CONFIG_HOME", dir.path());
        let themes = Config::path().parent().unwrap().join("themes");
        std::fs::create_dir_all(&themes).unwrap();
        std::fs::write(
            themes.join("fonts.toml"),
            "[font]\nfont_family = 'DejaVu Serif'\nfont_size = 19.5",
        )
        .unwrap();
        let mut cfg: Config = toml::from_str("theme = 'fonts'\nfont_size = 99\n[font]\nfont_family = 'DejaVu Sans'\nmono_font_family = 'DejaVu Sans Mono'\nfont_size = 15.0").unwrap();
        assert_eq!(
            cfg.startup_fonts(),
            FontConfig {
                font_family: Some("DejaVu Serif".into()),
                mono_font_family: Some("DejaVu Sans Mono".into()),
                font_size: Some(19.5)
            }
        );
        assert_eq!(cfg.startup_font_size(), 19.5);
        assert!(std::path::Path::new(&cfg.font_file(false).unwrap()).is_file());
        assert!(std::path::Path::new(&cfg.font_file(true).unwrap()).is_file());
        cfg.font_path = "/not/serialized.ttf".into();
        let serialized = toml::to_string(&cfg).unwrap();
        let table: toml::Value = toml::from_str(&serialized).unwrap();
        assert!(table.get("font_size").is_none());
        assert!(table.get("font_path").is_none());
        assert_eq!(table["font"]["font_size"].as_float(), Some(15.0));
        assert_eq!(
            toml::from_str::<Config>(&serialized).unwrap().font,
            cfg.font
        );
        cfg.theme = "missing".into();
        assert_eq!(cfg.startup_fonts(), cfg.font);
        assert_eq!(Config::default().startup_font_size(), 14.0);
        assert!(toml::from_str::<Config>("font = 'old-path.ttf'").is_err());
        match old {
            Some(value) => std::env::set_var("XDG_CONFIG_HOME", value),
            None => std::env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    #[test]
    fn component_metrics_retain_theme_values() {
        let parsed = parse_theme("[action_indicator]\nwidth = 20\nright_position = 21\nkey_padding_x = 22\nkey_padding_top = 23\nkey_padding_bottom = 24\nkey_border_radius = 25\nkey_font_size = 26\nkey_line_height = 27\n[emoji]\ncell_size = 28\nfont_size = 29\ncell_border_radius = 30\ncell_gap = 31\ncolumns = 3\n[ai]\nuser_bubble_padding_x = 32\nuser_bubble_padding_y = 33\nuser_bubble_border_radius = 34\nmessage_gap = 35\n[markdown]\nparagraph_line_height = 36\nheading_line_height = 37\ncode_block_radius = 38\ncode_line_height = 39\n[clipboard]\ncolor_icon_size = 40\npreview_padding = 41\ncolor_swatch_size = 42\ncolor_preview_gap = 43\ncolor_code_gap = 44\ncolor_label_width = 45\n").unwrap();
        assert_eq!(parsed.metrics.action_indicator_width, 20.0);
        assert_eq!(parsed.metrics.action_indicator_right_position, 21.0);
        assert_eq!(parsed.metrics.action_indicator_key_padding_x, 22.0);
        assert_eq!(parsed.metrics.action_indicator_key_padding_top, 23.0);
        assert_eq!(parsed.metrics.action_indicator_key_padding_bottom, 24.0);
        assert_eq!(parsed.metrics.action_indicator_key_border_radius, 25.0);
        assert_eq!(parsed.metrics.action_indicator_key_font_size, 26.0);
        assert_eq!(parsed.metrics.action_indicator_key_line_height, 27.0);
        assert_eq!(parsed.metrics.emoji_cell_size, 28.0);
        assert_eq!(parsed.metrics.emoji_font_size, 29.0);
        assert_eq!(parsed.metrics.emoji_cell_border_radius, 30.0);
        assert_eq!(parsed.metrics.emoji_cell_gap, 31.0);
        assert_eq!(parsed.metrics.ai_user_bubble_padding_x, 32.0);
        assert_eq!(parsed.metrics.ai_user_bubble_padding_y, 33.0);
        assert_eq!(parsed.metrics.ai_user_bubble_border_radius, 34.0);
        assert_eq!(parsed.metrics.ai_message_gap, 35.0);
        assert_eq!(parsed.metrics.markdown_paragraph_line_height, 36.0);
        assert_eq!(parsed.metrics.markdown_heading_line_height, 37.0);
        assert_eq!(parsed.metrics.markdown_code_block_radius, 38.0);
        assert_eq!(parsed.metrics.markdown_code_line_height, 39.0);
        assert_eq!(parsed.metrics.clipboard_color_icon_size, 40.0);
        assert_eq!(parsed.metrics.clipboard_preview_padding, 41.0);
        assert_eq!(parsed.metrics.clipboard_color_swatch_size, 42.0);
        assert_eq!(parsed.metrics.clipboard_color_preview_gap, 43.0);
        assert_eq!(parsed.metrics.clipboard_color_code_gap, 44.0);
        assert_eq!(parsed.metrics.clipboard_color_label_width, 45.0);
        assert_eq!(parsed.metrics.emoji_columns, 3);
        assert_eq!(parse_theme("").unwrap().metrics, ThemeMetrics::default());
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
        assert_eq!(
            parse_theme("window_background = '##abc'").unwrap().bg,
            [170, 187, 204]
        );
        assert!(parse_theme("window_background = ' #abc '").is_none());
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
