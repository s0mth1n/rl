//! CPU drawing: a premultiplied ARGB canvas, rounded rectangles and text.
use crate::config::{Rgb, Theme};
use crate::icons::{Cache, Icon};
use crate::items::Item;
use rusttype::{point, Font, Scale};

pub const HEAD: f32 = 60.0;
pub const ROW: f32 = 48.0;
pub const PAD: f32 = 10.0;

pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub buf: Vec<u8>,
}

#[allow(clippy::too_many_arguments)]
impl Canvas {
    pub fn new(w: u32, h: u32) -> Self {
        Canvas {
            w,
            h,
            buf: vec![0; (w * h * 4) as usize],
        }
    }

    #[inline]
    fn blend(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        if x < 0 || y < 0 || x >= self.w as i32 || y >= self.h as i32 || a <= 0.0 {
            return;
        }
        let o = ((y as u32 * self.w + x as u32) * 4) as usize;
        let k = 1.0 - a;
        // Argb8888 little endian: B G R A, premultiplied.
        self.buf[o] = (c[2] as f32 * a + self.buf[o] as f32 * k) as u8;
        self.buf[o + 1] = (c[1] as f32 * a + self.buf[o + 1] as f32 * k) as u8;
        self.buf[o + 2] = (c[0] as f32 * a + self.buf[o + 2] as f32 * k) as u8;
        self.buf[o + 3] = (255.0 * a + self.buf[o + 3] as f32 * k) as u8;
    }

    /// Filled rounded rectangle with a soft edge. Coordinates are physical pixels.
    pub fn rrect(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgb, alpha: f32) {
        let (x0, y0, x1, y1) = (
            (x.floor() as i32).max(0),
            (y.floor() as i32).max(0),
            ((x + w).ceil() as i32).min(self.w as i32),
            ((y + h).ceil() as i32).min(self.h as i32),
        );
        for py in y0..y1 {
            for px in x0..x1 {
                let (fx, fy) = (px as f32 + 0.5 - x, py as f32 + 0.5 - y);
                let dx = (r - fx).max(fx - (w - r)).max(0.0);
                let dy = (r - fy).max(fy - (h - r)).max(0.0);
                let d = (dx * dx + dy * dy).sqrt();
                let cov = (r - d + 0.5)
                    .clamp(0.0, 1.0)
                    .min((fx + 0.5).clamp(0.0, 1.0))
                    .min((w - fx + 0.5).clamp(0.0, 1.0))
                    .min((fy + 0.5).clamp(0.0, 1.0))
                    .min((h - fy + 0.5).clamp(0.0, 1.0));
                self.blend(px, py, c, cov * alpha);
            }
        }
    }

    /// Rounded outline `t` pixels thick.
    pub fn rrect_outline(
        &mut self,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        r: f32,
        t: f32,
        c: Rgb,
        alpha: f32,
    ) {
        let (x0, y0, x1, y1) = (
            (x.floor() as i32).max(0),
            (y.floor() as i32).max(0),
            ((x + w).ceil() as i32).min(self.w as i32),
            ((y + h).ceil() as i32).min(self.h as i32),
        );
        for py in y0..y1 {
            for px in x0..x1 {
                let (fx, fy) = (px as f32 + 0.5 - x, py as f32 + 0.5 - y);
                let dx = (r - fx).max(fx - (w - r)).max(0.0);
                let dy = (r - fy).max(fy - (h - r)).max(0.0);
                let d = (dx * dx + dy * dy).sqrt();
                let outer = (r - d + 0.5).clamp(0.0, 1.0);
                let inner = (d - (r - t) + 0.5).clamp(0.0, 1.0);
                let edge = if dx > 0.0 || dy > 0.0 {
                    outer * inner
                } else {
                    let m = fx.min(w - fx).min(fy).min(h - fy);
                    (t - m + 0.5).clamp(0.0, 1.0)
                };
                self.blend(px, py, c, edge * alpha);
            }
        }
    }

