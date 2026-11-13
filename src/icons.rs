//! Icon lookup and decoding. Icons load only when a row is visible and stay in a small cache.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Premultiplied RGBA pixels, `size` x `size`.
pub fn data_dirs() -> Vec<PathBuf> {
    let home = std::env::var("XDG_DATA_HOME")
        .unwrap_or_else(|_| format!("{}/.local/share", std::env::var("HOME").unwrap_or_default()));
    let mut v = vec![PathBuf::from(&home)];
    v.extend(
        std::env::var("XDG_DATA_DIRS")
            .unwrap_or("/usr/local/share:/usr/share".into())
            .split(':')
            .filter(|d| !d.is_empty())
            .map(PathBuf::from),
    );
    // NixOS keeps system and user packages here. A daemon started with a short
    // XDG_DATA_DIRS (a compositor autostart entry, a bare service) would otherwise
    // miss every icon and desktop file that lives only in these profiles.
    let user = std::env::var("USER").unwrap_or_default();
    let mut nix = vec![
        PathBuf::from(&home).join("../.nix-profile/share"),
        PathBuf::from("/run/current-system/sw/share"),
        PathBuf::from("/nix/var/nix/profiles/default/share"),
    ];
    if let Ok(profile_home) = std::env::var("HOME") {
        nix.push(PathBuf::from(profile_home).join(".nix-profile/share"));
    }
    if !user.is_empty() {
        nix.push(PathBuf::from(format!(
            "/etc/profiles/per-user/{user}/share"
        )));
    }
    for dir in nix {
        let dir = dir.components().collect::<PathBuf>();
        if dir.parent().is_some() && !v.contains(&dir) && dir.is_dir() {
            v.push(dir);
        }
    }
    v
}

/// Last-resort icon search for names the theme index misses: `pixmaps` in every
/// data dir (the reference lookup only reads /usr/share/pixmaps, which NixOS lacks)
/// and the `apps` folders of every installed theme, whatever its index says.
fn scan_data_dirs(name: &str) -> Option<PathBuf> {
    scan_dirs(name, &data_dirs())
}

fn scan_dirs(name: &str, dirs: &[PathBuf]) -> Option<PathBuf> {
    let file = |dir: &Path| -> Option<PathBuf> {
        ["svg", "png"]
            .iter()
            .map(|ext| dir.join(format!("{name}.{ext}")))
            .find(|path| path.is_file())
    };
    for base in dirs {
        if let Some(path) = file(&base.join("pixmaps")) {
            return Some(path);
        }
    }
    let mut best: Option<(u32, PathBuf)> = None;
    for base in dirs {
        let mut roots = vec![base.join("icons")];
        if base.ends_with(".local/share") {
            roots.push(base.join("../.icons"));
        }
        for root in roots {
            let Ok(themes) = std::fs::read_dir(&root) else {
                continue;
            };
            for theme in themes.flatten() {
                let Ok(inner) = std::fs::read_dir(theme.path()) else {
                    continue;
                };
                for first in inner.flatten() {
                    // `48x48/apps`, `scalable/apps`, and Breeze's `apps/48`.
                    let first_name = first.file_name().to_string_lossy().into_owned();
                    let mut candidates = vec![first.path().join("apps")];
                    if first_name == "apps" {
                        candidates = std::fs::read_dir(first.path())
                            .into_iter()
                            .flatten()
                            .flatten()
                            .map(|e| e.path())
                            .collect();
                    }
                    for dir in candidates {
                        if let Some(path) = file(&dir) {
                            let size = dir
                                .parent()
                                .and_then(|p| p.file_name())
                                .or_else(|| dir.file_name())
                                .and_then(|n| n.to_str())
                                .and_then(|n| n.split('x').next()?.parse::<u32>().ok())
                                .unwrap_or(if path.extension().is_some_and(|e| e == "svg") {
                                    4096
                                } else {
                                    0
                                });
                            let score = if path.extension().is_some_and(|e| e == "svg") {
                                10_000
                            } else {
                                size.min(256)
                            };
                            if best.as_ref().is_none_or(|(s, _)| score > *s) {
                                best = Some((score, path));
                            }
                        }
                    }
                }
            }
        }
    }
    best.map(|(_, path)| path)
}

/// Icon named by the desktop entry a window class belongs to: `<class>.desktop`
/// or an entry whose StartupWMClass equals the class.
fn desktop_entry_icon(app_id: &str) -> Option<String> {
    desktop_entry_icon_in(app_id, &data_dirs())
}

