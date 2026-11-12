//! Icon lookup and decoding. Icons load only when a row is visible and stay in a small cache.
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Premultiplied RGBA pixels, `size` x `size`.
pub struct Icon {
    pub size: u32,
    pub px: Vec<u8>,
    /// Stroke-only builtin symbol: draw tinted, not as-is.
    pub symbol: bool,
}

pub const SEARCH: &str = "builtin:search";
pub const CLEAR: &str = "builtin:clear";
pub const BACK: &str = "builtin:back";
pub const PALETTE: &str = "builtin:palette";

const BACK_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="m12 19-7-7 7-7"/><path d="M19 12H5"/></svg>"#;
const PALETTE_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round"><circle cx="12" cy="12" r="9"/><circle cx="8.5" cy="10" r="1.2"/><circle cx="12" cy="7.5" r="1.2"/><circle cx="15.5" cy="10" r="1.2"/><circle cx="9" cy="14.5" r="1.2"/></svg>"#;

// Lucide circle-x (ISC, see NOTICE): the glyph zlaunch's clear button shows.
const CLEAR_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="10"/><path d="m15 9-6 6"/><path d="m9 9 6 6"/></svg>"#;

const SMILEY_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M8 14s1.5 2 4 2 4-2 4-2"/><line x1="9" y1="9" x2="9.01" y2="9"/><line x1="15" y1="9" x2="15.01" y2="9"/></svg>"#;
const CLIPBOARD_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="8" y="2" width="8" height="4" rx="1"/><path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"/></svg>"#;
const SETTINGS_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="3.2"/><path d="M12 1.8v3M12 19.2v3M1.8 12h3M19.2 12h3M4.7 4.7l2.2 2.2M17.1 17.1l2.2 2.2M4.7 19.3l2.2-2.2M17.1 6.9l2.2-2.2"/></svg>"#;
const GLOBE_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="9"/><path d="M3 12h18"/><path d="M12 3c-2.5 2.6-4 5.6-4 9s1.5 6.4 4 9"/><path d="M12 3c2.5 2.6 4 5.6 4 9s-1.5 6.4-4 9"/></svg>"#;
const TERMINAL_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="4" width="18" height="16" rx="2"/><path d="m7 9 3 3-3 3"/><line x1="13" y1="15" x2="17" y2="15"/></svg>"#;
const FOLDER_SVG: &[u8] = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v9a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z"/></svg>"#;

/// Builtin strokes for the mode rows, used when no icon theme provides them.
fn fallback_symbol(name: &str) -> Option<&'static [u8]> {
    Some(match name {
        "smiley" | "face-smile" => SMILEY_SVG,
        "clipboard" | "edit-paste" => CLIPBOARD_SVG,
        "palette" => PALETTE_SVG,
        "settings" | "preferences-system" | "emblem-system" => SETTINGS_SVG,
        "globe" | "web-browser" | "applications-internet" => GLOBE_SVG,
        "folder" | "system-file-manager" => FOLDER_SVG,
        "utilities-terminal" => TERMINAL_SVG,
        _ => return None,
    })
}

const SIZE_DIRS: [&str; 12] = [
    "48x48", "64x64", "128x128", "256x256", "512x512", "scalable", "32x32", "96x96", "36x36",
    "24x24", "22x22", "16x16",
];

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
    v
}

/// The themes tried in order: the configured one first, then common fallbacks.
pub fn theme_chain(theme: &str) -> Vec<String> {
    let mut themes = Vec::new();
    if !theme.is_empty() {
        themes.push(theme.to_string());
    }
    for t in ["hicolor", "Adwaita", "breeze", "Papirus"] {
        if !themes.iter().any(|x| x == t) {
            themes.push(t.to_string());
        }
    }
    themes
}

/// Find an icon file for `name`. An absolute path is used as is.
pub fn find(name: &str, themes: &[String]) -> Option<PathBuf> {
    if name.is_empty() {
        return None;
    }
    if name.starts_with('/') {
        return Some(PathBuf::from(name)).filter(|p| p.is_file());
    }
    let dirs = data_dirs();
    let names = [name.to_string(), format!("{name}-symbolic")];
    for theme in themes {
        for base in &dirs {
            let root = base.join("icons").join(theme);
            if !root.is_dir() {
                continue;
            }
            for sz in SIZE_DIRS {
                for n in &names {
                    for ext in ["png", "svg"] {
                        for cat in ["apps", "mimetypes", "places", "devices", "status"] {
                            for layout in [format!("{sz}/{cat}"), format!("{cat}/{sz}")] {
                                for file in [format!("{n}.{ext}"), format!("{n}.symbolic.{ext}")] {
                                    let p = root.join(&layout).join(&file);
                                    if p.is_file() {
                                        return Some(p);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    for base in &dirs {
        for ext in ["png", "svg"] {
            let p = base.join("pixmaps").join(format!("{name}.{ext}"));
            if p.is_file() {
                return Some(p);
            }
        }
    }
    let p = PathBuf::from("/usr/share/pixmaps").join(format!("{name}.png"));
    if p.is_file() {
        Some(p)
    } else {
        None
    }
}

/// Decode a PNG or SVG file to `size` x `size` premultiplied pixels.
pub fn load(path: &Path, size: u32) -> Option<Icon> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    let data = std::fs::read(path).ok()?;
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

fn decode_png(data: &[u8], size: u32) -> Option<Vec<u8>> {
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
        }
    }
    pub fn ensure(&mut self, name: &str, size: u32) {
        let key = (name.to_string(), size);
        if self.map.contains_key(&key) {
            return;
        }
        if self.map.len() >= 256 {
            self.map.clear();
        }
        let symbol = match name {
            SEARCH => Some(include_bytes!("../assets/icons/search.svg").as_slice()),
            CLEAR => Some(CLEAR_SVG),
            BACK => Some(BACK_SVG),
            PALETTE => Some(PALETTE_SVG),
            _ => None,
        };
        let icon = if let Some(svg) = symbol {
            render_svg(svg, size).map(|px| Icon {
                size,
                px,
                symbol: true,
            })
        } else if let Some(g) = name.strip_prefix("emoji:") {
            self.emoji(g, size)
        } else {
            find(name, &self.themes)
                .and_then(|p| load(&p, size))
                .or_else(|| {
                    fallback_symbol(name).and_then(|svg| {
                        render_svg(svg, size).map(|px| Icon {
                            size,
                            px,
                            symbol: true,
                        })
                    })
                })
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
    fn input_symbols_are_embedded_and_cached_at_each_scale() {
        let mut cache = Cache::new("", "");
        for scale in [1, 2, 3] {
            cache.ensure_input(scale);
            for name in [SEARCH, CLEAR, BACK, PALETTE] {
                let icon = cache.get(name, 16 * scale).unwrap();
                assert_eq!(icon.size, 16 * scale);
                assert_eq!(icon.px.len(), (16 * scale * 16 * scale * 4) as usize);
                assert!(icon.px.chunks(4).any(|p| p[3] != 0));
                assert_eq!(icon.px[3], 0);
            }
        }
        cache.ensure_input(1);
        assert_eq!(cache.map.len(), 12);
        for index in 0..90 {
            cache.map.insert((format!("missing-{index}"), 16), None);
        }
        cache.ensure_input(4);
        assert!(cache.get(SEARCH, 64).is_some());
        assert!(cache.get(CLEAR, 64).is_some());
        assert_eq!(cache.map.len(), 4);
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
