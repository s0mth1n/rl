//! Clipboard list and preview, using zlaunch's split-pane layout.
use crate::clip::{History, Img};
use crate::items::Action;
use crate::ui::{Canvas, View, HEAD, LIST_PAD, ROW};
use rusttype::{Font, Scale};
use std::cell::RefCell;
use std::rc::Rc;

struct Preview {
    w: u32,
    h: u32,
    pixels: Vec<u8>,
}

type CachedPreview = (u64, u32, u32, Option<Rc<Preview>>);
thread_local! {
    static IMAGE: RefCell<Option<CachedPreview>> = const { RefCell::new(None) };
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
        let result = decode(&image.png, w, h).map(Rc::new);
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
    let th = v.theme;
    let w = v.w as f32 * s;
    let h = v.h as f32 * s;
    let split = w * 0.5;
    let fs = v.font_size * s;
    let rows = (v.vp / ROW).floor().max(1.0) as usize;
    cv.rrect(
        0.0,
        0.0,
        w,
        h,
        th.window_border_radius * s,
        th.bg,
        th.bg_alpha,
    );
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
                v.theme.dim,
                v.theme.dim_alpha,
            );
        }
    }
    let text_x = if v.back { crate::ui::INPUT_X } else { 24.0 };
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
        if query.is_empty() { th.dim } else { th.text },
        if query.is_empty() {
            th.dim_alpha
        } else {
            th.text_alpha
        },
        w - 48.0 * s,
    );
    cv.rrect(0.0, (HEAD - 1.0) * s, w, s, 0.0, th.border, th.border_alpha);
    cv.rrect(
        split,
        HEAD * s,
        s,
        (h - HEAD * s - s).max(0.0),
        0.0,
        th.border,
        th.border_alpha,
    );
    for (row, item) in v.items.iter().skip(v.top).take(rows).enumerate() {
        let y = (HEAD + LIST_PAD + row as f32 * ROW) * s;
        let selected = row + v.top == v.sel;
        if selected {
            cv.rrect(
                0.0,
                y + s,
                split - 8.0 * s,
                (ROW - 2.0) * s,
                6.0 * s,
                th.select,
                th.select_alpha,
            );
        }
        cv.rrect(
            8.0 * s,
            y + 12.0 * s,
            24.0 * s,
            24.0 * s,
            4.0 * s,
            th.icon_placeholder_background.rgb,
            th.icon_placeholder_background.alpha,
        );
        let icon = match item.action {
            Action::ClipImg(_) => "▧",
            Action::ClearClip => "×",
            _ => "≡",
        };
        cv.text(
            font,
            18.0 * s,
            13.0 * s,
            y + 13.0 * s,
            icon,
            th.dim,
            th.dim_alpha,
            18.0 * s,
        );
        let removable = selected && matches!(item.action, Action::Clip(_) | Action::ClipImg(_));
        let max_w = (split - if removable { 116.0 * s } else { 56.0 * s }).max(0.0);
        if removable {
            cv.rrect(
                split - 70.0 * s,
                y + 16.0 * s,
                56.0 * s,
                17.0 * s,
                3.0 * s,
                th.key_background.rgb,
                th.key_background.alpha,
            );
            cv.rrect_outline(
                split - 70.0 * s,
                y + 16.0 * s,
                56.0 * s,
                17.0 * s,
                3.0 * s,
                s,
                th.key_border.rgb,
                th.key_border.alpha,
            );
            cv.text(
                font,
                10.0 * s,
                split - 66.0 * s,
                y + 18.0 * s,
                "Shift Del",
                th.dim,
                th.dim_alpha,
                50.0 * s,
            );
        }
        cv.text(
            font,
            fs.min(14.0 * s),
            40.0 * s,
            y + 7.0 * s,
            &item.title,
            th.text,
            th.text_alpha,
            max_w,
        );
        cv.text(
            font,
            12.0 * s,
            40.0 * s,
            y + 25.0 * s,
            &item.sub,
            th.dim,
            th.dim_alpha,
            max_w,
        );
    }
    if v.items.is_empty() {
        cv.text(
            font,
            14.0 * s,
            16.0 * s,
            (HEAD + 18.0) * s,
            "No clipboard history",
            th.dim,
            th.dim_alpha,
            split - 32.0 * s,
        );
    }
    if v.items.len() > rows {
        let track = (rows as f32 * ROW - 12.0) * s;
        let thumb = (track * rows as f32 / v.items.len() as f32)
            .max(18.0 * s)
            .min(track);
        let y = (HEAD + 6.0) * s + (track - thumb) * v.top as f32 / (v.items.len() - rows) as f32;
        cv.rrect(split - 4.0 * s, y, 2.0 * s, thumb, s, th.dim, 0.5);
    }
    let x = split + 17.0 * s;
    let y = (HEAD + LIST_PAD + 16.0) * s;
    let pw = (w - x - 16.0 * s).max(0.0);
    let ph = (h - y - 16.0 * s).max(0.0);
    let action = v.items.get(v.sel).map(|item| &item.action);
    if let Some(Action::ClipImg(id)) = action {
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
            cv.text(
                font,
                14.0 * s,
                x,
                y,
                "Preview unavailable",
                th.dim,
                th.dim_alpha,
                pw,
            );
        }
    } else {
        IMAGE.with(|slot| *slot.borrow_mut() = None);
        let text = match action {
            Some(Action::Clip(text)) => text.as_str(),
            Some(Action::ClearClip) => "Remove all saved text and images.",
            _ => "No selection",
        };
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

#[cfg(test)]
mod tests {
    use super::*;

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
            png: png(),
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