fn desktop_entry_icon_in(app_id: &str, dirs: &[PathBuf]) -> Option<String> {
    let lower = app_id.to_lowercase();
    for base in dirs {
        let Ok(files) = std::fs::read_dir(base.join("applications")) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().is_none_or(|e| e != "desktop") {
                continue;
            }
            let stem = path
                .file_stem()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let mut icon = None;
            let mut class_matches = false;
            for line in text.lines() {
                if let Some(v) = line.strip_prefix("Icon=") {
                    icon.get_or_insert_with(|| v.trim().to_string());
                } else if let Some(v) = line.strip_prefix("StartupWMClass=") {
                    class_matches = v.trim().to_lowercase() == lower;
                }
                if line.starts_with('[') && !line.starts_with("[Desktop Entry]") {
                    break;
                }
            }
            if (stem == lower || class_matches) && icon.as_ref().is_some_and(|i| !i.is_empty()) {
                return icon;
            }
        }
    }
    None
}

pub struct Icon {
    pub size: u32,
    pub px: Vec<u8>,
    /// Stroke-only builtin symbol: draw tinted, not as-is.
    pub symbol: bool,
}

pub const SEARCH: &str = "builtin:search";
pub const CLEAR: &str = "builtin:clear";
pub const BACK: &str = "builtin:back";
pub const DASHBOARD: &str = "builtin:dashboard";
pub const MODE_ACTIONS: &str = "builtin:mode-actions";
pub const MODE_SEARCH: &str = "builtin:mode-search";
pub const PALETTE: &str = "builtin:palette";

const BACK_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m12 19-7-7 7-7"/><path d="M19 12H5"/></svg>"#;
const PALETTE_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="9"/><circle cx="8.5" cy="10" r="1.2"/><circle cx="12" cy="7.5" r="1.2"/><circle cx="15.5" cy="10" r="1.2"/><circle cx="9" cy="14.5" r="1.2"/></svg>"#;

// Lucide circle-x (ISC, see NOTICE): the glyph zlaunch's clear button shows.
const CLEAR_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="m15 9-6 6"/><path d="m9 9 6 6"/></svg>"#;

pub fn phosphor(name: &str) -> Option<&'static [u8]> {
    Some(match name {
        "power" => include_bytes!("../assets/icons/phosphor/power.svg"),
        "reboot" => include_bytes!("../assets/icons/phosphor/reboot.svg"),
        "moon" => include_bytes!("../assets/icons/phosphor/moon.svg"),
        "lock" => include_bytes!("../assets/icons/phosphor/lock.svg"),
        "sign-out" => include_bytes!("../assets/icons/phosphor/sign-out.svg"),
        "smiley" => include_bytes!("../assets/icons/phosphor/smiley.svg"),
        "terminal" => include_bytes!("../assets/icons/phosphor/terminal.svg"),
        "clipboard" => include_bytes!("../assets/icons/phosphor/clipboard-bold.svg"),
        "clipboard-text" => include_bytes!("../assets/icons/phosphor/clipboard-text-bold.svg"),
        "file" => include_bytes!("../assets/icons/phosphor/file-bold.svg"),
        "file-text" => include_bytes!("../assets/icons/phosphor/file-text-bold.svg"),
        "file-image" => include_bytes!("../assets/icons/phosphor/file-image-bold.svg"),
        "image" => include_bytes!("../assets/icons/phosphor/image-bold.svg"),
        "magnifying-glass" => include_bytes!("../assets/icons/phosphor/magnifying-glass-bold.svg"),
        "globe" => include_bytes!("../assets/icons/phosphor/globe-bold.svg"),
        "book-open" => include_bytes!("../assets/icons/phosphor/book-open-bold.svg"),
        "youtube-logo" => include_bytes!("../assets/icons/phosphor/youtube-logo-bold.svg"),
        "brain" => include_bytes!("../assets/icons/phosphor/brain-bold.svg"),
        "palette" => include_bytes!("../assets/icons/phosphor/palette-bold.svg"),
        _ => return None,
    })
}

fn read_theme_setting(path: impl AsRef<Path>) -> std::io::Result<String> {
    String::from_utf8(crate::state_io::read(path, 1024 * 1024)?)
        .map_err(|error| std::io::Error::new(std::io::ErrorKind::InvalidData, error))
}

fn desktop_theme(config: &Path) -> Option<String> {
    if let Ok(source) = read_theme_setting(config.join("kdeglobals")) {
        let mut icons = false;
        for line in source.lines().map(str::trim) {
            if line.starts_with('[') {
                icons = line == "[Icons]";
                continue;
            }
            if icons {
                if let Some(theme) = line.strip_prefix("Theme=") {
                    return Some(theme.into());
                }
            }
        }
    }
    for file in ["gtk-3.0/settings.ini", "gtk-4.0/settings.ini"] {
        if let Ok(source) = read_theme_setting(config.join(file)) {
            for line in source.lines().map(str::trim) {
                if let Some(theme) = line.strip_prefix("gtk-icon-theme-name=") {
                    return Some(theme.into());
                }
            }
        }
    }
    None
}

