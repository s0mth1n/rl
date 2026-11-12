//! Clipboard list and preview, using zlaunch's split-pane layout.
use crate::clip::{self, History, Img};
use crate::config::Rgb;
use crate::items::Action;
use crate::ui::{
    Canvas, View, CLIP_TOP, HEAD, INPUT_DIM, INPUT_FG, LIST_PAD, SEPARATOR, SEPARATOR_ALPHA,
};
use rusttype::{Font, Scale};
use std::cell::RefCell;
use std::rc::Rc;

struct Preview {
    w: u32,
    h: u32,
    pixels: Vec<u8>,
}

type CachedPreview = (u64, u32, u32, Option<Rc<Preview>>);
type CachedFile = (
    String,
    Option<std::time::SystemTime>,
    u64,
    u32,
    u32,
    Option<Rc<Preview>>,
);
thread_local! {
    static IMAGE: RefCell<Option<CachedPreview>> = const { RefCell::new(None) };
    static FILE_IMAGE: RefCell<Option<CachedFile>> = const { RefCell::new(None) };
}

/// Image files larger than this are not previewed.
const MAX_FILE_IMAGE: u64 = 64 * 1024 * 1024;

/// Read up to `limit` bytes of a file without blocking: a FIFO or device at a
/// clipboard path must not stall the preview render.
fn read_capped(path: &str, limit: u64) -> Option<(Vec<u8>, Option<u64>)> {
    use std::io::Read;
    use std::os::unix::fs::OpenOptionsExt;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let total = file.metadata().ok().map(|m| m.len());
    let mut bytes = Vec::new();
    file.take(limit).read_to_end(&mut bytes).ok()?;
    Some((bytes, total))
}

/// Preview an image file, cached per content and pane size like `image_preview`.
fn file_image_preview(path: &str, w: u32, h: u32) -> Option<Rc<Preview>> {
    FILE_IMAGE.with(|slot| {
        let mut cache = slot.borrow_mut();
        let meta = std::fs::metadata(path).ok()?;
        let key = (path.to_string(), meta.modified().ok(), meta.len());
        if let Some((p, mtime, len, cw, ch, result)) = cache.as_ref() {
            if (p, mtime, len, cw, ch) == (&key.0, &key.1, &key.2, &w, &h) {
                return result.clone();
            }
        }
        let result = read_capped(path, MAX_FILE_IMAGE + 1)
            .and_then(|(bytes, _)| {
                (bytes.len() as u64 <= MAX_FILE_IMAGE)
                    .then_some(bytes)
                    .and_then(|b| decode(&b, w, h))
            })
            .map(Rc::new);
        *cache = Some((key.0, key.1, key.2, w, h, result.clone()));
        result
    })
}

fn fit(w: u32, h: u32, max_w: u32, max_h: u32) -> (u32, u32) {
    if w == 0 || h == 0 || max_w == 0 || max_h == 0 {
        return (0, 0);
    }
    let factor = (max_w as f64 / w as f64).min(max_h as f64 / h as f64);
    (
        (w as f64 * factor).round().max(1.0) as u32,
        (h as f64 * factor).round().max(1.0) as u32,
    )
}

fn decode(data: &[u8], max_w: u32, max_h: u32) -> Option<Preview> {
    // zlaunch previews png/jpg/jpeg/gif/webp/svg files (clipboard_rendering.rs).
    if looks_like_svg(data) {
        return decode_svg(data, max_w, max_h);
    }
    if !data.starts_with(b"\x89PNG") {
        return decode_raster(data, max_w, max_h);
    }
    decode_png(data, max_w, max_h)
}

fn looks_like_svg(data: &[u8]) -> bool {
    data[..data.len().min(4096)]
        .windows(4)
        .any(|w| w == b"<svg")
}

fn decode_svg(data: &[u8], max_w: u32, max_h: u32) -> Option<Preview> {
    use resvg::{tiny_skia, usvg};
    let tree = usvg::Tree::from_data(data, &usvg::Options::default()).ok()?;
    let s = tree.size();
    let (w, h) = fit(
        s.width().max(1.0) as u32,
        s.height().max(1.0) as u32,
        max_w,
        max_h,
    );
    if w == 0 || h == 0 {
        return None;
    }
    let k = (w as f32 / s.width()).min(h as f32 / s.height());
    let mut pm = tiny_skia::Pixmap::new(w, h)?;
    resvg::render(
        &tree,
        tiny_skia::Transform::from_row(k, 0.0, 0.0, k, 0.0, 0.0),
        &mut pm.as_mut(),
    );
    Some(Preview {
        w,
        h,
        pixels: pm.take(),
    })
}