    /// Draw a premultiplied icon with its top left at (x, y).
    pub fn icon(&mut self, x: i32, y: i32, ic: &Icon) {
        let n = ic.size as i32;
        for iy in 0..n {
            let py = y + iy;
            if py < 0 || py >= self.h as i32 {
                continue;
            }
            for ix in 0..n {
                let px = x + ix;
                if px < 0 || px >= self.w as i32 {
                    continue;
                }
                let so = ((iy * n + ix) * 4) as usize;
                let sa = ic.px[so + 3] as u32;
                if sa == 0 {
                    continue;
                }
                let o = ((py as u32 * self.w + px as u32) * 4) as usize;
                let k = 255 - sa;
                self.buf[o] = (ic.px[so + 2] as u32 + self.buf[o] as u32 * k / 255).min(255) as u8;
                self.buf[o + 1] =
                    (ic.px[so + 1] as u32 + self.buf[o + 1] as u32 * k / 255).min(255) as u8;
                self.buf[o + 2] =
                    (ic.px[so] as u32 + self.buf[o + 2] as u32 * k / 255).min(255) as u8;
                self.buf[o + 3] = (sa + self.buf[o + 3] as u32 * k / 255).min(255) as u8;
            }
        }
    }

    pub fn text_width(font: &Font, size: f32, s: &str) -> f32 {
        let sc = Scale::uniform(size);
        font.layout(s, sc, point(0.0, 0.0)).last().map_or(0.0, |g| {
            g.position().x + g.unpositioned().h_metrics().advance_width
        })
    }

    /// Draw `s` with its top at `y`, cut to `max_w` with an ellipsis. Returns the drawn width.
    pub fn text(
        &mut self,
        font: &Font,
        size: f32,
        x: f32,
        y: f32,
        s: &str,
        c: Rgb,
        max_w: f32,
    ) -> f32 {
        let sc = Scale::uniform(size);
        let asc = font.v_metrics(sc).ascent;
        let mut text = s.to_string();
        if Self::text_width(font, size, &text) > max_w {
            while !text.is_empty() && Self::text_width(font, size, &format!("{text}...")) > max_w {
                text.pop();
            }
            text.push_str("...");
        }
        let mut end = x;
        for g in font.layout(&text, sc, point(x, y + asc)) {
            end = g.position().x + g.unpositioned().h_metrics().advance_width;
            if let Some(bb) = g.pixel_bounding_box() {
                g.draw(|gx, gy, v| self.blend(bb.min.x + gx as i32, bb.min.y + gy as i32, c, v));
            }
        }
        end - x
    }
}

pub struct View<'a> {
    pub query: &'a str,
    pub items: &'a [Item],
    pub sel: usize,
    pub top: usize,
    pub rows: usize,
    pub theme: Theme,
    pub w: u32,
    pub h: u32,
    pub scale: u32,
    pub font_size: f32,
    pub icons: &'a Cache,
}

pub fn icon_px(scale: u32) -> u32 {
    32 * scale
}

pub fn panel_height(rows: usize) -> u32 {
    (HEAD + rows.max(1) as f32 * ROW + PAD) as u32
}