/// KDE, GTK3, then GTK4 settings choose the theme; hicolor is the fallback.
pub fn theme_chain(_theme: &str) -> Vec<String> {
    static THEME: std::sync::OnceLock<Option<String>> = std::sync::OnceLock::new();
    let theme = THEME.get_or_init(|| {
        let root = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
        desktop_theme(&root)
    });
    theme
        .iter()
        .cloned()
        .chain(std::iter::once("hicolor".into()))
        .collect()
}

/// Resolve at 64 px through the reference theme library, including inherited themes.
pub fn find(name: &str, themes: &[String]) -> Option<PathBuf> {
    if name.starts_with('/') {
        return Some(PathBuf::from(name)).filter(|path| path.exists());
    }
    for theme in themes {
        if let Some(path) = freedesktop_icons::lookup(name)
            .with_size(64)
            .with_theme(theme)
            .find()
        {
            return Some(path);
        }
    }
    freedesktop_icons::lookup(name)
        .with_size(64)
        .find()
        .or_else(|| scan_data_dirs(name))
}

pub fn find_window(name: &str, themes: &[String]) -> Option<PathBuf> {
    for candidate in [
        name.to_string(),
        name.to_lowercase(),
        name.rsplit('.').next().unwrap_or(name).to_string(),
        name.rsplit('.').next().unwrap_or(name).to_lowercase(),
    ] {
        if let Some(path) = find(&candidate, themes) {
            return Some(path);
        }
    }
    // Windows of apps whose class is not an icon name (Electron apps, KDE's
    // org.kde.* classes): use the icon of the matching desktop entry.
    desktop_entry_icon(name).and_then(|icon| find(&icon, themes))
}

/// Decode a PNG or SVG file to `size` x `size` premultiplied pixels.
pub fn load(path: &Path, size: u32) -> Option<Icon> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    let data = crate::state_io::read(path, 8 * 1024 * 1024).ok()?;
    if data.len() > 8 * 1024 * 1024 {
        return None;
    }
    let px = match ext.as_str() {
        "png" => decode_png(&data, size)?,
        "svg" => render_svg(&data, size)?,
        _ => return None,
    };
    Some(Icon {
        size,
        px,
        symbol: false,
    })
}

pub(crate) fn decode_png(data: &[u8], size: u32) -> Option<Vec<u8>> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(data));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut r = dec.read_info().ok()?;
    if r.info().width == 0
        || r.info().height == 0
        || r.info().width > 4096
        || r.info().height > 4096
    {
        return None;
    }
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).ok()?;
    let (w, h) = (info.width as usize, info.height as usize);
    if w == 0 || h == 0 || w > 4096 || h > 4096 {
        return None;
    }
    let rgba: Vec<u8> = match info.color_type {
        png::ColorType::Rgba => buf[..w * h * 4].to_vec(),
        png::ColorType::Rgb => buf[..w * h * 3]
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf[..w * h * 2]
            .chunks(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buf[..w * h].iter().flat_map(|&g| [g, g, g, 255]).collect(),
        _ => return None,
    };
    Some(resize(&premultiply(&rgba), w, h, size as usize))
}

fn premultiply(rgba: &[u8]) -> Vec<u8> {
    rgba.chunks(4)
        .flat_map(|p| {
            let a = p[3] as u32;
            [
                (p[0] as u32 * a / 255) as u8,
                (p[1] as u32 * a / 255) as u8,
                (p[2] as u32 * a / 255) as u8,
                p[3],
            ]
        })
        .collect()
}

/// Area-average resize of a square-fit image into `d` x `d`, keeping the aspect ratio and centring.
pub fn resize(src: &[u8], w: usize, h: usize, d: usize) -> Vec<u8> {
    let mut out = vec![0u8; d * d * 4];
    let scale = (d as f32 / w as f32).min(d as f32 / h as f32);
    let (tw, th) = (
        ((w as f32 * scale).round() as usize).max(1),
        ((h as f32 * scale).round() as usize).max(1),
    );
    let (ox, oy) = ((d - tw) / 2, (d - th) / 2);
    for y in 0..th {
        let (sy0, sy1) = (y * h / th, ((y + 1) * h / th).max(y * h / th + 1).min(h));
        for x in 0..tw {
            let (sx0, sx1) = (x * w / tw, ((x + 1) * w / tw).max(x * w / tw + 1).min(w));
            let mut acc = [0u32; 4];
            let mut n = 0u32;
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let o = (sy * w + sx) * 4;
                    for k in 0..4 {
                        acc[k] += src[o + k] as u32;
                    }
                    n += 1;
                }
            }
            let o = ((y + oy) * d + x + ox) * 4;
            for k in 0..4 {
                out[o + k] = (acc[k] / n.max(1)) as u8;
            }
        }
    }
    out
}