fn decode_raster(data: &[u8], max_w: u32, max_h: u32) -> Option<Preview> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(data))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    reader.limits(limits);
    let img = reader.decode().ok()?;
    let (w, h) = fit(img.width(), img.height(), max_w, max_h);
    if w == 0 || h == 0 {
        return None;
    }
    let raw = image::imageops::resize(&img, w, h, image::imageops::FilterType::Nearest).into_raw();
    let mut pixels = Vec::with_capacity(raw.len());
    for p in raw.as_chunks::<4>().0 {
        let a = p[3] as u32;
        pixels.extend_from_slice(&[
            (p[0] as u32 * a / 255) as u8,
            (p[1] as u32 * a / 255) as u8,
            (p[2] as u32 * a / 255) as u8,
            p[3],
        ]);
    }
    Some(Preview { w, h, pixels })
}

fn decode_png(data: &[u8], max_w: u32, max_h: u32) -> Option<Preview> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(data));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    decoder.set_limits(png::Limits {
        bytes: 32 * 1024 * 1024,
    });
    let mut reader = decoder.read_info().ok()?;
    if reader.output_buffer_size() > 32 * 1024 * 1024 {
        return None;
    }
    let mut raw = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut raw).ok()?;
    let (w, h) = fit(info.width, info.height, max_w, max_h);
    if w == 0 || h == 0 {
        return None;
    }
    let channels = info.color_type.samples();
    let mut pixels = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        for x in 0..w {
            let sx = x as u64 * info.width as u64 / w as u64;
            let sy = y as u64 * info.height as u64 / h as u64;
            let offset = (sy * info.width as u64 + sx) as usize * channels;
            let p = &raw[offset..offset + channels];
            let rgba = match info.color_type {
                png::ColorType::Rgb => [p[0], p[1], p[2], 255],
                png::ColorType::Rgba => [p[0], p[1], p[2], p[3]],
                png::ColorType::Grayscale => [p[0], p[0], p[0], 255],
                png::ColorType::GrayscaleAlpha => [p[0], p[0], p[0], p[1]],
                _ => return None,
            };
            let a = rgba[3] as u32;
            pixels.extend_from_slice(&[
                (rgba[0] as u32 * a / 255) as u8,
                (rgba[1] as u32 * a / 255) as u8,
                (rgba[2] as u32 * a / 255) as u8,
                rgba[3],
            ]);
        }
    }
    Some(Preview { w, h, pixels })
}

fn image_preview(image: &Img, w: u32, h: u32) -> Option<Rc<Preview>> {
    IMAGE.with(|slot| {
        let mut cache = slot.borrow_mut();
        if let Some((id, cw, ch, result)) = cache.as_ref() {
            if (*id, *cw, *ch) == (image.id, w, h) {
                return result.clone();
            }
        }
        let result = image
            .bytes()
            .and_then(|png| decode(&png, w, h))
            .map(Rc::new);
        *cache = Some((image.id, w, h, result.clone()));
        result
    })
}

fn draw_image(cv: &mut Canvas, image: &Preview, x: u32, y: u32) {
    for iy in 0..image.h.min(cv.h.saturating_sub(y)) {
        for ix in 0..image.w.min(cv.w.saturating_sub(x)) {
            let src = ((iy * image.w + ix) * 4) as usize;
            let dst = (((y + iy) * cv.w + x + ix) * 4) as usize;
            let a = image.pixels[src + 3] as u32;
            let k = 255 - a;
            for (d, s) in [(0, 2), (1, 1), (2, 0)] {
                cv.buf[dst + d] = (image.pixels[src + s] as u32 + cv.buf[dst + d] as u32 * k / 255)
                    .min(255) as u8;
            }
            cv.buf[dst + 3] = (a + cv.buf[dst + 3] as u32 * k / 255).min(255) as u8;
        }
    }
}