pub fn draw(cv: &mut Canvas, font: &Font, v: &View) {
    let s = v.scale as f32;
    let th = &v.theme;
    let (w, h) = (v.w as f32 * s, v.h as f32 * s);
    cv.rrect(0.0, 0.0, w, h, 12.0 * s, th.bg, 0.98);
    cv.rrect_outline(0.0, 0.0, w, h, 12.0 * s, 1.0 * s, th.border, 1.0);
    // search field
    let fs = v.font_size * s * 1.2;
    if v.query.is_empty() {
        cv.text(
            font,
            fs,
            24.0 * s,
            17.0 * s,
            "Search applications - : emoji - ; clipboard - settings",
            th.dim,
            w - 48.0 * s,
        );
    } else {
        let tw = cv.text(font, fs, 24.0 * s, 17.0 * s, v.query, th.text, w - 60.0 * s);
        cv.rrect(
            24.0 * s + tw + 2.0 * s,
            17.0 * s,
            1.5 * s,
            fs * 1.15,
            0.5 * s,
            th.accent,
            1.0,
        );
    }
    cv.rrect(0.0, (HEAD - 1.0) * s, w, 1.0 * s, 0.0, th.border, 1.0);
    if v.items.is_empty() {
        cv.text(
            font,
            v.font_size * s,
            24.0 * s,
            (HEAD + 14.0) * s,
            "No results",
            th.dim,
            w - 48.0 * s,
        );
        return;
    }
    for r in 0..v.rows {
        let i = v.top + r;
        let Some(it) = v.items.get(i) else { break };
        let y = (HEAD + r as f32 * ROW) * s;
        if i == v.sel {
            cv.rrect(
                8.0 * s,
                y + 2.0 * s,
                w - 16.0 * s,
                (ROW - 4.0) * s,
                8.0 * s,
                th.select,
                1.0,
            );
        }
        let badge = if i == v.sel { th.accent } else { th.dim };
        let icon = it
            .icon
            .as_deref()
            .and_then(|n| v.icons.get(n, icon_px(v.scale)));
        if let Some(ic) = icon {
            cv.icon((20.0 * s) as i32, (y + 8.0 * s) as i32, ic);
        } else {
            cv.rrect(
                20.0 * s,
                y + 8.0 * s,
                32.0 * s,
                32.0 * s,
                8.0 * s,
                badge,
                0.18,
            );
            let initial: String = it
                .title
                .chars()
                .find(|c| c.is_alphanumeric())
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default();
            let iw = Canvas::text_width(font, 15.0 * s, &initial);
            cv.text(
                font,
                15.0 * s,
                36.0 * s - iw / 2.0,
                y + 14.0 * s,
                &initial,
                badge,
                40.0 * s,
            );
        }
        let tag_w = Canvas::text_width(font, 11.0 * s, it.tag);
        let max_t = w - 64.0 * s - tag_w - 40.0 * s;
        if it.sub.is_empty() {
            cv.text(
                font,
                v.font_size * s,
                64.0 * s,
                y + 13.0 * s,
                &it.title,
                th.text,
                max_t,
            );
        } else {
            cv.text(
                font,
                v.font_size * s,
                64.0 * s,
                y + 6.0 * s,
                &it.title,
                th.text,
                max_t,
            );
            cv.text(
                font,
                11.5 * s,
                64.0 * s,
                y + 27.0 * s,
                &it.sub,
                th.dim,
                max_t,
            );
        }
        cv.text(
            font,
            11.0 * s,
            w - 20.0 * s - tag_w,
            y + 17.0 * s,
            it.tag,
            th.dim,
            tag_w + 4.0,
        );
    }
    // scroll hint
    if v.items.len() > v.rows {
        let track = v.rows as f32 * ROW * s - 12.0 * s;
        let th_h = (track * v.rows as f32 / v.items.len() as f32).max(18.0 * s);
        let ty =
            HEAD * s + 6.0 * s + (track - th_h) * (v.top as f32 / (v.items.len() - v.rows) as f32);
        cv.rrect(w - 5.0 * s, ty, 2.5 * s, th_h, 1.2 * s, th.dim, 0.5);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn height_follows_rows() {
        assert_eq!(panel_height(0), panel_height(1));
        assert!(panel_height(8) > panel_height(2));
    }
    #[test]
    fn rrect_clips_to_canvas() {
        let mut c = Canvas::new(20, 20);
        c.rrect(-5.0, -5.0, 100.0, 100.0, 4.0, [255, 0, 0], 1.0);
        assert_eq!(c.buf[((10 * 20 + 10) * 4 + 2) as usize], 255);
        assert_eq!(c.buf.len(), 20 * 20 * 4);
    }
}