fn render_svg(data: &[u8], size: u32) -> Option<Vec<u8>> {
    use resvg::{tiny_skia, usvg};
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_data(data, &opt).ok()?;
    let s = tree.size();
    let k = (size as f32 / s.width()).min(size as f32 / s.height());
    let mut pm = tiny_skia::Pixmap::new(size, size)?;
    let (ox, oy) = (
        (size as f32 - s.width() * k) / 2.0,
        (size as f32 - s.height() * k) / 2.0,
    );
    resvg::render(
        &tree,
        tiny_skia::Transform::from_row(k, 0.0, 0.0, k, ox, oy),
        &mut pm.as_mut(),
    );
    Some(pm.take())
}

/// Map a font file read-only. Only the pages that get used stay in memory.
fn map_file(p: &str) -> Option<memmap2::Mmap> {
    let f = std::fs::File::open(p).ok()?;
    unsafe { memmap2::Mmap::map(&f).ok() }
}

/// The PNG bytes of one emoji from a font with CBDT or sbix bitmaps. Sequences (flags, ZWJ) are not looked up.
pub fn emoji_png(font: &[u8], glyph: &str) -> Option<Vec<u8>> {
    let face = ttf_parser::Face::parse(font, 0).ok()?;
    let ch = glyph.chars().find(|&c| c != '\u{fe0f}')?;
    if glyph.chars().filter(|&c| c != '\u{fe0f}').count() != 1 {
        return None;
    }
    let gid = face.glyph_index(ch)?;
    let img = face.glyph_raster_image(gid, 128)?;
    if img.format != ttf_parser::RasterImageFormat::PNG {
        return None;
    }
    Some(img.data.to_vec())
}

/// Like emoji_png, but multi-char sequences (flags, skin tones, ZWJ chains) are first
/// resolved through the font's GSUB ligature table, and only when the font cannot compose
/// them fall back to the bitmap of their first base char.
pub fn emoji_png_lenient(font: &[u8], glyph: &str) -> Option<Vec<u8>> {
    emoji_png(font, glyph)
        .or_else(|| emoji_png_sequence(font, glyph))
        .or_else(|| {
            // Flags would otherwise draw the first regional indicator's letter bitmap; the
            // two-letter badge the picker falls back to reads better than half a country code.
            if glyph
                .chars()
                .all(|c| ('\u{1f1e6}'..='\u{1f1ff}').contains(&c))
            {
                return None;
            }
            let first = glyph.chars().find(|&c| c != '\u{fe0f}')?;
            emoji_png(font, &first.to_string())
        })
}

/// The PNG bytes of a multi-char emoji sequence composed by the font itself. None when the
/// font has no ligature for it.
fn emoji_png_sequence(font: &[u8], glyph: &str) -> Option<Vec<u8>> {
    if glyph.chars().filter(|&c| c != '\u{fe0f}').count() < 2 {
        return None;
    }
    let face = ttf_parser::Face::parse(font, 0).ok()?;
    let gid = sequence_glyph(&face, glyph)?;
    let img = face.glyph_raster_image(gid, 128)?;
    if img.format != ttf_parser::RasterImageFormat::PNG {
        return None;
    }
    Some(img.data.to_vec())
}

/// Map every char to a glyph and fold the sequence with the font's GSUB ligature rules
/// (greedy, longest first match), the way a text stack composes emoji. Some only when the
/// whole sequence collapses into one glyph.
fn sequence_glyph(face: &ttf_parser::Face, glyph: &str) -> Option<ttf_parser::GlyphId> {
    let mut ids: Vec<ttf_parser::GlyphId> = Vec::new();
    for c in glyph.chars() {
        if c == '\u{fe0f}' {
            continue;
        }
        ids.push(face.glyph_index(c)?);
    }
    if ids.len() < 2 {
        return ids.first().copied();
    }
    let gsub = face.tables().gsub?;
    'apply: while ids.len() > 1 {
        for i in 0..ids.len() {
            for l in 0..gsub.lookups.len() {
                let Some(lookup) = gsub.lookups.get(l) else {
                    continue;
                };
                for s in 0..lookup.subtables.len() {
                    let Some(ttf_parser::gsub::SubstitutionSubtable::Ligature(ls)) = lookup
                        .subtables
                        .get::<ttf_parser::gsub::SubstitutionSubtable>(s)
                    else {
                        continue;
                    };
                    let Some(cov) = ls.coverage.get(ids[i]) else {
                        continue;
                    };
                    let Some(set) = ls.ligature_sets.get(cov) else {
                        continue;
                    };
                    for n in 0..set.len() {
                        let Some(lig) = set.get(n) else {
                            continue;
                        };
                        let comps: Vec<ttf_parser::GlyphId> = lig.components.into_iter().collect();
                        if !comps.is_empty()
                            && ids.len() - i > comps.len()
                            && ids[i + 1..i + 1 + comps.len()] == comps[..]
                        {
                            ids.splice(i..=i + comps.len(), [lig.glyph]);
                            continue 'apply;
                        }
                    }
                }
            }
        }
        // No rule matched anywhere: the font cannot compose this sequence.
        return None;
    }
    ids.first().copied()
}