fn wrap(font: &Font, size: f32, text: &str, width: f32, max_lines: usize) -> Vec<String> {
    if width <= 0.0 || max_lines == 0 {
        return Vec::new();
    }
    let scale = Scale::uniform(size);
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut used = 0.0;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\r' {
            continue;
        }
        let glyph = if c == '\t' { ' ' } else { c };
        let advance = font.glyph(glyph).scaled(scale).h_metrics().advance_width;
        if c == '\n' || (!line.is_empty() && used + advance > width) {
            let remainder = if c != '\n' {
                line.rfind(char::is_whitespace)
                    .filter(|&i| i > 0)
                    .map(|i| {
                        let rest = line[i..].trim_start().to_string();
                        line.truncate(i);
                        rest
                    })
                    .unwrap_or_default()
            } else {
                String::new()
            };
            lines.push(std::mem::replace(&mut line, remainder));
            used = Canvas::text_width(font, size, &line);
            if lines.len() == max_lines {
                if c != '\n' || chars.peek().is_some() {
                    let last = lines.last_mut().unwrap();
                    while !last.is_empty()
                        && Canvas::text_width(font, size, &format!("{last}...")) > width
                    {
                        last.pop();
                    }
                    last.push_str("...");
                }
                return lines;
            }
            if c.is_whitespace() {
                continue;
            }
        }
        line.push(glyph);
        used += advance;
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