/// Small bounded cache. A miss is remembered too, so a missing icon is looked up once.
pub struct Cache {
    map: HashMap<(String, u32), Option<Icon>>,
    themes: Vec<String>,
    emoji_font: Option<Option<memmap2::Mmap>>,
    /// PNG bytes supplied by the compositor for open windows (KWin), keyed by
    /// window address; used by "window-data:" icon names.
    window_pngs: HashMap<String, Vec<u8>>,
}

impl Cache {
    pub fn new(theme: &str, emoji_font: &str) -> Self {
        Cache {
            map: HashMap::new(),
            themes: theme_chain(theme),
            emoji_font: if emoji_font.is_empty() {
                None
            } else {
                Some(map_file(emoji_font))
            },
            window_pngs: HashMap::new(),
        }
    }
    /// Register compositor-provided PNG icon pixels for a window address.
    pub fn set_window_icon(&mut self, address: &str, png: Vec<u8>) {
        if self.window_pngs.len() >= 64 {
            self.window_pngs.clear();
        }
        self.window_pngs.insert(address.to_string(), png);
    }
    pub fn ensure(&mut self, name: &str, size: u32) {
        // Raster icons are limited to 4 MiB each, including theme and scale overrides.
        if size == 0 || size > 1024 {
            return;
        }
        let key = (name.to_string(), size);
        if self.map.contains_key(&key) {
            return;
        }
        if self.map.len() >= 256 {
            self.map.clear();
        }
        let symbol: Option<&[u8]> = match name {
            SEARCH => Some(include_bytes!("../assets/icons/search.svg").as_slice()),
            CLEAR => Some(CLEAR_SVG),
            BACK => Some(BACK_SVG),
            PALETTE => Some(PALETTE_SVG),
            "power" => Some(include_bytes!("../assets/icons/power.svg").as_slice()),
            "reboot" => Some(include_bytes!("../assets/icons/reboot.svg").as_slice()),
            "moon" => Some(include_bytes!("../assets/icons/moon.svg").as_slice()),
            "lock" => Some(include_bytes!("../assets/icons/lock.svg").as_slice()),
            "sign-out" => Some(include_bytes!("../assets/icons/sign-out.svg").as_slice()),
            DASHBOARD => Some(include_bytes!("../assets/icons/layout-dashboard.svg")),
            MODE_ACTIONS => Some(include_bytes!("../assets/icons/settings.svg")),
            MODE_SEARCH => Some(include_bytes!("../assets/icons/globe.svg")),
            "phosphor:copy" => Some(include_bytes!("../assets/icons/copy.svg").as_slice()),
            "phosphor:brain" => Some(include_bytes!("../assets/icons/brain.svg").as_slice()),
            _ => None,
        };
        let icon = if symbol.is_none() && name.starts_with("phosphor:") {
            let name = name.strip_prefix("phosphor:").unwrap();
            phosphor(name)
                .and_then(|svg| render_svg(svg, size * 2 / 3))
                .map(|px| {
                    let inner = size * 2 / 3;
                    let mut padded = vec![0; (size * size * 4) as usize];
                    let offset = (size - inner) / 2;
                    for y in 0..inner {
                        let src = (y * inner * 4) as usize;
                        let dst = (((y + offset) * size + offset) * 4) as usize;
                        padded[dst..dst + (inner * 4) as usize]
                            .copy_from_slice(&px[src..src + (inner * 4) as usize]);
                    }
                    Icon {
                        size,
                        px: padded,
                        symbol: true,
                    }
                })
        } else if let Some(svg) = symbol {
            render_svg(svg, size).map(|px| Icon {
                size,
                px,
                symbol: true,
            })
        } else if let Some(g) = name.strip_prefix("emoji:") {
            self.emoji(g, size)
        } else if let Some(address) = name.strip_prefix("window-data:") {
            self.window_pngs
                .get(address)
                .and_then(|png| decode_png(png, size))
                .map(|px| Icon {
                    size,
                    px,
                    symbol: false,
                })
        } else {
            (if let Some(app_id) = name.strip_prefix("window:") {
                find_window(app_id, &self.themes)
            } else {
                find(name, &self.themes)
            })
            .and_then(|p| load(&p, size))
        };
        self.map.insert(key, icon);
    }
    pub fn ensure_input(&mut self, scale: u32) {
        if self.map.len() > 94
            && (self.get(SEARCH, 16 * scale).is_none() || self.get(CLEAR, 16 * scale).is_none())
        {
            self.map.clear();
        }
        self.ensure(SEARCH, 16 * scale);
        self.ensure(CLEAR, 16 * scale);
        self.ensure(BACK, 16 * scale);
        self.ensure(PALETTE, 16 * scale);
        self.ensure(DASHBOARD, 16 * scale);
        self.ensure(MODE_ACTIONS, 16 * scale);
        self.ensure(MODE_SEARCH, 16 * scale);
    }
    /// A colour emoji from the system emoji font, when that font stores PNG bitmaps (Noto Color Emoji, Apple).
    fn emoji(&mut self, glyph: &str, size: u32) -> Option<Icon> {
        if self.emoji_font.is_none() {
            // EMOJI_FONT overrides the system emoji family.
            let fam = std::env::var("EMOJI_FONT").unwrap_or_else(|_| "emoji".into());
            let path = std::process::Command::new("fc-match")
                .args(["-f", "%{file}", &fam])
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
            self.emoji_font = Some(path.and_then(|p| map_file(&p)));
        }
        let data = self.emoji_font.as_ref()?.as_ref()?;
        let png = emoji_png_lenient(&data[..], glyph)?;
        Some(Icon {
            size,
            px: decode_png(&png, size)?,
            symbol: false,
        })
    }

    pub fn get(&self, name: &str, size: u32) -> Option<&Icon> {
        self.map.get(&(name.to_string(), size))?.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clipboard_header_copy_uses_the_component_svg_at_requested_size() {
        let mut cache = Cache::new("", "");
        for size in [16, 32, 48] {
            cache.ensure("phosphor:copy", size);
            let icon = cache.get("phosphor:copy", size).unwrap();
            assert!(icon.symbol);
            assert_eq!(
                icon.px,
                render_svg(include_bytes!("../assets/icons/copy.svg"), size).unwrap()
            );
        }
    }

    #[test]
    fn phosphor_assets_render_in_sixteen_pixel_glyph_boxes() {
        for name in [
            "smiley",
            "clipboard",
            "palette",
            "power",
            "globe",
            "file-text",
        ] {
            let source = phosphor(name).unwrap();
            let expected = render_svg(source, 16).unwrap();
            let mut cache = Cache::new("", "");
            let key = format!("phosphor:{name}");
            cache.ensure(&key, 24);
            let icon = cache.get(&key, 24).unwrap();
            assert!(icon.symbol);
            for y in 0..24usize {
                for x in 0..24usize {
                    let got = &icon.px[(y * 24 + x) * 4..(y * 24 + x + 1) * 4];
                    if (4..20).contains(&x) && (4..20).contains(&y) {
                        let p = ((y - 4) * 16 + x - 4) * 4;
                        assert_eq!(got, &expected[p..p + 4], "{name} {x},{y}");
                    } else {
                        assert_eq!(got, [0, 0, 0, 0], "{name} {x},{y}");
                    }
                }
            }
        }
        assert!(phosphor("not-an-icon").is_none());
    }

    #[test]
    fn input_symbols_are_embedded_and_cached_at_each_scale() {
        let mut cache = Cache::new("", "");
        for scale in [1, 2, 3] {
            cache.ensure_input(scale);
            for name in [
                SEARCH,
                CLEAR,
                BACK,
                PALETTE,
                DASHBOARD,
                MODE_ACTIONS,
                MODE_SEARCH,
            ] {
                let icon = cache.get(name, 16 * scale).unwrap();
                assert_eq!(icon.size, 16 * scale);
                assert_eq!(icon.px.len(), (16 * scale * 16 * scale * 4) as usize);
                assert!(icon.px.chunks(4).any(|p| p[3] != 0));
                assert_eq!(icon.px[3], 0);
            }
        }
        cache.ensure_input(1);
        assert_eq!(cache.map.len(), 21);
        for index in 0..90 {
            cache.map.insert((format!("missing-{index}"), 16), None);
        }
        cache.ensure_input(4);
        assert!(cache.get(SEARCH, 64).is_some());
        assert!(cache.get(CLEAR, 64).is_some());
        assert_eq!(cache.map.len(), 7);
    }

    #[test]
    fn resize_keeps_colour_and_centres() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        // 2x1 opaque red image into 4x4: occupies the middle two rows
        let src = [255, 0, 0, 255, 255, 0, 0, 255];
        let out = resize(&src, 2, 1, 4);
        assert_eq!(&out[16..20], &[255, 0, 0, 255]);
        assert_eq!(out[3], 0);
    }
    #[test]
    fn svg_renders() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let svg = b"<svg xmlns='http://www.w3.org/2000/svg' width='10' height='10'><rect width='10' height='10' fill='lime'/></svg>";
        let px = render_svg(svg, 8).unwrap();
        assert_eq!(&px[0..4], &[0, 255, 0, 255]);
    }
    #[test]
    fn emoji_sequences_compose_when_a_colour_font_is_available() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let Ok(out) = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", "emoji"])
            .output()
        else {
            return;
        };
        let path = String::from_utf8_lossy(&out.stdout).into_owned();
        let Ok(font) = std::fs::read(&path) else {
            return;
        };
        if emoji_png_lenient(&font, "\u{1f600}").is_none() {
            return; // no bitmap emoji font here: nothing to check
        }
        let man = emoji_png_lenient(&font, "\u{1f468}").unwrap();
        let family = emoji_png_lenient(&font, "\u{1f468}\u{200d}\u{1f469}\u{200d}\u{1f467}")
            .expect("ZWJ family sequence composes");
        assert_ne!(man, family);
        assert!(
            emoji_png_lenient(&font, "\u{1f1ee}\u{1f1f3}").is_some(),
            "flag composes"
        );
        let wave = emoji_png_lenient(&font, "\u{1f44b}").unwrap();
        let wave_tone = emoji_png_lenient(&font, "\u{1f44b}\u{1f3fd}").expect("skin tone composes");
        assert_ne!(wave, wave_tone);
    }
    #[test]
    fn missing_icon_is_remembered() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let mut c = Cache::new("", "");
        c.ensure("definitely-not-an-icon-zzz", 32);
        assert!(c.get("definitely-not-an-icon-zzz", 32).is_none());
        assert_eq!(c.map.len(), 1);
    }
    #[test]
    fn cache_holds_twenty_rows_of_eight_emoji() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let mut cache = Cache::new("", "");
        let names: Vec<_> = emojis::iter()
            .take(160)
            .map(|e| format!("emoji:{}", e.as_str()))
            .collect();
        for name in &names {
            cache.ensure(name, 28);
        }
        assert_eq!(cache.map.len(), 160);
        assert!(names
            .iter()
            .all(|name| cache.map.contains_key(&(name.clone(), 28))));
    }
    #[test]
    fn png_roundtrip() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let mut buf = Vec::new();
        {
            let mut e = png::Encoder::new(&mut buf, 2, 2);
            e.set_color(png::ColorType::Rgba);
            e.set_depth(png::BitDepth::Eight);
            let mut w = e.write_header().unwrap();
            w.write_image_data(&[
                0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255, 0, 0, 255, 255,
            ])
            .unwrap();
        }
        let px = decode_png(&buf, 4).unwrap();
        assert_eq!(&px[0..4], &[0, 0, 255, 255]);
    }
}