pub fn draw(cv: &mut Canvas, font: &Font, v: &View, history: &History) {
    let s = v.scale as f32;
    let layout = crate::ui::Layout::clipboard(&v.theme.metrics);
    let top0 = HEAD + CLIP_TOP;
    let th = v.theme;
    let m = th.metrics;
    let w = v.w as f32 * s;
    let h = v.h as f32 * s;
    let split = w * 0.5;
    let fs = v.font_size * s;
    cv.rrect(
        0.0,
        0.0,
        w,
        h,
        th.window_border_radius * s,
        th.bg,
        th.bg_alpha,
    );
    // Visible LauncherTheme.window_border outline, matching the launcher view.
    cv.rrect_outline(
        0.0,
        0.0,
        w,
        h,
        th.window_border_radius * s,
        s,
        th.border,
        th.border_alpha,
    );
    let query = v.query.strip_prefix(';').unwrap_or(v.query).trim_start();
    if v.back {
        if let Some(icon) = v.icons.get(crate::icons::BACK, 16 * v.scale) {
            cv.symbol(
                (22.0 * s) as i32,
                ((HEAD - 16.0) * 0.5 * s) as i32,
                icon,
                INPUT_DIM,
                1.0,
            );
        }
    } else if let Some(icon) = v.icons.get("phosphor:copy", 16 * v.scale) {
        // Direct clipboard mode: zlaunch's input prefix is the copy icon.
        cv.symbol(
            (22.0 * s) as i32,
            ((HEAD - 16.0) * 0.5 * s) as i32,
            icon,
            INPUT_DIM,
            1.0,
        );
    }
    let text_x = crate::ui::INPUT_X;
    cv.text(
        font,
        fs * 1.2,
        text_x * s,
        17.0 * s,
        if query.is_empty() {
            "Search clipboard history..."
        } else {
            query
        },
        if query.is_empty() {
            INPUT_DIM
        } else {
            INPUT_FG
        },
        1.0,
        w - 48.0 * s,
    );
    cv.rrect(0.0, (HEAD - 1.0) * s, w, s, 0.0, SEPARATOR, SEPARATOR_ALPHA);
    // List/preview separator: theme.window_border, transparent under the daemon.
    cv.rrect(
        split,
        HEAD * s,
        s,
        (h - HEAD * s - s).max(0.0),
        0.0,
        th.border,
        0.0,
    );
    cv.set_clip_top((top0 * s) as i32);
    cv.set_clip_y(((top0 + v.vp) * s) as i32);
    for (row, item) in v
        .items
        .iter()
        .skip(v.top)
        .take(((v.vp + v.scroll_remainder) / layout.row).ceil() as usize)
        .enumerate()
    {
        let y = (top0 + row as f32 * layout.row - v.scroll_remainder) * s;
        let selected = row + v.top == v.sel;
        let left = 13.0 * s;
        let box_y = y + (m.item_margin_y - 1.0) * s;
        let box_h = (layout.row - 2.0 * m.item_margin_y - 8.0).max(1.0) * s;
        let ix = left + m.item_padding_x * s;
        let icon_size = m.icon_size.max(0.0) * s;
        let iy = box_y + (box_h - icon_size) / 2.0;
        let tx = ix + icon_size + 7.0 * s;
        let bg = if selected {
            crate::config::Rgba {
                rgb: th.select,
                alpha: th.select_alpha,
            }
        } else {
            th.item_background
        };
        cv.rrect(
            left,
            box_y,
            (split - left - (m.item_margin_x + 12.0) * s).max(0.0),
            box_h,
            m.item_border_radius * s,
            bg.rgb,
            bg.alpha,
        );
        cv.rrect(
            ix,
            iy,
            icon_size,
            icon_size,
            m.icon_border_radius * s,
            th.icon_placeholder_background.rgb,
            th.icon_placeholder_background.alpha,
        );
        if let Some(color) = match &item.action {
            Action::Clip(text, _) => clip::parse_color(text),
            _ => None,
        } {
            // Colour text gets a filled circle inside the icon box, like zlaunch.
            let d = m.clipboard_color_icon_size * s;
            cv.rrect(
                ix + (icon_size - d) / 2.0,
                iy + (icon_size - d) / 2.0,
                d,
                d,
                d * 0.5,
                [color[0], color[1], color[2]],
                color[3] as f32 / 255.0,
            );
            cv.rrect_outline(
                ix + (icon_size - d) / 2.0,
                iy + (icon_size - d) / 2.0,
                d,
                d,
                d * 0.5,
                s,
                th.border,
                th.border_alpha,
            );
        } else {
            // zlaunch's phosphor icon per content type.
            let icon = match &item.action {
                Action::ClipImg(..) => "phosphor:image",
                Action::ClipFiles(text, _) => {
                    let mut lines = text.lines();
                    match (lines.next(), lines.next()) {
                        (Some(path), None) => clip::clip_file_icon(path),
                        _ => "phosphor:file",
                    }
                }
                Action::Clip(text, _) => match clip::file_url_path(text) {
                    Some(path) => clip::clip_file_icon(&path),
                    None => "phosphor:clipboard-text",
                },
                _ => "phosphor:clipboard-text",
            };
            if let Some(icon) = v.icons.get(icon, crate::ui::icon_px(&th, v.scale)) {
                cv.symbol(
                    ix as i32,
                    iy as i32,
                    icon,
                    th.icon_placeholder_color.rgb,
                    th.icon_placeholder_color.alpha,
                );
            }
        }
        // The 0.5.1 release shows no Delete / Shift+Del chip on the selected row
        // (it exists only in pinned ee71f96), so the title gets the full width.
        let max_w = (split - tx - 16.0 * s).max(0.0);
        let text_top = box_y
            + (box_h - (m.item_title_line_height + m.layout_item_description_height) * s) / 2.0;
        cv.text(
            font,
            fs.min(14.0 * s),
            tx,
            text_top,
            &item.title,
            th.text,
            th.text_alpha,
            max_w,
        );
        cv.text(
            font,
            12.0 * s,
            tx,
            text_top
                + (m.item_title_line_height
                    + (m.layout_item_description_height - 12.0) / 2.0
                    + 1.0)
                    * s,
            &item.sub,
            th.dim,
            th.dim_alpha,
            max_w,
        );
    }
    if v.items.is_empty() {
        // zlaunch centres the empty state in the list column at empty_state_height.
        let eh = m.empty_state_height * s;
        let label = "No clipboard history";
        let tw = Canvas::text_width(font, 14.0 * s, label);
        cv.text(
            font,
            14.0 * s,
            ((split - 8.0 * s - tw) / 2.0).max(0.0),
            HEAD * s + (eh - 14.0 * s) / 2.0,
            label,
            th.empty_state_color.rgb,
            th.empty_state_color.alpha,
            tw + 4.0,
        );
    }
    cv.clear_clip();
    // The release draws a 1px divider between the list and the preview pane.
    cv.rrect(
        split,
        HEAD * s,
        m.layout_separator_width * s,
        h - HEAD * s,
        0.0,
        crate::ui::SEPARATOR,
        crate::ui::SEPARATOR_ALPHA,
    );
    let x = split + (m.layout_separator_width + m.clipboard_preview_padding) * s;
    // Measured against the 0.5.1 release: preview text starts 4px higher.
    let y = (HEAD + LIST_PAD + m.clipboard_preview_padding - 4.0) * s;
    let pw = (w - x - (1.0 + m.clipboard_preview_padding) * s).max(0.0);
    let ph = (h - y - m.clipboard_preview_padding * s).max(0.0);
    draw_preview(cv, font, v, history, (x, y, pw, ph));
    crate::ui::draw_scrollbar(cv, v);
}