#[cfg(test)]
mod resolver_tests {
    use super::*;
    #[test]
    fn desktop_theme_priority_is_kde_then_gtk3_then_gtk4() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(root.path().join("gtk-3.0")).unwrap();
        std::fs::create_dir_all(root.path().join("gtk-4.0")).unwrap();
        let gtk4 = root.path().join("gtk-4.0/settings.ini");
        let gtk3 = root.path().join("gtk-3.0/settings.ini");
        let kde = root.path().join("kdeglobals");
        std::fs::write(gtk4, "gtk-icon-theme-name=Fourth\n").unwrap();
        assert_eq!(desktop_theme(root.path()).as_deref(), Some("Fourth"));
        std::fs::write(&gtk3, "[Settings]\n gtk-icon-theme-name=Third\n").unwrap();
        assert_eq!(desktop_theme(root.path()).as_deref(), Some("Third"));
        std::fs::write(
            &kde,
            "[Other]\nTheme=Wrong\n[Icons]\nTheme=Kde\n[Else]\nTheme=Wrong\n",
        )
        .unwrap();
        assert_eq!(desktop_theme(root.path()).as_deref(), Some("Kde"));
        std::fs::remove_file(kde).unwrap();
        assert_eq!(desktop_theme(root.path()).as_deref(), Some("Third"));
    }
    #[test]
    fn reference_lookup_fixture_worker() {
        let Some(root) = std::env::var_os("RL_ICON_LOOKUP_TEST_ROOT") else {
            return;
        };
        let root = PathBuf::from(root);
        let themes = theme_chain("");
        assert_eq!(themes, ["Child", "hicolor"]);
        assert_eq!(
            find("shared-icon", &themes),
            Some(root.join("data/icons/Parent/64x64/apps/shared-icon.svg"))
        );
        assert_eq!(
            find("fallback-icon", &themes),
            Some(root.join("data/icons/hicolor/64x64/apps/fallback-icon.svg"))
        );
        assert_eq!(
            find_window("ORG.EXAMPLE.SHARED-ICON", &themes),
            Some(root.join("data/icons/Parent/64x64/apps/shared-icon.svg"))
        );
        let absolute = root.join("absolute.svg");
        assert_eq!(find(absolute.to_str().unwrap(), &themes), Some(absolute));
        assert!(find("rl-icon-should-not-exist-xyz", &themes).is_none());
        assert!(find(root.join("missing.svg").to_str().unwrap(), &themes).is_none());
    }
    #[test]
    fn reference_lookup_uses_theme_inheritance_and_window_name_fallbacks() {
        let root = tempfile::tempdir().unwrap();
        for theme in ["Child", "Parent", "hicolor"] {
            let path = root.path().join(format!("data/icons/{theme}"));
            std::fs::create_dir_all(path.join("64x64/apps")).unwrap();
            std::fs::write(path.join("index.theme"), format!("[Icon Theme]\nName={theme}\nDirectories=64x64/apps\n{}[64x64/apps]\nSize=64\nType=Fixed\nContext=Applications\n", if theme == "Child" { "Inherits=Parent\n" } else { "" })).unwrap();
        }
        let svg = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"64\"/>";
        std::fs::write(
            root.path()
                .join("data/icons/Parent/64x64/apps/shared-icon.svg"),
            svg,
        )
        .unwrap();
        std::fs::write(
            root.path()
                .join("data/icons/hicolor/64x64/apps/fallback-icon.svg"),
            svg,
        )
        .unwrap();
        std::fs::write(root.path().join("absolute.svg"), svg).unwrap();
        std::fs::create_dir_all(root.path().join("config/gtk-3.0")).unwrap();
        std::fs::write(
            root.path().join("config/gtk-3.0/settings.ini"),
            "gtk-icon-theme-name=Child\n",
        )
        .unwrap();
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "icons::resolver_tests::reference_lookup_fixture_worker",
                "--nocapture",
            ])
            .env("RL_ICON_LOOKUP_TEST_ROOT", root.path())
            .env("HOME", root.path())
            .env("XDG_DATA_HOME", root.path().join("data"))
            .env("XDG_DATA_DIRS", root.path().join("data"))
            .env("XDG_CONFIG_HOME", root.path().join("config"))
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
    }
}