/// Preview pane contents for the selected entry (scrollbar always draws after this).
fn draw_preview(
    cv: &mut Canvas,
    font: &Font,
    v: &View,
    history: &History,
    area: (f32, f32, f32, f32),
) {
    let (x, y, pw, ph) = area;
    let s = v.scale as f32;
    let th = v.theme;
    let fs = v.font_size * s;
    let action = v.items.get(v.sel).map(|item| &item.action);
    if let Some(Action::ClipImg(id, _)) = action {
        if let Some(image) = history
            .images
            .iter()
            .find(|image| image.id == *id)
            .and_then(|image| image_preview(image, pw as u32, ph as u32))
        {
            draw_image(
                cv,
                &image,
                (x + (pw - image.w as f32) * 0.5) as u32,
                (y + (ph - image.h as f32) * 0.5) as u32,
            );
        } else {
            centered_label(
                cv,
                font,
                s,
                x,
                y,
                pw,
                ph,
                "[Image preview unavailable]",
                th.dim,
                th.dim_alpha,
            );
        }
    } else {
        IMAGE.with(|slot| *slot.borrow_mut() = None);
        let (text, files) = match action {
            Some(Action::Clip(text, _)) => (text.as_str(), false),
            Some(Action::ClipFiles(text, _)) => (text.as_str(), true),
            _ => {
                centered_label(
                    cv,
                    font,
                    s,
                    x,
                    y,
                    pw,
                    ph,
                    "No selection",
                    th.empty_state_color.rgb,
                    th.empty_state_color.alpha,
                );
                return;
            }
        };
        if !files {
            if let Some(color) = clip::parse_color(text) {
                draw_color_preview(cv, font, s, (x, y, pw, ph), color, th);
                return;
            }
        }
        let paths: Option<Vec<String>> = if files {
            Some(text.lines().map(str::to_string).collect())
        } else {
            clip::file_url_path(text).map(|p| vec![p])
        };
        if let Some(paths) = paths {
            draw_paths_preview(cv, font, s, x, y, pw, ph, &paths, th);
            return;
        }
        let line_h = 20.0 * s;
        for (row, line) in wrap(font, fs.min(14.0 * s), text, pw, (ph / line_h) as usize)
            .iter()
            .enumerate()
        {
            cv.text(
                font,
                fs.min(14.0 * s),
                x,
                y + row as f32 * line_h,
                line,
                th.text,
                th.text_alpha,
                pw,
            );
        }
    }
}

/// File text preview: the first 10000 bytes (cut at a char boundary, never
/// mid-character) with the total size, like zlaunch's file preview.
fn truncate_file_text(content: &str, total: u64) -> String {
    if total > 10000 {
        format!(
            "{}...\n\n[Content truncated - {} bytes total]",
            &content[..content.floor_char_boundary(10000)],
            total
        )
    } else {
        content.to_string()
    }
}

/// Preview text from a file: a bounded, nonblocking read, so a huge file is
/// truncated by its real size and a FIFO cannot hang the render.
fn file_text_preview(path: &str) -> Option<String> {
    let (bytes, total) = read_capped(path, 10001)?;
    let content = match String::from_utf8(bytes) {
        Ok(s) => s,
        // The capped read may split a character: keep the valid prefix.
        Err(e) if e.utf8_error().valid_up_to() >= 10000 => {
            let up = e.utf8_error().valid_up_to();
            String::from_utf8(e.into_bytes()[..up].to_vec()).ok()?
        }
        Err(_) => return None,
    };
    let total = total.unwrap_or(0).max(content.len() as u64);
    Some(truncate_file_text(&content, total))
}

/// Preview one or more clipboard file paths, like zlaunch's preview panel.
#[cfg(test)]
mod truncation_tests {
    #[test]
    fn file_text_truncation_respects_char_boundaries() {
        let content = format!("{}é{}", "x".repeat(9999), "y".repeat(200));
        let preview = super::truncate_file_text(&content, content.len() as u64);
        assert!(preview.starts_with(&"x".repeat(9999)));
        assert!(preview.contains("[Content truncated - 10201 bytes total]"));
        let short = "é".repeat(100);
        assert_eq!(super::truncate_file_text(&short, short.len() as u64), short);
    }

    #[test]
    fn file_preview_reports_the_real_size_of_large_files() {
        let dir = std::env::temp_dir().join(format!("zl-fp-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("big.txt");
        std::fs::write(&path, "x".repeat(1 << 20)).unwrap();
        let preview = super::file_text_preview(path.to_str().unwrap()).unwrap();
        assert!(preview.starts_with(&"x".repeat(10000)));
        assert!(preview.ends_with("[Content truncated - 1048576 bytes total]"));
        let invalid = dir.join("binary.txt");
        let mut bytes = "ok".repeat(5000).into_bytes();
        bytes.push(0xff);
        std::fs::write(&invalid, &bytes).unwrap();
        assert!(super::file_text_preview(invalid.to_str().unwrap()).is_some());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn file_preview_never_blocks_on_special_files() {
        let dir = std::env::temp_dir().join(format!("zl-fifo-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("pipe.txt");
        let c_path = std::ffi::CString::new(path.to_str().unwrap()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        // No writer ever opens the FIFO: the read must finish instead of blocking.
        assert_eq!(
            super::file_text_preview(path.to_str().unwrap()).as_deref(),
            Some("")
        );
        assert!(super::file_image_preview(path.to_str().unwrap(), 100, 100).is_none());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn image_file_preview_caches_by_content_and_size() {
        let dir = std::env::temp_dir().join(format!("zl-img-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("shot.png");
        std::fs::write(&path, crate::clip::tests_png(20, 10)).unwrap();
        let first = super::file_image_preview(path.to_str().unwrap(), 200, 100).unwrap();
        // Same bytes again: the cached preview is reused.
        let second = super::file_image_preview(path.to_str().unwrap(), 200, 100).unwrap();
        assert!(std::rc::Rc::ptr_eq(&first, &second));
        // Replaced file: the stale preview is not served.
        std::fs::write(&path, crate::clip::tests_png(8, 4)).unwrap();
        let third = super::file_image_preview(path.to_str().unwrap(), 200, 100).unwrap();
        assert!(!std::rc::Rc::ptr_eq(&first, &third));
        std::fs::remove_dir_all(&dir).ok();
    }
}
#[allow(clippy::too_many_arguments)]
fn draw_paths_preview(
    cv: &mut Canvas,
    font: &Font,
    s: f32,
    x: f32,
    y: f32,
    pw: f32,
    ph: f32,
    paths: &[String],
    th: crate::config::Theme,
) {
    if paths.len() == 1 {
        let path = &paths[0];
        if clip::is_image_path(path) {
            if let Some(image) = file_image_preview(path, pw as u32, ph as u32) {
                draw_image(
                    cv,
                    &image,
                    (x + (pw - image.w as f32) * 0.5) as u32,
                    (y + (ph - image.h as f32) * 0.5) as u32,
                );
                return;
            }
        }
        if clip::is_text_path(path) {
            if let Some(preview) = file_text_preview(path) {
                draw_wrapped(cv, font, s, x, y, pw, ph, &preview, th.text, th.text_alpha);
                return;
            }
        }
        draw_wrapped(cv, font, s, x, y, pw, ph, path, th.dim, th.dim_alpha);
        return;
    }
    let joined = paths.join("\n");
    draw_wrapped(cv, font, s, x, y, pw, ph, &joined, th.text, th.text_alpha);
}

/// Draw wrapped 14px text from the top of the preview pane.
#[allow(clippy::too_many_arguments)]
fn draw_wrapped(
    cv: &mut Canvas,
    font: &Font,
    s: f32,
    x: f32,
    y: f32,
    pw: f32,
    ph: f32,
    text: &str,
    c: Rgb,
    alpha: f32,
) {
    let line_h = 20.0 * s;
    for (row, line) in wrap(font, 14.0 * s, text, pw, (ph / line_h) as usize)
        .iter()
        .enumerate()
    {
        cv.text(
            font,
            14.0 * s,
            x,
            y + row as f32 * line_h,
            line,
            c,
            alpha,
            pw,
        );
    }
}

/// Centre a 14px label in the preview pane.
#[allow(clippy::too_many_arguments)]
fn centered_label(
    cv: &mut Canvas,
    font: &Font,
    s: f32,
    x: f32,
    y: f32,
    pw: f32,
    ph: f32,
    label: &str,
    c: Rgb,
    alpha: f32,
) {
    let tw = Canvas::text_width(font, 14.0 * s, label);
    cv.text(
        font,
        14.0 * s,
        x + (pw - tw) / 2.0,
        y + (ph - 14.0 * s) / 2.0,
        label,
        c,
        alpha,
        tw + 4.0,
    );
}

/// Hue 0-360, saturation and lightness 0-100, matching zlaunch's conversion.
fn to_hsl([r, g, b, _]: [u8; 4]) -> (u16, u8, u8) {
    let (r, g, b) = (r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let delta = max - min;
    let l = (max + min) / 2.0;
    if delta == 0.0 {
        return (0, 0, (l * 100.0).round() as u8);
    }
    let s = delta / (1.0 - (2.0 * l - 1.0).abs());
    let mut h = if max == r {
        60.0 * (((g - b) / delta) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / delta + 2.0)
    } else {
        60.0 * ((r - g) / delta + 4.0)
    };
    if h < 0.0 {
        h += 360.0;
    }
    (
        h.round() as u16,
        (s * 100.0).round() as u8,
        (l * 100.0).round() as u8,
    )
}

/// Colour preview: swatch circle with HEX/RGB/HSL codes, centred in the pane.
#[allow(clippy::too_many_arguments)]
fn draw_color_preview(
    cv: &mut Canvas,
    font: &Font,
    s: f32,
    rect: (f32, f32, f32, f32),
    color: [u8; 4],
    th: crate::config::Theme,
) {
    let (x, y, pw, ph) = rect;
    let swatch = th.metrics.clipboard_color_swatch_size * s;
    let (h2, s2, l2) = to_hsl(color);
    let hex = if color[3] == 255 {
        format!("#{:02X}{:02X}{:02X}", color[0], color[1], color[2])
    } else {
        format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            color[0], color[1], color[2], color[3]
        )
    };
    let rows = [
        ("HEX", hex),
        ("RGB", format!("{}, {}, {}", color[0], color[1], color[2])),
        ("HSL", format!("{h2}\u{00b0}, {s2}%, {l2}%")),
    ];
    let row_h = 16.0 * s;
    let code_gap = th.metrics.clipboard_color_code_gap * s;
    let preview_gap = th.metrics.clipboard_color_preview_gap * s;
    let codes_h = 3.0 * row_h + 2.0 * code_gap;
    // Measured against the 0.5.1 release: the swatch sits 16px higher.
    let top = y + (ph - swatch - preview_gap - codes_h) / 2.0 - 12.0 * s;
    let sx = x + (pw - swatch) / 2.0;
    cv.rrect(
        sx,
        top,
        swatch,
        swatch,
        swatch * 0.5,
        [color[0], color[1], color[2]],
        color[3] as f32 / 255.0,
    );
    // zlaunch rings the swatch with a 1px window_border outline.
    cv.rrect_outline(
        sx,
        top,
        swatch,
        swatch,
        swatch * 0.5,
        s,
        th.border,
        th.border_alpha,
    );
    let mut ry = top + swatch + preview_gap;
    let label_w = th.metrics.clipboard_color_label_width * s;
    let value_w = rows
        .iter()
        .map(|(_, v)| Canvas::text_width(font, 14.0 * s, v))
        .fold(0.0, f32::max);
    let block_w = label_w + code_gap + value_w;
    let lx = x + (pw - block_w) / 2.0;
    for (label, value) in rows {
        cv.text(
            font,
            12.0 * s,
            lx,
            ry + 2.0 * s,
            label,
            th.dim,
            th.dim_alpha,
            label_w,
        );
        cv.text(
            font,
            14.0 * s,
            lx + label_w + code_gap,
            ry,
            &value,
            th.text,
            th.text_alpha,
            value_w + 4.0,
        );
        ry += row_h + code_gap;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clipboard_metrics_change_row_pitch_and_preview_pixels() {
        let font = Font::try_from_vec(
            std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf").unwrap(),
        )
        .unwrap();
        let mut theme = crate::config::Theme::default();
        assert_eq!(
            crate::ui::Layout::clipboard(&theme.metrics).row,
            crate::ui::CLIP_ROW
        );
        theme.metrics.item_content_height += 20.0;
        assert_eq!(
            crate::ui::Layout::clipboard(&theme.metrics).row,
            crate::ui::CLIP_ROW + 20.0
        );
        let mut original = Canvas::new(320, 350);
        let mut custom = Canvas::new(320, 350);
        draw_color_preview(
            &mut original,
            &font,
            1.0,
            (0.0, 0.0, 320.0, 350.0),
            [255, 0, 0, 255],
            theme,
        );
        theme.metrics.clipboard_color_swatch_size = 64.0;
        theme.metrics.clipboard_color_preview_gap = 35.0;
        theme.metrics.clipboard_color_code_gap = 14.0;
        theme.metrics.clipboard_color_label_width = 45.0;
        draw_color_preview(
            &mut custom,
            &font,
            1.0,
            (0.0, 0.0, 320.0, 350.0),
            [255, 0, 0, 255],
            theme,
        );
        let red_pixels = |cv: &Canvas| {
            cv.buf
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[2] > 200 && p[1] < 20 && p[0] < 20)
                .count()
        };
        assert!(red_pixels(&original) > 10000);
        assert!(
            (2800..3500).contains(&red_pixels(&custom)),
            "{}",
            red_pixels(&custom)
        );
        assert_ne!(original.buf, custom.buf);
    }

    #[test]
    fn text_wraps_at_words_and_preserves_newlines() {
        let font = Font::try_from_vec(
            std::fs::read("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf").unwrap(),
        )
        .unwrap();
        let width = Canvas::text_width(&font, 14.0, "Hello ");
        assert_eq!(
            wrap(&font, 14.0, "Hello world", width, 4),
            ["Hello", "world"]
        );
        assert_eq!(
            wrap(&font, 14.0, "one\ntwo\nthree", 100.0, 2),
            ["one", "two..."]
        );
        assert_eq!(wrap(&font, 14.0, "", 100.0, 2), [""]);
        assert!(wrap(&font, 14.0, "text", 100.0, 0).is_empty());
        assert_eq!(
            wrap(
                &font,
                14.0,
                "éééé",
                Canvas::text_width(&font, 14.0, "éé"),
                4
            ),
            ["éé", "éé"]
        );
    }

    #[test]
    fn image_fits_without_changing_aspect_ratio() {
        assert_eq!(fit(400, 200, 100, 100), (100, 50));
        assert_eq!(fit(200, 400, 100, 100), (50, 100));
        assert_eq!(fit(1, 8192, 100, 100), (1, 100));
        assert_eq!(fit(0, 10, 100, 100), (0, 0));
        assert_eq!(fit(10, 10, 0, 100), (0, 0));
    }

    fn png() -> Vec<u8> {
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 2, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[255, 0, 0, 255, 0, 255, 0, 128])
                .unwrap();
        }
        bytes
    }

    #[test]
    fn preview_decodes_and_blends_alpha() {
        let image = decode(&png(), 2, 1).unwrap();
        assert_eq!((image.w, image.h), (2, 1));
        assert_eq!(image.pixels, [255, 0, 0, 255, 0, 128, 0, 128]);
        let mut canvas = Canvas::new(2, 1);
        canvas.buf.fill(255);
        draw_image(&mut canvas, &image, 0, 0);
        assert_eq!(canvas.buf, [0, 0, 255, 255, 127, 255, 127, 255]);
        assert!(decode(b"broken", 10, 10).is_none());
        assert!(decode(&png(), 0, 10).is_none());
    }

    #[test]
    fn selected_image_is_cached_until_size_changes() {
        let image = Img {
            id: 1,
            png: Some(png()),
            path: std::path::PathBuf::new(),
            w: 2,
            h: 1,
            at: 0,
        };
        let a = image_preview(&image, 10, 10).unwrap();
        let b = image_preview(&image, 10, 10).unwrap();
        assert!(Rc::ptr_eq(&a, &b));
        let c = image_preview(&image, 20, 10).unwrap();
        assert!(!Rc::ptr_eq(&a, &c));
        assert_eq!((c.w, c.h), (20, 10));
    }
}