#[cfg(test)]
mod fallback_tests {
    use super::*;

    #[test]
    fn pixmaps_and_unindexed_theme_folders_are_searched_in_every_data_dir() {
        let root = tempfile::tempdir().unwrap();
        let share = root.path().join("profile/share");
        std::fs::create_dir_all(share.join("pixmaps")).unwrap();
        std::fs::write(share.join("pixmaps/legacy-app.png"), b"x").unwrap();
        // Breeze layout (apps/48) in a theme with no index.theme.
        std::fs::create_dir_all(share.join("icons/breeze/apps/48")).unwrap();
        std::fs::write(
            share.join("icons/breeze/apps/48/org.kde.plasma.emojier.svg"),
            b"x",
        )
        .unwrap();
        std::fs::create_dir_all(share.join("icons/other/64x64/apps")).unwrap();
        std::fs::write(
            share.join("icons/other/64x64/apps/claude-desktop.png"),
            b"x",
        )
        .unwrap();
        let dirs = [root.path().join("empty"), share.clone()];
        assert_eq!(
            scan_dirs("legacy-app", &dirs),
            Some(share.join("pixmaps/legacy-app.png"))
        );
        assert_eq!(
            scan_dirs("org.kde.plasma.emojier", &dirs),
            Some(share.join("icons/breeze/apps/48/org.kde.plasma.emojier.svg"))
        );
        assert_eq!(
            scan_dirs("claude-desktop", &dirs),
            Some(share.join("icons/other/64x64/apps/claude-desktop.png"))
        );
        assert_eq!(scan_dirs("nothing-here", &dirs), None);
    }

    #[test]
    fn window_class_finds_its_desktop_entry_icon() {
        let root = tempfile::tempdir().unwrap();
        let apps = root.path().join("applications");
        std::fs::create_dir_all(&apps).unwrap();
        std::fs::write(
            apps.join("claude-desktop.desktop"),
            "[Desktop Entry]\nName=Claude\nIcon=claude-icon\nStartupWMClass=Claude\n",
        )
        .unwrap();
        std::fs::write(
            apps.join("org.kde.systemmonitor.desktop"),
            "[Desktop Entry]\nName=Monitor\nIcon=utilities-system-monitor\n",
        )
        .unwrap();
        let dirs = [root.path().to_path_buf()];
        assert_eq!(
            desktop_entry_icon_in("Claude", &dirs).as_deref(),
            Some("claude-icon")
        );
        assert_eq!(
            desktop_entry_icon_in("org.kde.SystemMonitor", &dirs).as_deref(),
            Some("utilities-system-monitor")
        );
        assert_eq!(desktop_entry_icon_in("unknown", &dirs), None);
    }
}

#[cfg(test)]
mod allocation_tests {
    use super::*;

    #[test]
    fn oversized_theme_icons_are_not_allocated_or_cached() {
        let mut cache = Cache::new("", "");
        for name in ["phosphor:palette", SEARCH, "😀", "/missing.png"] {
            for size in [0, 1025, 65536, u32::MAX] {
                cache.ensure(name, size);
                assert!(cache.get(name, size).is_none());
            }
        }
        cache.ensure("phosphor:palette", 24);
        assert_eq!(
            cache.get("phosphor:palette", 24).unwrap().px.len(),
            24 * 24 * 4
        );
    }
}
