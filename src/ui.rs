//! CPU drawing: a premultiplied ARGB canvas, rounded rectangles and text.
use crate::config::{Rgb, Theme};
use crate::icons::{Cache, Icon};
use crate::items::{Action, Item};
use rusttype::{point, Font, Scale};

// Layout metrics follow zlaunch (MIT, see NOTICE): 600x400 panel, 12 px corner
// radius, 1 px border, 57 px header (12 + 32 input + 12 + 1 separator), 8 px
// list padding, 50 px rows, 24 px icons, 14 px titles and input text.
pub const HEAD: f32 = 58.0;
#[allow(dead_code)]
pub const ROW: f32 = 50.0;
#[allow(dead_code)]
pub const HDR: f32 = 28.0;
#[cfg(test)]
pub const ICON: f32 = 24.0;
pub const LIST_PAD: f32 = 8.0;
/// Clipboard rows in the 0.5.1 release: 58px pitch, list origin HEAD + 5.
pub const CLIP_ROW: f32 = 58.0;
pub const CLIP_TOP: f32 = 5.0;
/// Query text origin in the header; the search icon box sits at (22, 20.5), drawn by the icon code.
pub const INPUT_X: f32 = 49.7;
/// Right edge the query text must clear (room for the clear button).
pub const INPUT_RIGHT: f32 = 90.0;
/// Input row colors come from zlaunch's pinned gpui-component Default Dark
/// theme, not from the launcher theme, so they stay constant across themes
/// (zlaunch daemon/theme.rs overrides only background and borders): query
/// text and caret #fafafa, placeholder and header icons #737373.
pub(crate) const INPUT_FG: Rgb = [250, 250, 250];
pub(crate) const INPUT_DIM: Rgb = [115, 115, 115];
/// Input selection measured from the reference launcher's Default Dark input.
const INPUT_SELECTION: Rgb = [17, 31, 73];
/// Header separator: gpui theme.border, which zlaunch overrides to white at
/// 10% for every theme (daemon/theme.rs configure_theme).
pub(crate) const SEPARATOR: Rgb = [255, 255, 255];
pub(crate) const SEPARATOR_ALPHA: f32 = 0.1;

pub struct Canvas {
    pub w: u32,
    pub h: u32,
    pub buf: Vec<u8>,
    /// Pixels at or below this y are never painted; `i32::MAX` disables the
    /// clip. The list sets it to the viewport bottom so a partial row is cut
    /// off mid-row, like zlaunch's overflow-hidden vlist.
    clip_y: i32,
    clip_top: i32,
}

#[allow(clippy::too_many_arguments)]
impl Canvas {
    pub fn new(w: u32, h: u32) -> Self {
        Canvas {
            w,
            h,
            buf: vec![0; (w * h * 4) as usize],
            clip_y: i32::MAX,
            clip_top: 0,
        }
    }

    /// Clip all painting to y < `max_y` (physical pixels).
    pub fn set_clip_y(&mut self, max_y: i32) {
        self.clip_y = max_y;
    }

    pub fn set_clip_top(&mut self, min_y: i32) {
        self.clip_top = min_y;
    }

    pub fn clear_clip(&mut self) {
        self.clip_y = i32::MAX;
        self.clip_top = 0;
    }

    #[inline]
    fn blend(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        if x < 0
            || y < 0
            || x >= self.w as i32
            || y >= self.h as i32
            || y >= self.clip_y
            || y < self.clip_top
            || a <= 0.0
        {
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

    /// Gaussian box shadow like CSS/gpui: `blur` is the CSS blur radius (sigma = blur / 2),
    /// `spread` grows or shrinks the rectangle, the offset is vertical. Corners follow the
    /// separable profile, which is close enough for small radii.
    #[allow(clippy::too_many_arguments)]
    pub fn shadow(
        &mut self,
        (x, y, w, h): (f32, f32, f32, f32),
        offset_y: f32,
        blur: f32,
        spread: f32,
        alpha: f32,
    ) {
        // Abramowitz & Stegun constants carry more digits than f32 holds.
        #[allow(clippy::excessive_precision)]
        fn erf(x: f32) -> f32 {
            let t = 1.0 / (1.0 + 0.3275911 * x.abs());
            let poly = t
                * (0.254829592
                    + t * (-0.284496736
                        + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
            let y = 1.0 - poly * (-x * x).exp();
            if x >= 0.0 {
                y
            } else {
                -y
            }
        }
        let sigma = (blur / 2.0).max(0.01);
        let k = sigma * std::f32::consts::SQRT_2;
        let (x0, x1) = (x - spread, x + w + spread);
        let (y0, y1) = (y + offset_y - spread, y + offset_y + h + spread);
        let reach = blur + 1.0;
        let (px0, px1) = (
            ((x0 - reach).floor() as i32).max(0),
            ((x1 + reach).ceil() as i32).min(self.w as i32),
        );
        let (py0, py1) = (
            ((y0 - reach).floor() as i32).max(0),
            ((y1 + reach).ceil() as i32).min(self.h as i32),
        );
        let horizontal: Vec<f32> = (px0..px1)
            .map(|px| {
                let fx = px as f32 + 0.5;
                0.5 * (erf((fx - x0) / k) - erf((fx - x1) / k))
            })
            .collect();
        for py in py0..py1 {
            let fy = py as f32 + 0.5;
            let ay = 0.5 * (erf((fy - y0) / k) - erf((fy - y1) / k));
            for (px, ax) in (px0..px1).zip(&horizontal) {
                let a = alpha * ax * ay;
                if a > 0.002 {
                    self.blend(px, py, [0, 0, 0], a);
                }
            }
        }
    }

    #[cfg(test)]
    fn shadow_scalar(
        &mut self,
        (x, y, w, h): (f32, f32, f32, f32),
        offset_y: f32,
        blur: f32,
        spread: f32,
        alpha: f32,
    ) {
        // Abramowitz & Stegun constants carry more digits than f32 holds.
        #[allow(clippy::excessive_precision)]
        fn erf(x: f32) -> f32 {
            let t = 1.0 / (1.0 + 0.3275911 * x.abs());
            let poly = t
                * (0.254829592
                    + t * (-0.284496736
                        + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
            let y = 1.0 - poly * (-x * x).exp();
            if x >= 0.0 {
                y
            } else {
                -y
            }
        }
        let sigma = (blur / 2.0).max(0.01);
        let k = sigma * std::f32::consts::SQRT_2;
        let (x0, x1) = (x - spread, x + w + spread);
        let (y0, y1) = (y + offset_y - spread, y + offset_y + h + spread);
        let reach = blur + 1.0;
        let (px0, px1) = (
            ((x0 - reach).floor() as i32).max(0),
            ((x1 + reach).ceil() as i32).min(self.w as i32),
        );
        let (py0, py1) = (
            ((y0 - reach).floor() as i32).max(0),
            ((y1 + reach).ceil() as i32).min(self.h as i32),
        );
        for py in py0..py1 {
            let fy = py as f32 + 0.5;
            let ay = 0.5 * (erf((fy - y0) / k) - erf((fy - y1) / k));
            for px in px0..px1 {
                let fx = px as f32 + 0.5;
                let ax = 0.5 * (erf((fx - x0) / k) - erf((fx - x1) / k));
                let a = alpha * ax * ay;
                if a > 0.002 {
                    self.blend(px, py, [0, 0, 0], a);
                }
            }
        }
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
                if r >= 0.5 && fx >= r && fx <= w - r && fy >= r && fy <= h - r {
                    self.blend(px, py, c, alpha);
                    continue;
                }
                let dx = (r - fx).max(fx - (w - r)).max(0.0);
                let dy = (r - fy).max(fy - (h - r)).max(0.0);
                let d = (dx * dx + dy * dy).sqrt();
                // A pixel whose center sits inside the shape (d == 0) is fully
                // covered; without this, r < 0.5 rects - the square ones like
                // the caret and separators - painted every pixel half strength.
                let arc = if r >= 0.0 && d == 0.0 {
                    1.0
                } else {
                    (r - d + 0.5).clamp(0.0, 1.0)
                };
                let cov = arc
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
        let inset = r.max(t + 0.5);
        let mut interior = (x0..x1).filter(|&px| {
            let fx = px as f32 + 0.5 - x;
            fx >= inset && fx <= w - inset
        });
        let left = interior.clone().next().unwrap_or(x1);
        let right = interior.next_back().map_or(x1, |px| px + 1);
        for py in y0..y1 {
            let fy = py as f32 + 0.5 - y;
            let middle = fy >= inset && fy <= h - inset;
            let edges =
                (x0..if middle { left } else { x1 }).chain((if middle { right } else { x1 })..x1);
            for px in edges {
                let fx = px as f32 + 0.5 - x;
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

    #[cfg(test)]
    fn rrect_outline_scalar(
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
                let inset = r.max(t + 0.5);
                if fx >= inset && fx <= w - inset && fy >= inset && fy <= h - inset {
                    continue;
                }
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
        self.icon_rounded(x, y, ic, 0.0);
    }

    pub fn icon_rounded(&mut self, x: i32, y: i32, ic: &Icon, radius: f32) {
        let n = ic.size as i32;
        let radius = radius.clamp(0.0, ic.size as f32 / 2.0);
        for iy in 0..n {
            let py = y + iy;
            if py < 0 || py >= self.h as i32 || py < self.clip_top || py >= self.clip_y {
                continue;
            }
            for ix in 0..n {
                let px = x + ix;
                if px < 0 || px >= self.w as i32 {
                    continue;
                }
                let so = ((iy * n + ix) * 4) as usize;
                let coverage = if radius > 0.0 {
                    let fx = ix as f32 + 0.5;
                    let fy = iy as f32 + 0.5;
                    let dx = (radius - fx).max(fx - (n as f32 - radius)).max(0.0);
                    let dy = (radius - fy).max(fy - (n as f32 - radius)).max(0.0);
                    (radius - dx.hypot(dy) + 0.5).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                let src = |offset| (ic.px[so + offset] as f32 * coverage).round() as u32;
                let sa = src(3);
                if sa == 0 {
                    continue;
                }
                let o = ((py as u32 * self.w + px as u32) * 4) as usize;
                let k = 255 - sa;
                self.buf[o] = (src(2) + self.buf[o] as u32 * k / 255).min(255) as u8;
                self.buf[o + 1] = (src(1) + self.buf[o + 1] as u32 * k / 255).min(255) as u8;
                self.buf[o + 2] = (src(0) + self.buf[o + 2] as u32 * k / 255).min(255) as u8;
                self.buf[o + 3] = (sa + self.buf[o + 3] as u32 * k / 255).min(255) as u8;
            }
        }
    }

    pub fn symbol(&mut self, x: i32, y: i32, icon: &Icon, color: Rgb, alpha: f32) {
        for iy in 0..icon.size as i32 {
            for ix in 0..icon.size as i32 {
                let a = icon.px[((iy * icon.size as i32 + ix) * 4 + 3) as usize];
                self.blend(x + ix, y + iy, color, a as f32 / 255.0 * alpha);
            }
        }
    }

    pub fn text_width(font: &Font, size: f32, s: &str) -> f32 {
        let sc = Scale::uniform(size);
        font.layout(s, sc, point(0.0, 0.0)).last().map_or(0.0, |g| {
            g.position().x + g.unpositioned().h_metrics().advance_width
        })
    }

    /// Draw `s` with its top at `y` and colour at `alpha`, cut to `max_w` with an
    /// ellipsis. Returns the drawn width.
    #[allow(clippy::too_many_arguments)]
    pub fn text(
        &mut self,
        font: &Font,
        size: f32,
        x: f32,
        y: f32,
        s: &str,
        c: Rgb,
        alpha: f32,
        max_w: f32,
    ) -> f32 {
        let sc = Scale::uniform(size);
        let asc = font.v_metrics(sc).ascent;
        let mut text = s.to_string();
        if Self::text_width(font, size, &text) > max_w {
            while !text.is_empty() && Self::text_width(font, size, &format!("{text}…")) > max_w {
                text.pop();
            }
            text.push('…');
        }
        let mut end = x;
        for g in font.layout(&text, sc, point(x, y + asc)) {
            end = g.position().x + g.unpositioned().h_metrics().advance_width;
            if let Some(bb) = g.pixel_bounding_box() {
                g.draw(|gx, gy, v| {
                    self.blend(bb.min.x + gx as i32, bb.min.y + gy as i32, c, v * alpha)
                });
            }
        }
        end - x
    }
}

fn lighten(c: Rgb, amount: f32) -> Rgb {
    let c = c.map(|v| v as f32 / 255.0);
    let hi = c.iter().copied().fold(0.0, f32::max);
    let lo = c.iter().copied().fold(1.0, f32::min);
    let light = (hi + lo) / 2.0;
    let next = (light + amount).min(1.0);
    let chroma = hi - lo;
    if chroma == 0.0 {
        return [(next * 255.0).round() as u8; 3];
    }
    let saturation = chroma / (1.0 - (2.0 * light - 1.0).abs());
    let new_chroma = (1.0 - (2.0 * next - 1.0).abs()) * saturation;
    c.map(|v| (((v - lo) / chroma * new_chroma + next - new_chroma / 2.0) * 255.0).round() as u8)
}

/// Theme preview icon, like zlaunch's theme rows: the window background holding
/// the title, selection and border colours.
fn theme_icon(cv: &mut Canvas, x: f32, y: f32, size: f32, th: &Theme) {
    let inner = size * 0.35;
    let gap = size * 0.08;
    let pad = size * 0.15;
    let r = size * 0.12;
    cv.rrect(
        x,
        y,
        size,
        size,
        size / 4.0,
        lighten(th.bg, 0.05),
        th.bg_alpha,
    );
    cv.rrect(x + pad, y + pad, inner, inner, r, th.text, th.text_alpha);
    cv.rrect(
        x + pad + inner + gap,
        y + pad,
        inner,
        inner,
        r,
        lighten(th.select, 0.1),
        th.select_alpha,
    );
    let bar_y = y + pad + inner + gap;
    cv.rrect(
        x + pad,
        bar_y,
        size - 2.0 * pad,
        y + size - pad - bar_y,
        r,
        th.border,
        th.border_alpha,
    );
}

pub struct View<'a> {
    pub theme_palettes: Option<&'a std::collections::HashMap<String, Theme>>,
    pub query: &'a str,
    pub literal_prefix: bool,
    pub placeholder: Option<&'a str>,
    pub input_text: Option<&'a str>,
    pub marked: Option<std::ops::Range<usize>>,
    pub input: Option<(usize, Option<usize>, bool)>,
    pub items: &'a [Item],
    pub sel: usize,
    pub top: usize,
    pub scroll_remainder: f32,
    /// List viewport height in unscaled pixels, from `viewport`.
    pub vp: f32,
    pub theme: Theme,
    pub w: u32,
    pub h: u32,
    pub scale: u32,
    pub font_size: f32,
    pub mono_font_family: Option<&'a str>,
    pub icons: &'a Cache,
    /// Bold face for section headers (zlaunch styles them EXTRA_BOLD); falls
    /// back to the regular face when no bold font was found.
    pub bold_font: Option<&'a Font<'a>>,
    /// The sub-view was entered from the main list, so the input prefix is a
    /// clickable back arrow instead of the magnifier (zlaunch mode origin).
    pub back: bool,
    pub scrollbar: crate::scroll::Style,
}
impl View<'_> {
    #[allow(dead_code)]
    pub fn mono_font_family(&self) -> Option<&str> {
        self.mono_font_family
    }
}

pub fn icon_px(theme: &Theme, scale: u32) -> u32 {
    (theme.metrics.icon_size * scale as f32).round().max(1.0) as u32
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub row: f32,
    pub header: f32,
    pub cell: f32,
    pub gap: f32,
    pub columns: usize,
}

impl Default for Layout {
    fn default() -> Self {
        Self::new(&crate::config::ThemeMetrics::default())
    }
}

impl Layout {
    pub fn new(m: &crate::config::ThemeMetrics) -> Self {
        Self {
            row: (2.0 * (m.item_margin_y + m.item_padding_y)
                + m.item_content_height.max(m.icon_size))
            .max(1.0),
            header: (16.0 + m.section_header_margin_top + m.section_header_margin_bottom).max(1.0),
            cell: m.emoji_cell_size.max(1.0),
            gap: m.emoji_cell_gap,
            columns: m.emoji_columns.max(1),
        }
    }
    pub fn clipboard(metrics: &crate::config::ThemeMetrics) -> Self {
        let mut layout = Self::new(metrics);
        layout.row = (CLIP_ROW + layout.row - Self::default().row).max(1.0);
        layout
    }
    fn visible_emoji_rows(&self) -> usize {
        (LIST_VP / self.cell).floor().max(1.0) as usize
    }
    pub fn slots(&self, items: &[Item], top: usize, vp: f32) -> Vec<Slot> {
        let headers = show_headers(items);
        let mut out = Vec::new();
        let mut y = 0.0;
        for (i, item) in items.iter().enumerate().skip(top) {
            let sec = section_of(item);
            let starts = i == 0 || sec != section_of(&items[i - 1]);
            if headers && starts {
                if let Some(name) = sec {
                    if y + self.header >= vp {
                        break;
                    }
                    out.push(Slot {
                        y,
                        h: self.header,
                        item: None,
                        header: name,
                    });
                    y += self.header;
                }
            }
            if y >= vp {
                break;
            }
            out.push(Slot {
                y,
                h: self.row.min(vp - y),
                item: Some(i),
                header: "",
            });
            y += self.row;
        }
        out
    }
    pub fn max_top(&self, items: &[Item], vp: f32) -> usize {
        let last = items.len().saturating_sub(1);
        let (mut lo, mut hi) = (0, items.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            if self
                .slots(items, mid, vp)
                .last()
                .is_some_and(|s| s.item == Some(last))
            {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }
        if lo == items.len() {
            0
        } else {
            lo
        }
    }
    pub fn item_at(&self, items: &[Item], top: usize, vp: f32, y: f32) -> Option<usize> {
        self.slots(items, top, vp)
            .into_iter()
            .find(|s| y >= s.y && y < s.y + s.h)
            .and_then(|s| s.item)
    }
    pub fn emoji_columns(&self, width: u32) -> usize {
        (((width as f32 - 16.0 + self.gap) / (self.cell + self.gap)) as usize)
            .clamp(1, self.columns)
    }
    pub fn emoji_rows(&self, count: usize, width: u32) -> usize {
        count
            .div_ceil(self.emoji_columns(width))
            .clamp(1, self.visible_emoji_rows())
    }
    pub fn emoji_hit(
        &self,
        width: u32,
        rows: usize,
        top: usize,
        count: usize,
        x: f32,
        y: f32,
    ) -> Option<usize> {
        let columns = self.emoji_columns(width);
        let grid_width = columns as f32 * (self.cell + self.gap) - self.gap;
        let x = x - (width as f32 - grid_width) / 2.0;
        let y = y - HEAD - EMOJI_PAD;
        if x < 0.0 || y < 0.0 || x >= grid_width || y >= rows as f32 * self.cell {
            return None;
        }
        let col = (x / (self.cell + self.gap)) as usize;
        if x % (self.cell + self.gap) >= self.cell {
            return None;
        }
        let index = top + (y / self.cell) as usize * columns + col;
        (index < count).then_some(index)
    }
    pub fn scroll_offset(&self, items: &[Item], top: usize, clipboard: bool) -> f32 {
        let mut offset = top.min(items.len()) as f32 * self.row;
        if !clipboard && show_headers(items) {
            for (i, item) in items.iter().take(top).enumerate() {
                if section_of(item).is_some()
                    && (i == 0 || section_of(item) != section_of(&items[i - 1]))
                {
                    offset += self.header;
                }
            }
        }
        offset
    }
    pub fn scroll_item_y(&self, items: &[Item], index: usize, clipboard: bool) -> f32 {
        let header = !clipboard
            && show_headers(items)
            && items.get(index).is_some_and(|item| {
                section_of(item).is_some()
                    && (index == 0 || section_of(item) != section_of(&items[index - 1]))
            });
        self.scroll_offset(items, index, clipboard) + if header { self.header } else { 0.0 }
    }
    pub fn scroll_geometry(
        &self,
        items: &[Item],
        query: &str,
        width: u32,
        vp: f32,
        top: usize,
        remainder: f32,
    ) -> Option<crate::scroll::Geometry> {
        let (content, offset) = if query.starts_with(':') {
            let columns = self.emoji_columns(width);
            (
                items.len().div_ceil(columns) as f32 * self.cell,
                (top / columns) as f32 * self.cell,
            )
        } else {
            let clipboard = query.starts_with(';');
            (
                self.scroll_offset(items, items.len(), clipboard),
                self.scroll_offset(items, top, clipboard),
            )
        };
        crate::scroll::Geometry::new(vp, content, offset + remainder)
    }
}

/// Section an item is listed under, if its kind has one.
pub fn section_of(it: &Item) -> Option<&'static str> {
    if it.best {
        return Some("Best Match");
    }
    match it.tag {
        "Application" => Some("Applications"),
        "Window" => Some("Windows"),
        "Basic" | "System" => Some("Commands"),
        "Calculator" => Some("Calculator"),
        "AI" | "Search" => Some("Search and AI"),
        _ => None,
    }
}

/// Headers are shown only when the list has more than one section.
pub fn show_headers(items: &[Item]) -> bool {
    let mut first = None;
    for it in items {
        if let Some(sec) = section_of(it) {
            match first {
                None => first = Some(sec),
                Some(f) if f != sec => return true,
                _ => {}
            }
        }
    }
    false
}

/// One drawn entry of the list: a section header or an item row.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Slot {
    /// Offset from the top of the list, unscaled pixels.
    pub y: f32,
    pub h: f32,
    pub item: Option<usize>,
    pub header: &'static str,
}

/// Entries at least partly visible in `vp` pixels when the list starts at
/// item `top`. The last row may be clipped partway through, like zlaunch's
/// vlist; its `h` is the visible part only, so hit tests and the scrollbar
/// only count the visible region. Headers are kept whole: a header is shown
/// only when it and at least a sliver of the row below it fit.
#[allow(dead_code)]
pub fn slots(items: &[Item], top: usize, vp: f32) -> Vec<Slot> {
    Layout::default().slots(items, top, vp)
}

/// Fixed panel height: zlaunch sizes every panel to `launcher_size`
/// (default 600x400) instead of to its content (config/types.rs, render.rs).
pub const PANEL_HEIGHT: u32 = 400;
/// Fixed height of the list viewport inside the 400 px panel.
pub const LIST_VP: f32 = PANEL_HEIGHT as f32 - HEAD - LIST_PAD * 2.0;
/// Emoji grid rows that fit in the fixed panel.
#[allow(dead_code)]
pub const EMOJI_ROWS: usize = 5; // (400 - HEAD - 2*EMOJI_PAD) / EMOJI_CELL

/// Largest first item that still leaves no gap below the last row.
#[allow(dead_code)]
pub fn max_top(items: &[Item], vp: f32) -> usize {
    Layout::default().max_top(items, vp)
}

/// First visible item so that `sel` is fully shown, moving `top` as little as possible.
/// Item under list-relative y (unscaled), if any.
#[allow(dead_code)]
pub fn item_at(items: &[Item], top: usize, vp: f32, y: f32) -> Option<usize> {
    Layout::default().item_at(items, top, vp, y)
}

fn action_label(a: &Action) -> &'static str {
    match a {
        Action::App(_) | Action::WebSearch(_) => "Open",
        Action::System(_) => "Run",
        Action::Calculate(_) => "Copy",
        Action::Window(_) => "Switch",
        Action::Clip(_) | Action::ClipFiles(_) | Action::ClipImg(_) => "Copy",
        Action::View(_) => "→",
        Action::Theme(_) => "Apply",
    }
}

/// Draw `s` centred in a line box of height `lh` whose top is `y`.
#[allow(clippy::too_many_arguments)]
fn text_lh(
    cv: &mut Canvas,
    font: &Font,
    size: f32,
    x: f32,
    y: f32,
    lh: f32,
    s: &str,
    c: Rgb,
    alpha: f32,
    max_w: f32,
) {
    let vm = font.v_metrics(Scale::uniform(size));
    let gap = (lh - (vm.ascent - vm.descent)) / 2.0;
    cv.text(font, size, x, y + gap, s, c, alpha, max_w);
}

/// Small "return" key glyph: a box with an arrow, right edge at `right`. Returns its width.
fn enter_key(cv: &mut Canvas, font: &Font, s: f32, right: f32, mid: f32, th: &Theme) -> f32 {
    let m = th.metrics;
    let fs = m.action_indicator_key_font_size * s;
    let lh = m.action_indicator_key_line_height * s;
    let px = m.action_indicator_key_padding_x * s;
    let pt = m.action_indicator_key_padding_top * s;
    let pb = m.action_indicator_key_padding_bottom * s;
    let radius = m.action_indicator_key_border_radius * s;
    let (kw, kh) = (
        Canvas::text_width(font, fs, "↵") + 2.0 * px + 2.0 * s,
        lh + pt + pb + 2.0 * s,
    );
    let (x, y) = (right - kw, mid - kh / 2.0);
    let bg = th.key_background;
    cv.rrect(x, y, kw, kh, radius, bg.rgb, bg.alpha);
    let bd = th.key_border;
    cv.rrect_outline(x, y, kw, kh, radius, s, bd.rgb, bd.alpha);
    let kc = th.key_color;
    text_lh(
        cv,
        font,
        fs,
        x + px + s,
        y + pt + s,
        lh,
        "↵",
        kc.rgb,
        kc.alpha,
        kw,
    );
    kw
}

#[allow(dead_code)]
pub const EMOJI_CELL: f32 = 64.0;
#[allow(dead_code)]
pub const EMOJI_GAP: f32 = 2.0;
pub const EMOJI_PAD: f32 = 8.0;

#[allow(dead_code)]
pub fn emoji_columns(width: u32) -> usize {
    Layout::default().emoji_columns(width)
}

#[allow(dead_code)]
pub fn emoji_rows(count: usize, width: u32) -> usize {
    Layout::default().emoji_rows(count, width)
}

#[allow(dead_code)]
pub fn emoji_icon_px(scale: u32) -> u32 {
    28 * scale
}

#[allow(dead_code)]
pub fn emoji_hit(
    width: u32,
    rows: usize,
    top: usize,
    count: usize,
    x: f32,
    y: f32,
) -> Option<usize> {
    Layout::default().emoji_hit(width, rows, top, count, x, y)
}

#[cfg(test)]
pub fn emoji_horizontal(selected: usize, count: usize, right: bool) -> usize {
    if count == 0 {
        return 0;
    }
    if right {
        (selected + 1) % count
    } else if selected == 0 {
        count - 1
    } else {
        selected - 1
    }
}

pub fn emoji_vertical(selected: usize, count: usize, columns: usize, down: bool) -> usize {
    if count == 0 {
        return 0;
    }
    if down {
        if selected + columns < count {
            selected + columns
        } else {
            selected % columns
        }
    } else if selected >= columns {
        selected - columns
    } else {
        let last = (count - 1) / columns * columns + selected;
        if last < count {
            last
        } else {
            last.saturating_sub(columns)
        }
    }
}

fn draw_emojis(cv: &mut Canvas, font: &Font, v: &View) {
    let s = v.scale as f32;
    let layout = Layout::new(&v.theme.metrics);
    let columns = layout.emoji_columns(v.w);
    let grid_width = columns as f32 * (layout.cell + layout.gap) - layout.gap;
    let left = (v.w as f32 - grid_width) / 2.0;
    if v.items.is_empty() {
        // zlaunch's render_empty: the message is centred in a 200px-high box
        // at the top of the list area, in empty_state_color.
        let es = v.font_size * s;
        let tw = Canvas::text_width(font, es, "No emojis found");
        let lh = (v.font_size * (1.0 + 5.0_f32.sqrt()) / 2.0).round() * s;
        text_lh(
            cv,
            font,
            es,
            (v.w as f32 * s - tw) / 2.0,
            (HEAD + LIST_PAD) * s + (v.theme.metrics.empty_state_height * s - lh) / 2.0,
            lh,
            "No emojis found",
            v.theme.empty_state_color.rgb,
            v.theme.empty_state_color.alpha,
            v.w as f32 * s,
        );
    }
    // Rows that start above the viewport bottom are drawn and clipped
    // mid-row, like zlaunch's overflow-hidden emoji vlist.
    cv.set_clip_top(((HEAD + LIST_PAD) * s) as i32);
    cv.set_clip_y(((HEAD + LIST_PAD + v.vp) * s) as i32);
    for (offset, item) in v
        .items
        .iter()
        .skip(v.top)
        .take(((v.vp + v.scroll_remainder) / layout.cell).ceil() as usize * columns)
        .enumerate()
    {
        let x = (left + (offset % columns) as f32 * (layout.cell + layout.gap)) * s;
        let y =
            (HEAD + EMOJI_PAD + (offset / columns) as f32 * layout.cell - v.scroll_remainder) * s;
        if v.top + offset == v.sel {
            cv.rrect(
                x,
                y,
                layout.cell * s,
                layout.cell * s,
                v.theme.metrics.emoji_cell_border_radius * s,
                v.theme.emoji_selected.rgb,
                v.theme.emoji_selected.alpha,
            );
        }
        if let Some(icon) = item.icon.as_deref().and_then(|name| {
            v.icons.get(
                name,
                (v.theme.metrics.emoji_font_size * s).round().max(1.0) as u32,
            )
        }) {
            let inset = (layout.cell * s - icon.size as f32) / 2.0;
            cv.icon((x + inset) as i32, (y + inset) as i32, icon);
        } else if let Some(glyph) = item
            .icon
            .as_deref()
            .and_then(|name| name.strip_prefix("emoji:"))
        {
            let size = v.theme.metrics.emoji_font_size * s;
            let width = Canvas::text_width(font, size, glyph);
            let lh = (v.theme.metrics.emoji_font_size * (1.0 + 5.0_f32.sqrt()) / 2.0).round() * s;
            text_lh(
                cv,
                font,
                size,
                x + (layout.cell * s - width) / 2.0,
                y + (layout.cell * s - lh) / 2.0,
                lh,
                glyph,
                v.theme.text,
                v.theme.text_alpha,
                layout.cell * s,
            );
        }
    }
    cv.clear_clip();
}

impl View<'_> {
    fn view_query(&self) -> &str {
        if self.literal_prefix {
            ""
        } else {
            self.query
        }
    }
}

/// Hit zone for the back arrow in the input prefix, shown when a sub-view
/// was entered from the main list.
pub fn back_button_at(x: f64, y: f64) -> bool {
    (14.0..40.0).contains(&x) && y >= (HEAD as f64 - 24.0) / 2.0 && y < (HEAD as f64 + 24.0) / 2.0
}

#[cfg(test)]
pub fn clear_button_at(width: u32, query: &str, x: f64, y: f64) -> bool {
    clear_button_at_mode(width, query, x, y, false)
}

pub fn clear_button_at_mode(width: u32, query: &str, x: f64, y: f64, literal: bool) -> bool {
    !(if literal {
        query
    } else {
        query.strip_prefix([':', ';', '#']).unwrap_or(query)
    })
    .is_empty()
        && x >= width as f64 - 44.0
        && x < width as f64 - 20.0
        && y >= (HEAD as f64 - 24.0) / 2.0
        && y < (HEAD as f64 + 24.0) / 2.0
}

fn input_tail<'a>(font: &Font, size: f32, query: &'a str, max_w: f32) -> &'a str {
    let width = Canvas::text_width(font, size, query);
    if width <= max_w {
        return query;
    }
    for ((index, _), glyph) in
        query
            .char_indices()
            .zip(font.layout(query, Scale::uniform(size), point(0.0, 0.0)))
    {
        if glyph.position().x >= width - max_w {
            return &query[index..];
        }
    }
    ""
}

fn input_start(font: &Font, size: f32, query: &str, caret: usize, max_w: f32) -> usize {
    let head = &query[..caret];
    let tail = input_tail(font, size, head, max_w);
    head.len() - tail.len()
}

pub fn input_at_mode(
    font: &Font,
    query: &str,
    caret: usize,
    width: u32,
    x: f64,
    literal: bool,
) -> usize {
    let shown = if literal {
        query
    } else {
        query
            .strip_prefix([':', ';', '#'])
            .map(str::trim_start)
            .unwrap_or(query)
    };
    let prefix = query.len() - shown.len();
    let caret = caret.saturating_sub(prefix).min(shown.len());
    let start = input_start(
        font,
        14.0,
        shown,
        caret,
        (width as f32 - INPUT_X - INPUT_RIGHT).max(0.0),
    );
    let x = x as f32 - INPUT_X;
    let mut previous = 0.0;
    for (i, ch) in shown[start..].char_indices() {
        let next = Canvas::text_width(font, 14.0, &shown[start..start + i + ch.len_utf8()]);
        if x < (previous + next) / 2.0 {
            return prefix + start + i;
        }
        previous = next;
    }
    query.len()
}

fn input_visible_text<'a>(query: &str, text: &'a str) -> &'a str {
    if query.starts_with([':', ';']) {
        text[1..].trim_start()
    } else if query.starts_with('#') {
        &text[1..]
    } else {
        text
    }
}

pub fn input_cursor_rectangle(
    font: &Font,
    query: &str,
    text: &str,
    caret: usize,
    width: u32,
) -> (i32, i32, i32, i32) {
    let shown = input_visible_text(query, text);
    let prefix = text.len() - shown.len();
    let caret = caret.saturating_sub(prefix).min(shown.len());
    let start = input_start(
        font,
        16.0,
        shown,
        caret,
        (width as f32 - INPUT_X - INPUT_RIGHT).max(0.0),
    );
    let x = INPUT_X + Canvas::text_width(font, 16.0, &shown[start..caret]);
    (
        x.round() as i32,
        ((HEAD - 16.0) / 2.0).round() as i32,
        2,
        17,
    )
}

fn draw_input(cv: &mut Canvas, font: &Font, v: &View) {
    draw_input_with_placeholder(cv, font, v, None);
}

pub fn draw_input_with_placeholder(
    cv: &mut Canvas,
    font: &Font,
    v: &View,
    override_placeholder: Option<&str>,
) {
    let s = v.scale as f32;
    let w = v.w as f32 * s;
    let fs = 16.0 * s;
    let y = (HEAD * s - fs) / 2.0;
    // Menu-entered sub-views show a back arrow; the theme picker entered
    // directly shows a palette, as zlaunch's theme mode does.
    let prefix_icon = if v.back {
        crate::icons::BACK
    } else if v.view_query().trim_start().starts_with('#') {
        crate::icons::PALETTE
    } else {
        match v.placeholder {
            Some("Search windows...") => crate::icons::DASHBOARD,
            Some("Search actions...") => crate::icons::MODE_ACTIONS,
            Some("Search the web...") => crate::icons::MODE_SEARCH,
            _ => crate::icons::SEARCH,
        }
    };
    if let Some(icon) = v.icons.get(prefix_icon, 16 * v.scale) {
        cv.symbol(
            (22.0 * s) as i32,
            ((HEAD - 16.0) * 0.5 * s) as i32,
            icon,
            INPUT_DIM,
            1.0,
        );
    }
    let x = INPUT_X * s;
    let max_w = (w - x - INPUT_RIGHT * s).max(0.0);
    let input_text = v.input_text.unwrap_or(v.query);
    let placeholder = if v.view_query().starts_with(':') {
        "Search emojis..."
    } else if v.view_query().starts_with('#') {
        "Search themes..."
    } else if v.view_query().starts_with(';') {
        "Search clipboard..."
    } else {
        v.placeholder.unwrap_or("Search anything...")
    };
    let query = input_visible_text(v.view_query(), input_text);
    let placeholder = override_placeholder.unwrap_or(placeholder);
    let prefix = input_text.len() - query.len();
    let (caret, anchor, visible) = v.input.unwrap_or((v.query.len(), None, true));
    let caret = caret.saturating_sub(prefix).min(query.len());
    let start = input_start(font, fs, query, caret, max_w);
    let mut end = start;
    for (i, ch) in query[start..].char_indices() {
        if Canvas::text_width(font, fs, &query[start..start + i + ch.len_utf8()]) > max_w {
            break;
        }
        end = start + i + ch.len_utf8();
    }
    if let Some(anchor) = anchor {
        let anchor = anchor.saturating_sub(prefix).min(query.len());
        let lo = caret.min(anchor).max(start);
        let hi = caret.max(anchor).min(end);
        if lo < hi {
            let left = Canvas::text_width(font, fs, &query[start..lo]);
            let width = Canvas::text_width(font, fs, &query[lo..hi]);
            cv.rrect(x + left, y, width, 17.0 * s, 0.0, INPUT_SELECTION, 1.0);
        }
    }
    if query.is_empty() {
        cv.text(font, fs, x, y, placeholder, INPUT_DIM, 1.0, max_w);
    } else {
        cv.text(font, fs, x, y, &query[start..end], INPUT_FG, 1.0, max_w);
    }
    if let Some(marked) = &v.marked {
        let lo = marked.start.saturating_sub(prefix).max(start).min(end);
        let hi = marked.end.saturating_sub(prefix).min(end).max(lo);
        if lo < hi {
            let left = Canvas::text_width(font, fs, &query[start..lo]);
            let width = Canvas::text_width(font, fs, &query[lo..hi]);
            cv.rrect(x + left, y + 17.0 * s, width, s, 0.0, INPUT_FG, 1.0);
        }
    }
    if visible && anchor.is_none_or(|a| a == caret + prefix) {
        let cx = Canvas::text_width(font, fs, &query[start..caret]);
        cv.rrect(x + cx, y, 2.0 * s, 17.0 * s, 0.0, INPUT_FG, 1.0);
    }
    // zlaunch's Input cleanable(true) shows CircleX whenever text is present;
    // a bare mode prefix (':', ';') alone is not text.
    if !query.is_empty() {
        if let Some(icon) = v.icons.get(crate::icons::CLEAR, 16 * v.scale) {
            cv.symbol(
                (w - 40.0 * s) as i32,
                ((HEAD - 16.0) * 0.5 * s) as i32,
                icon,
                INPUT_DIM,
                1.0,
            );
        }
    }
}

pub fn draw(cv: &mut Canvas, font: &Font, v: &View) {
    let layout = Layout::new(&v.theme.metrics);
    let s = v.scale as f32;
    let th = &v.theme;
    let (w, h) = (v.w as f32 * s, v.h as f32 * s);
    let wr = th.window_border_radius * s;
    cv.rrect(0.0, 0.0, w, h, wr, th.bg, th.bg_alpha);
    cv.rrect_outline(0.0, 0.0, w, h, wr, s, th.border, th.border_alpha);
    draw_input(cv, font, v);
    cv.rrect(0.0, (HEAD - 1.0) * s, w, s, 0.0, SEPARATOR, SEPARATOR_ALPHA);
    if v.view_query().starts_with(':') {
        draw_emojis(cv, font, v);
        draw_scrollbar(cv, v);
        return;
    }
    if v.items.is_empty() {
        let es = v.font_size * s;
        let label = if crate::items::theme_view(v.view_query()) {
            "No themes found"
        } else {
            "No items found"
        };
        let tw = Canvas::text_width(font, es, label);
        let ec = th.empty_state_color;
        cv.text(
            font,
            es,
            (w - tw) / 2.0,
            (HEAD + LIST_PAD + v.theme.metrics.empty_state_height / 2.0 - 10.0) * s,
            label,
            ec.rgb,
            ec.alpha,
            w,
        );
        return;
    }
    let title_px = 14.0 * s;
    let m = th.metrics;
    // Clip rows at the viewport bottom so a partial row is cut mid-row,
    // like zlaunch's overflow-hidden vlist.
    cv.set_clip_top(((HEAD + LIST_PAD) * s) as i32);
    cv.set_clip_y(((HEAD + LIST_PAD + v.vp) * s) as i32);
    for sl in layout.slots(v.items, v.top, v.vp + v.scroll_remainder) {
        let y = (HEAD + LIST_PAD + sl.y - v.scroll_remainder) * s;
        let Some(i) = sl.item else {
            // section header: EXTRA_BOLD in zlaunch; 8px above, 4px below, 16px in
            text_lh(
                cv,
                v.bold_font.unwrap_or(font),
                12.0 * s,
                (1.0 + m.item_margin_x + m.item_padding_x) * s,
                y + m.section_header_margin_top * s,
                16.0 * s,
                sl.header,
                th.section_header_color.rgb,
                th.section_header_color.alpha,
                w - 32.0 * s,
            );
            continue;
        };
        let it = &v.items[i];
        let selected = i == v.sel;
        // item box: 1px margin, 8px side margin, 6px corners; zlaunch draws the
        // themed background on every row, the selected one with the select colour
        let (bg, bg_alpha) = if selected {
            (th.select, th.select_alpha)
        } else {
            (th.item_background.rgb, th.item_background.alpha)
        };
        if bg_alpha > 0.0 {
            cv.rrect(
                (1.0 + m.item_margin_x) * s,
                y + m.item_margin_y * s,
                w - 2.0 * (1.0 + m.item_margin_x) * s,
                (layout.row - 2.0 * m.item_margin_y) * s,
                m.item_border_radius * s,
                bg,
                bg_alpha,
            );
        }
        let icon_size = m.icon_size * s;
        let (ix, iy) = (
            (1.0 + m.item_margin_x + m.item_padding_x) * s,
            y + (layout.row * s - icon_size) / 2.0,
        );
        let icon_name = it.icon.as_deref();
        if matches!(it.action, Action::Calculate(_)) {
            let bg = th.calculator_background;
            cv.rrect(ix, iy, icon_size, icon_size, 4.0 * s, bg.rgb, bg.alpha);
            let color = th.calculator_icon;
            text_lh(
                cv,
                font,
                14.0 * s,
                ix + 6.0 * s,
                iy,
                icon_size,
                "=",
                color.rgb,
                color.alpha,
                icon_size,
            );
        } else if let Some(name) = icon_name.and_then(|n| n.strip_prefix("theme:")) {
            let palette = v
                .theme_palettes
                .and_then(|palettes| palettes.get(name))
                .copied()
                .unwrap_or_else(|| crate::config::theme(name));
            theme_icon(cv, ix, iy, icon_size, &palette);
        } else if let Some(ic) = icon_name.and_then(|n| v.icons.get(n, icon_px(th, v.scale))) {
            if ic.symbol {
                let pb = th.icon_placeholder_background;
                cv.rrect(
                    ix,
                    iy,
                    icon_size,
                    icon_size,
                    m.icon_border_radius * s,
                    pb.rgb,
                    pb.alpha,
                );
                let pc = th.icon_placeholder_color;
                cv.symbol(ix as i32, iy as i32, ic, pc.rgb, pc.alpha);
            } else {
                cv.icon_rounded(ix as i32, iy as i32, ic, m.icon_border_radius * s);
            }
        } else {
            let pb = th.icon_placeholder_background;
            cv.rrect(
                ix,
                iy,
                icon_size,
                icon_size,
                m.icon_border_radius * s,
                pb.rgb,
                pb.alpha,
            );
            let initial = "?";
            let iw = Canvas::text_width(font, 14.0 * s, initial);
            let pc = th.icon_placeholder_color;
            text_lh(
                cv,
                font,
                14.0 * s,
                ix + icon_size / 2.0 - iw / 2.0,
                iy,
                icon_size,
                initial,
                pc.rgb,
                pc.alpha,
                40.0 * s,
            );
        }
        // trailing action hint, shown on the selected row only
        let mut right = w
            - (1.0
                + th.metrics.item_margin_x
                + th.metrics.item_padding_x
                + th.metrics.action_indicator_right_position)
                * s;
        if selected {
            let label = action_label(&it.action);
            let kw = enter_key(cv, font, s, right, y + layout.row / 2.0 * s, th);
            let lw = Canvas::text_width(font, 12.0 * s, label);
            text_lh(
                cv,
                font,
                12.0 * s,
                right - kw - 8.0 * s - lw,
                y + (layout.row - 16.0) / 2.0 * s,
                16.0 * s,
                label,
                th.action_label.rgb,
                th.action_label.alpha,
                lw + 4.0,
            );
            right -= kw + 8.0 * s + lw + 8.0 * s;
        }
        let tx = ix + icon_size + 8.0 * s;
        let max_t = right - tx - 8.0 * s;
        if let Action::Calculate(result) = &it.action {
            let (color, alpha) = if result.is_error {
                (th.calculator_error.rgb, th.calculator_error.alpha)
            } else {
                (th.text, th.text_alpha)
            };
            text_lh(
                cv,
                font,
                title_px,
                tx,
                y + (ROW - 16.0) / 2.0 * s,
                16.0 * s,
                &it.title,
                color,
                alpha,
                max_t,
            );
        } else if it.sub.is_empty() {
            text_lh(
                cv,
                font,
                title_px,
                tx,
                y + (layout.row - m.item_title_line_height) / 2.0 * s,
                m.item_title_line_height * s,
                &it.title,
                th.text,
                th.text_alpha,
                max_t,
            );
        } else {
            text_lh(
                cv,
                font,
                title_px,
                tx,
                y + (layout.row - m.item_content_height) / 2.0 * s,
                m.item_title_line_height * s,
                &it.title,
                th.text,
                th.text_alpha,
                max_t,
            );
            text_lh(
                cv,
                font,
                12.0 * s,
                tx,
                y + ((layout.row - m.item_content_height) / 2.0 + m.item_title_line_height) * s,
                m.layout_item_description_height * s,
                &it.sub,
                th.dim,
                th.dim_alpha,
                max_t,
            );
        }
    }
    cv.clear_clip();
    draw_scrollbar(cv, v);
}

#[allow(dead_code)]
pub fn scroll_offset(items: &[Item], top: usize, clipboard: bool) -> f32 {
    Layout::default().scroll_offset(items, top, clipboard)
}

#[allow(dead_code)]
pub fn scroll_item_y(items: &[Item], index: usize, clipboard: bool) -> f32 {
    Layout::default().scroll_item_y(items, index, clipboard)
}

#[allow(dead_code)]
pub fn scroll_geometry(
    items: &[Item],
    query: &str,
    width: u32,
    vp: f32,
    top: usize,
    remainder: f32,
) -> Option<crate::scroll::Geometry> {
    let layout = if query.starts_with(';') {
        Layout::clipboard(&crate::config::ThemeMetrics::default())
    } else {
        Layout::default()
    };
    layout.scroll_geometry(items, query, width, vp, top, remainder)
}

pub fn draw_scrollbar(cv: &mut Canvas, v: &View) {
    if v.scrollbar.alpha <= 0.0 {
        return;
    }
    let layout = if v.view_query().starts_with(';') {
        Layout::clipboard(&v.theme.metrics)
    } else {
        Layout::new(&v.theme.metrics)
    };
    let Some(g) = layout.scroll_geometry(
        v.items,
        v.view_query(),
        v.w,
        v.vp,
        v.top,
        v.scroll_remainder,
    ) else {
        return;
    };
    let s = v.scale as f32;
    let right = if v.view_query().starts_with(';') {
        v.w as f32 * 0.5
    } else {
        v.w as f32
    };
    let wide = v.scrollbar.active || v.scrollbar.hovered;
    let width = if wide { 8.0 } else { 6.0 };
    let color = [82, 82, 82];
    let alpha = if v.scrollbar.active { 1.0 } else { 0.9 } * v.scrollbar.alpha;
    cv.rrect(
        (right - 4.0 - width) * s,
        (HEAD + LIST_PAD + g.start) * s,
        width * s,
        g.length * s,
        width * s / 2.0,
        color,
        alpha,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Rgba;
    fn app(name: &str) -> Item {
        Item {
            title: name.into(),
            sub: String::new(),
            tag: "Application",
            icon: None,
            action: Action::App(0),
            best: false,
        }
    }
    fn sectioned(n: usize) -> Vec<Item> {
        let mut v: Vec<Item> = (0..n).map(|i| app(&format!("a{i}"))).collect();
        v[0].best = true;
        v
    }

    /// DejaVu Sans, the font examples/render.rs uses. None where it is not installed.
    fn test_font() -> Option<Font<'static>> {
        [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        ]
        .iter()
        .find_map(|p| std::fs::read(p).ok())
        .and_then(Font::try_from_vec)
    }

    /// Pixel at (x, y) as premultiplied B, G, R, A.
    fn px(cv: &Canvas, x: u32, y: u32) -> [u8; 4] {
        let o = ((y * cv.w + x) * 4) as usize;
        [cv.buf[o], cv.buf[o + 1], cv.buf[o + 2], cv.buf[o + 3]]
    }

    /// A 640 px panel with `items`, `sel` selected and query "a", drawn with `th`.
    fn drawn(font: &Font, items: &[Item], sel: usize, th: Theme) -> Canvas {
        let mut icons = Cache::new("", "");
        icons.ensure_input(1);
        let vp = LIST_VP;
        let h = PANEL_HEIGHT;
        let mut cv = Canvas::new(640, h);
        draw(
            &mut cv,
            font,
            &View {
                theme_palettes: None,
                literal_prefix: false,
                query: "a",
                placeholder: None,
                input_text: None,
                marked: None,
                input: None,
                items,
                sel,
                top: 0,
                scroll_remainder: 0.0,
                vp,
                theme: th,
                w: 640,
                h,
                scale: 1,
                font_size: 15.0,
                mono_font_family: None,
                icons: &icons,
                bold_font: None,
                back: false,
                scrollbar: crate::scroll::Style::visible(),
            },
        );
        cv
    }

    /// Premultiplied blend of `c` at `a` over a buffer byte, as Canvas::blend does.
    fn over(c: u8, a: f32, buf: u8) -> u8 {
        (c as f32 * a + buf as f32 * (1.0 - a)) as u8
    }

    /// Pixels in the region brighter than the panel background.
    fn lit(cv: &Canvas, x0: u32, x1: u32, y0: u32, y1: u32) -> usize {
        (x0..x1)
            .flat_map(|x| (y0..y1).map(move |y| (x, y)))
            .filter(|&(x, y)| {
                let p = px(cv, x, y);
                p[0] as u32 + p[1] as u32 + p[2] as u32 > 150
            })
            .count()
    }

    /// A solidly covered pixel of the given exact color exists in the region.
    fn has_solid(cv: &Canvas, x0: u32, x1: u32, y0: u32, y1: u32, want: [u8; 4]) -> bool {
        (x0..x1)
            .flat_map(|x| (y0..y1).map(move |y| (x, y)))
            .any(|(x, y)| px(cv, x, y) == want)
    }

    #[test]
    fn separable_shadow_matches_scalar_pixels_at_fractional_offsets() {
        for (x, y, w, h) in [(20.0, 20.0, 133.0, 104.0), (-3.25, 4.5, 51.2, 47.1)] {
            for (offset, blur, spread, alpha) in [
                (10.0, 3.0, 0.0, 0.1),
                (4.0, 1.2, -1.0, 0.1),
                (-1.0, 0.0, 2.0, 0.75),
            ] {
                let mut fast = Canvas::new(180, 160);
                let mut scalar = Canvas::new(180, 160);
                fast.buf.fill(135);
                scalar.buf.fill(135);
                fast.set_clip_top(3);
                scalar.set_clip_top(3);
                fast.set_clip_y(140);
                scalar.set_clip_y(140);
                fast.shadow((x, y, w, h), offset, blur, spread, alpha);
                scalar.shadow_scalar((x, y, w, h), offset, blur, spread, alpha);
                assert_eq!(fast.buf, scalar.buf);
            }
        }
    }

    #[test]
    fn outline_skip_matches_scalar_pixels_with_fractional_bounds_and_clips() {
        for (x, y, w, h, r, t) in [
            (0.0, 0.0, 80.0, 60.0, 12.0, 1.0),
            (-4.25, 2.5, 53.2, 44.7, 6.0, 2.25),
            (3.4, -2.0, 5.0, 7.0, 9.0, 1.0),
            (1.0, 1.0, 30.0, 22.0, 0.0, 1.0),
        ] {
            for alpha in [0.0, 0.1, 0.75, 1.0] {
                let mut fast = Canvas::new(80, 60);
                let mut scalar = Canvas::new(80, 60);
                fast.buf.fill(73);
                scalar.buf.fill(73);
                fast.set_clip_top(3);
                scalar.set_clip_top(3);
                fast.set_clip_y(51);
                scalar.set_clip_y(51);
                fast.rrect_outline(x, y, w, h, r, t, [17, 93, 201], alpha);
                scalar.rrect_outline_scalar(x, y, w, h, r, t, [17, 93, 201], alpha);
                assert_eq!(fast.buf, scalar.buf);
            }
        }
    }

    #[test]
    fn preedit_mode_characters_are_visible_until_committed() {
        assert_eq!(input_visible_text("", ":"), ":");
        assert_eq!(input_visible_text("", ";"), ";");
        assert_eq!(input_visible_text("", "#"), "#");
        assert_eq!(input_visible_text(";", "; café"), "café");
    }

    #[test]
    fn input_row_matches_zlaunch_gpui_dark() {
        let Some(font) = test_font() else {
            eprintln!("skip: DejaVu Sans not installed");
            return;
        };
        let items: Vec<Item> = (0..3).map(|i| app(&format!("a{i}"))).collect();
        let cv = drawn(&font, &items, 0, Theme::default());
        // separator: white at 0.1, constant across themes (gpui theme.border)
        assert_eq!(px(&cv, 500, 57), [34, 34, 34, 185]);
        // magnifier at (22, 20.5) and, with a non-empty query, CircleX right
        assert!(lit(&cv, 22, 38, 20, 37) > 5, "magnifier icon drawn");
        assert!(lit(&cv, 600, 616, 20, 37) > 5, "clear button drawn");
        // query text and caret are solid #fafafa, not the theme's dimmed text
        // or accent: 250 over the 10-gray panel at full coverage
        assert!(
            has_solid(&cv, 50, 62, 20, 40, [250, 250, 250, 255]),
            "query"
        );
        assert!(
            has_solid(&cv, 54, 70, 21, 40, [250, 250, 250, 255]),
            "caret"
        );

        // empty query: placeholder in #737373, no clear button
        let mut icons = Cache::new("", "");
        icons.ensure_input(1);
        let vp = LIST_VP;
        let h = PANEL_HEIGHT;
        let mut cv = Canvas::new(640, h);
        draw(
            &mut cv,
            &font,
            &View {
                theme_palettes: None,
                literal_prefix: false,
                query: "",
                placeholder: None,
                input_text: None,
                marked: None,
                input: None,
                back: false,
                scrollbar: crate::scroll::Style::visible(),
                items: &items,
                sel: 0,
                top: 0,
                scroll_remainder: 0.0,
                vp,
                theme: Theme::default(),
                w: 640,
                h,
                scale: 1,
                font_size: 15.0,
                mono_font_family: None,
                icons: &icons,
                bold_font: None,
            },
        );
        assert!(
            has_solid(&cv, 50, 220, 20, 40, [115, 115, 115, 255]),
            "placeholder"
        );
        assert_eq!(lit(&cv, 600, 616, 20, 37), 0, "no clear button");
        assert!(lit(&cv, 22, 38, 20, 37) > 5, "magnifier still drawn");

        // selection: gpui Default Dark selection.background, constant #1d4ed8
        let mut icons = Cache::new("", "");
        icons.ensure_input(1);
        let vp = LIST_VP;
        let h = PANEL_HEIGHT;
        let mut cv = Canvas::new(640, h);
        draw(
            &mut cv,
            &font,
            &View {
                theme_palettes: None,
                literal_prefix: false,
                query: "a",
                placeholder: None,
                input_text: None,
                marked: None,
                input: Some((1, Some(0), true)),
                items: &items,
                sel: 0,
                top: 0,
                scroll_remainder: 0.0,
                vp,
                theme: Theme::default(),
                w: 640,
                h,
                scale: 1,
                font_size: 15.0,
                mono_font_family: None,
                back: false,
                scrollbar: crate::scroll::Style::default(),
                icons: &icons,
                bold_font: None,
            },
        );
        assert!(
            has_solid(&cv, 50, 62, 20, 40, [73, 31, 17, 255]),
            "selection"
        );
    }

    #[test]
    fn icon_corners_and_partial_rows_clip_premultiplied_pixels() {
        let icon = Icon {
            size: 24,
            px: [128, 64, 32, 128].repeat(24 * 24),
            symbol: false,
        };
        let mut cv = Canvas::new(30, 30);
        cv.set_clip_y(15);
        cv.icon_rounded(2, 2, &icon, 4.0);
        assert_eq!(px(&cv, 2, 2), [0; 4]);
        assert_eq!(px(&cv, 14, 14), [32, 64, 128, 128]);
        assert_eq!(px(&cv, 14, 15), [0; 4]);
        assert_eq!(px(&cv, 14, 25), [0; 4]);
        let edge = px(&cv, 3, 2);
        assert!(edge[3] > 0 && edge[3] < 128);
        assert!(edge[0] <= edge[1] && edge[1] <= edge[2] && edge[2] <= edge[3]);
        cv.clear_clip();
        cv.icon_rounded(2, 2, &icon, 100.0);
        assert_ne!(px(&cv, 14, 25), [0; 4]);
        let mut plain = Canvas::new(24, 24);
        plain.icon(0, 0, &icon);
        assert_eq!(px(&plain, 0, 0), [32, 64, 128, 128]);
    }

    #[test]
    fn missing_app_icons_use_the_same_question_mark() {
        let Some(font) = test_font() else { return };
        let a = drawn(&font, &[app("Alpha")], 0, Theme::default());
        let b = drawn(&font, &[app("Beta")], 0, Theme::default());
        for y in 78..102 {
            for x in 16..40 {
                assert_eq!(px(&a, x, y), px(&b, x, y));
            }
        }
    }

    #[test]
    fn text_ellipsis_and_key_box_use_font_glyph_widths() {
        let Some(font) = test_font() else { return };
        let mut actual = Canvas::new(120, 30);
        let mut expected = Canvas::new(120, 30);
        let width = Canvas::text_width(&font, 14.0, "ab…");
        actual.text(&font, 14.0, 0.0, 0.0, "abcdefghijkl", [255; 3], 1.0, width);
        expected.text(&font, 14.0, 0.0, 0.0, "ab…", [255; 3], 1.0, width);
        assert_eq!(actual.buf, expected.buf);
        for scale in [1.0, 2.0, 3.0] {
            let width = enter_key(&mut actual, &font, scale, 100.0, 15.0, &Theme::default());
            assert_eq!(
                width,
                Canvas::text_width(&font, 10.0 * scale, "↵") + 10.0 * scale
            );
        }
    }

    #[test]
    fn action_key_uses_theme_padding_font_line_height_and_radius() {
        let Some(font) = test_font() else { return };
        let mut theme = Theme::default();
        theme.metrics.action_indicator_key_padding_x = 9.0;
        theme.metrics.action_indicator_key_padding_top = 6.0;
        theme.metrics.action_indicator_key_padding_bottom = 4.0;
        theme.metrics.action_indicator_key_font_size = 18.0;
        theme.metrics.action_indicator_key_line_height = 24.0;
        theme.metrics.action_indicator_key_border_radius = 0.0;
        theme.key_background.alpha = 1.0;
        theme.key_background.rgb = [255, 0, 0];
        for scale in [1.0, 2.0] {
            let mut cv = Canvas::new(400, 120);
            let width = enter_key(&mut cv, &font, scale, 150.0 * scale, 30.0 * scale, &theme);
            assert_eq!(
                width,
                Canvas::text_width(&font, 18.0 * scale, "↵") + 20.0 * scale
            );
            let height = 36.0 * scale;
            let left = (150.0 * scale - width).ceil() as u32 + 2;
            let top = (30.0 * scale - height / 2.0).ceil() as u32;
            assert_ne!(px(&cv, left, top + 2), [0; 4]);
            assert_eq!(px(&cv, left, top - 1), [0; 4]);
            assert_eq!(px(&cv, left, (top as f32 + height + 1.0) as u32), [0; 4]);
        }
    }

    #[test]
    fn theme_alpha_reaches_panel_border_and_selected_row() {
        let Some(font) = test_font() else {
            eprintln!("skip: DejaVu Sans not installed");
            return;
        };
        let items: Vec<Item> = (0..3).map(|i| app(&format!("a{i}"))).collect();
        let cv = drawn(&font, &items, 0, Theme::default());
        // panel: gray 15 at 0.7 alpha over a transparent buffer
        assert_eq!(px(&cv, 500, 30), [10, 10, 10, 178]);
        // LauncherTheme.window_border is independent of the transparent GPUI border.
        assert_eq!(px(&cv, 0, 110), [33, 33, 33, 185]);
        // selected row: white at 0.07 over the panel, not solid white
        assert_eq!(px(&cv, 480, 90), [27, 27, 27, 183]);
        // unselected row: item_background is transparent, the panel shows through
        assert_eq!(px(&cv, 480, 140), [10, 10, 10, 178]);
    }

    #[test]
    fn unselected_rows_take_the_theme_item_background() {
        let Some(font) = test_font() else {
            eprintln!("skip: DejaVu Sans not installed");
            return;
        };
        let items: Vec<Item> = (0..3).map(|i| app(&format!("a{i}"))).collect();
        let th = Theme {
            item_background: Rgba {
                rgb: [255; 3],
                alpha: 0.2,
            },
            ..Theme::default()
        };
        let cv = drawn(&font, &items, 0, th);
        let bg = [10, 10, 10, 178];
        let want = [
            over(255, 0.2, bg[0]),
            over(255, 0.2, bg[0]),
            over(255, 0.2, bg[0]),
            over(255, 0.2, bg[3]),
        ];
        assert_eq!(px(&cv, 480, 140), want);
    }

    #[test]
    fn every_bundled_theme_renders_with_its_own_alpha() {
        let Some(font) = test_font() else {
            eprintln!("skip: DejaVu Sans not installed");
            return;
        };
        let items: Vec<Item> = (0..3).map(|i| app(&format!("a{i}"))).collect();
        for name in crate::config::THEMES {
            let th = crate::config::theme(name);
            let cv = drawn(&font, &items, 0, th);
            let ba = th.bg_alpha;
            let panel = [
                (th.bg[2] as f32 * ba) as u8,
                (th.bg[1] as f32 * ba) as u8,
                (th.bg[0] as f32 * ba) as u8,
                (255.0 * ba) as u8,
            ];
            assert_eq!(px(&cv, 500, 30), panel, "{name} panel");
            let sa = th.select_alpha;
            let selected = [
                over(th.select[2], sa, panel[0]),
                over(th.select[1], sa, panel[1]),
                over(th.select[0], sa, panel[2]),
                over(255, sa, panel[3]),
            ];
            assert_eq!(px(&cv, 480, 90), selected, "{name} selected row");
        }
    }

    #[allow(clippy::too_many_arguments)]
    impl Canvas {
        fn outline_reference(
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

        fn rrect_reference(&mut self, x: f32, y: f32, w: f32, h: f32, r: f32, c: Rgb, alpha: f32) {
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
                    let arc = if r >= 0.0 && d == 0.0 {
                        1.0
                    } else {
                        (r - d + 0.5).clamp(0.0, 1.0)
                    };
                    let cov = arc
                        .min((fx + 0.5).clamp(0.0, 1.0))
                        .min((w - fx + 0.5).clamp(0.0, 1.0))
                        .min((fy + 0.5).clamp(0.0, 1.0))
                        .min((h - fy + 0.5).clamp(0.0, 1.0));
                    self.blend(px, py, c, cov * alpha);
                }
            }
        }
    }
    #[test]
    fn outline_interior_matches_reference_pixels() {
        for (x, y, w, h) in [
            (0.0, 0.0, 20.0, 20.0),
            (-5.25, -2.5, 32.0, 25.75),
            (2.25, 3.75, 8.5, 9.25),
            (1.0, 2.0, 0.0, 4.0),
            (1.0, 2.0, -4.0, 4.0),
            (0.25, 0.25, 0.25, 0.25),
        ] {
            for r in [-1.0, 0.0, 0.25, 0.5, 1.0, 4.5, 12.0, 30.0] {
                for t in [-1.0, 0.0, 0.25, 1.0, 2.5, 8.0, 32.0] {
                    for alpha in [0.0, 0.1, 0.7, 0.98, 1.0] {
                        let mut actual = Canvas::new(20, 20);
                        let mut expected = Canvas::new(20, 20);
                        for (i, (a, e)) in actual.buf.iter_mut().zip(&mut expected.buf).enumerate()
                        {
                            *a = (i % 127) as u8;
                            *e = *a;
                        }
                        actual.rrect_outline(x, y, w, h, r, t, [31, 175, 240], alpha);
                        expected.outline_reference(x, y, w, h, r, t, [31, 175, 240], alpha);
                        assert_eq!(actual.buf, expected.buf, "{x} {y} {w} {h} {r} {t} {alpha}");
                    }
                }
            }
        }
    }
    #[test]
    fn rrect_interior_matches_reference_pixels() {
        for (x, y, w, h) in [
            (0.0, 0.0, 20.0, 20.0),
            (-5.25, -2.5, 32.0, 25.75),
            (2.25, 3.75, 8.5, 9.25),
            (1.0, 2.0, 0.0, 4.0),
            (1.0, 2.0, -4.0, 4.0),
            (0.25, 0.25, 0.25, 0.25),
        ] {
            for r in [-1.0, 0.0, 0.25, 0.5, 1.0, 4.5, 12.0, 30.0] {
                for alpha in [0.0, 0.1, 0.7, 0.98, 1.0] {
                    let mut actual = Canvas::new(20, 20);
                    let mut expected = Canvas::new(20, 20);
                    for (i, (a, e)) in actual.buf.iter_mut().zip(&mut expected.buf).enumerate() {
                        *a = (i % 127) as u8;
                        *e = *a;
                    }
                    actual.rrect(x, y, w, h, r, [31, 175, 240], alpha);
                    expected.rrect_reference(x, y, w, h, r, [31, 175, 240], alpha);
                    assert_eq!(actual.buf, expected.buf, "{x} {y} {w} {h} {r} {alpha}");
                }
            }
        }
    }
    #[test]
    fn emoji_grid_geometry_and_hit_testing() {
        assert_eq!(emoji_columns(640), 8);
        assert_eq!(emoji_columns(320), 4);
        assert_eq!(emoji_rows(0, 640), 1);
        assert_eq!(emoji_rows(9, 640), 2);
        assert_eq!(emoji_rows(1000, 640), EMOJI_ROWS);
        let left = (640.0 - (8.0 * 66.0 - 2.0)) / 2.0;
        let y = HEAD + EMOJI_PAD;
        assert_eq!(emoji_hit(640, 2, 0, 9, left + 1.0, y + 1.0), Some(0));
        assert_eq!(emoji_hit(640, 2, 0, 9, left + 65.0, y + 1.0), None);
        assert_eq!(emoji_hit(640, 2, 0, 9, left + 67.0, y + 1.0), Some(1));
        assert_eq!(emoji_hit(640, 2, 0, 9, left + 1.0, y + 65.0), Some(8));
        assert_eq!(emoji_hit(640, 2, 0, 9, left + 67.0, y + 65.0), None);
        assert_eq!(emoji_hit(640, 2, 8, 20, left + 1.0, y + 1.0), Some(8));
        assert_eq!(emoji_hit(640, 2, 0, 9, left - 1.0, y + 1.0), None);
        assert_eq!(emoji_hit(640, 2, 0, 9, left + 1.0, y - 1.0), None);
        assert_eq!(emoji_hit(640, 2, 0, 9, left + 1.0, y + 128.0), None);
    }

    #[test]
    fn emoji_horizontal_wraps_at_the_ends() {
        assert_eq!(emoji_horizontal(0, 0, true), 0);
        assert_eq!(emoji_horizontal(0, 1, false), 0);
        assert_eq!(emoji_horizontal(0, 17, false), 16);
        assert_eq!(emoji_horizontal(16, 17, true), 0);
        assert_eq!(emoji_horizontal(7, 17, true), 8);
        assert_eq!(emoji_horizontal(8, 17, false), 7);
    }

    #[test]
    fn emoji_vertical_wraps_in_the_same_column() {
        assert_eq!(emoji_vertical(0, 0, 8, true), 0);
        assert_eq!(emoji_vertical(0, 1, 8, false), 0);
        assert_eq!(emoji_vertical(0, 17, 8, true), 8);
        assert_eq!(emoji_vertical(16, 17, 8, true), 0);
        assert_eq!(emoji_vertical(15, 17, 8, true), 7);
        assert_eq!(emoji_vertical(0, 17, 8, false), 16);
        assert_eq!(emoji_vertical(7, 17, 8, false), 15);
        assert_eq!(emoji_vertical(15, 17, 8, false), 7);
    }

    #[test]
    fn emoji_vertical_covers_partial_rows_and_narrow_grids() {
        for columns in 1..=8 {
            for count in 1..=40 {
                for selected in 0..count {
                    let same_column: Vec<_> =
                        (selected % columns..count).step_by(columns).collect();
                    let row = same_column.iter().position(|&i| i == selected).unwrap();
                    let next = same_column[(row + 1) % same_column.len()];
                    let prev = same_column[(row + same_column.len() - 1) % same_column.len()];
                    assert_eq!(emoji_vertical(selected, count, columns, true), next);
                    assert_eq!(emoji_vertical(selected, count, columns, false), prev);
                }
            }
        }
    }

    #[test]
    fn long_input_keeps_its_unicode_suffix_and_caret_visible() {
        let _env = crate::testutil::ENV_LOCK.lock().unwrap();
        let out = std::process::Command::new("fc-match")
            .args(["-f", "%{file}", "sans-serif"])
            .output()
            .unwrap();
        let data = std::fs::read(String::from_utf8(out.stdout).unwrap()).unwrap();
        let font = Font::try_from_vec(data).unwrap();
        assert_eq!(input_tail(&font, 14.0, "", 30.0), "");
        assert_eq!(input_tail(&font, 14.0, "zed", 100.0), "zed");
        assert_eq!(input_tail(&font, 14.0, "a", 0.0), "");
        let query = "This is a long query with accented text and emoji café 😀";
        let tail = input_tail(&font, 14.0, query, 90.0);
        assert!(query.ends_with(tail));
        assert!(tail.ends_with("café 😀"));
        assert!(tail.len() < query.len());
        assert!(Canvas::text_width(&font, 14.0, tail) <= 90.0);
    }

    #[test]
    fn clear_button_uses_logical_coordinates_and_needs_a_query() {
        assert!(clear_button_at(640, "zed", 608.0, HEAD as f64 / 2.0));
        assert!(!clear_button_at(640, "", 608.0, HEAD as f64 / 2.0));
        for marker in [":", ";", "#"] {
            assert!(!clear_button_at(640, marker, 608.0, HEAD as f64 / 2.0));
            assert!(clear_button_at(
                640,
                &format!("{marker}x"),
                608.0,
                HEAD as f64 / 2.0
            ));
        }
        assert!(!clear_button_at(640, "zed", 595.0, HEAD as f64 / 2.0));
        assert!(!clear_button_at(640, "zed", 620.0, HEAD as f64 / 2.0));
        assert!(!clear_button_at(640, "zed", 608.0, HEAD as f64));
        assert!(!clear_button_at(640, "zed", f64::NAN, 20.0));
    }

    #[test]
    fn launcher_metrics_match_the_ported_layout() {
        let cfg = crate::config::Config::default();
        assert_eq!(
            (cfg.width, cfg.max_rows, cfg.startup_font_size()),
            (600, 6, 14.0)
        );
        assert_eq!(
            (HEAD, ROW, HDR, ICON, LIST_PAD),
            (58.0, 50.0, 28.0, 24.0, 8.0)
        );
        assert_eq!((INPUT_X, INPUT_RIGHT), (49.7, 90.0));
        // zlaunch's default launcher_size fixes every panel at 600x400.
        assert_eq!(PANEL_HEIGHT, 400);
        assert_eq!(LIST_VP, 326.0);
        assert_eq!(EMOJI_ROWS, 5);
        for scale in 1..=4 {
            assert_eq!(icon_px(&Theme::default(), scale), 24 * scale);
        }
    }
    #[test]
    fn list_hit_tests_exclude_headers_and_clip_partial_rows() {
        let items = sectioned(3);
        let vp = 206.0; // two headers + three rows
        for (y, expected) in [
            (-1.0, None),
            (0.0, None),
            (27.99, None),
            (28.0, Some(0)),
            (77.99, Some(0)),
            (78.0, None),
            (105.99, None),
            (106.0, Some(1)),
            (155.99, Some(1)),
            (156.0, Some(2)),
            (205.99, Some(2)),
            (206.0, None),
        ] {
            assert_eq!(item_at(&items, 0, vp, y), expected, "y={y}");
        }
        // A partially visible row is hit-tested by its visible region only.
        let partial = slots(&items, 0, HDR + ROW - 0.1);
        assert_eq!(partial.len(), 2);
        assert_eq!(partial[0].header, "Best Match");
        assert_eq!(partial[1].item, Some(0));
        assert_eq!(partial[1].h, ROW - 0.1);
        assert_eq!(
            item_at(&items, 0, HDR + ROW - 0.1, HDR + ROW - 0.2),
            Some(0)
        );
        assert_eq!(item_at(&items, 0, HDR + ROW - 0.1, HDR + ROW - 0.05), None);
        let one = slots(&items, 0, HDR + ROW);
        assert_eq!(one.len(), 2);
        assert_eq!(one[0].header, "Best Match");
        assert_eq!(one[1].item, Some(0));
        assert_eq!(one[1].h, ROW);
        assert_eq!(item_at(&items, 0, HDR + ROW, HDR + ROW), None);
        assert!(slots(&[], 0, 200.0).is_empty());
        assert_eq!(item_at(&[], 0, 200.0, 1.0), None);
    }
    #[test]
    fn partial_row_is_clipped_at_the_viewport_bottom() {
        // The fixed 326px viewport shows full rows plus a clipped last row.
        let Some(font) = test_font() else { return };
        let mut icons = Cache::new("", "");
        icons.ensure_input(1);
        let items = sectioned(30);
        let sl = slots(&items, 0, LIST_VP);
        let last = sl.last().unwrap();
        assert_eq!(last.h, 20.0); // 326 - (2*28 + 5*50)
                                  // The 7th row's box and text stop at the viewport bottom (392px).
        let mut cv = Canvas::new(600, PANEL_HEIGHT);
        draw(
            &mut cv,
            &font,
            &View {
                theme_palettes: None,
                literal_prefix: false,
                query: "a",
                placeholder: None,
                input_text: None,
                marked: None,
                input: None,
                items: &items,
                sel: 5,
                top: 0,
                scroll_remainder: 0.0,
                vp: LIST_VP,
                theme: Theme::default(),
                w: 600,
                h: PANEL_HEIGHT,
                scale: 1,
                font_size: 14.0,
                mono_font_family: None,
                icons: &icons,
                bold_font: None,
                back: false,
                scrollbar: crate::scroll::Style::default(),
            },
        );
        // Pixels below the viewport keep the panel background.
        for y in 392..399 {
            assert_eq!(px(&cv, 300, y), px(&cv, 300, 398), "y={y}");
        }
        assert_ne!(px(&cv, 300, 391), px(&cv, 300, 399));
    }
    #[test]
    fn emoji_draws_monochrome_or_missing_glyph_when_bitmap_is_unavailable() {
        let Some(font) = test_font() else { return };
        let icons = Cache::new("", "");
        for glyph in ["😀", "\u{10ffff}"] {
            let mut item = app("emoji");
            item.icon = Some(format!("emoji:{glyph}"));
            let items = [item];
            let mut cv = Canvas::new(600, 400);
            let view = View {
                theme_palettes: None,
                literal_prefix: false,
                query: ":",
                placeholder: None,
                input_text: None,
                marked: None,
                input: None,
                items: &items,
                sel: usize::MAX,
                top: 0,
                scroll_remainder: 0.0,
                vp: LIST_VP,
                theme: Theme::default(),
                w: 600,
                h: 400,
                scale: 1,
                font_size: 14.0,
                mono_font_family: None,
                icons: &icons,
                bold_font: None,
                back: false,
                scrollbar: crate::scroll::Style::default(),
            };
            draw_emojis(&mut cv, &font, &view);
            assert!(
                cv.buf.as_chunks::<4>().0.iter().any(|p| p[3] != 0),
                "{glyph}"
            );
            assert!(cv
                .buf
                .as_chunks::<4>()
                .0
                .iter()
                .enumerate()
                .filter(|(_, p)| p[3] != 0)
                .all(|(i, _)| (HEAD as usize..(HEAD + 64.0) as usize + 12).contains(&(i / 600))));
        }
    }

    #[test]
    fn emoji_empty_state_centres_a_golden_ratio_line_box() {
        let Some(font) = test_font() else { return };
        let icons = Cache::new("", "");
        for scale in [1, 2] {
            let s = scale as f32;
            let mut cv = Canvas::new(600 * scale, 400 * scale);
            let view = View {
                theme_palettes: None,
                literal_prefix: false,
                query: ":missing",
                placeholder: None,
                input_text: None,
                marked: None,
                input: None,
                items: &[],
                sel: 0,
                top: 0,
                scroll_remainder: 0.0,
                vp: LIST_VP,
                theme: Theme::default(),
                w: 600,
                h: 400,
                scale,
                font_size: 14.0,
                mono_font_family: None,
                icons: &icons,
                bold_font: None,
                back: false,
                scrollbar: crate::scroll::Style::default(),
            };
            draw_emojis(&mut cv, &font, &view);
            let mut expected = Canvas::new(600 * scale, 400 * scale);
            let lh = 23.0 * s;
            text_lh(
                &mut expected,
                &font,
                14.0 * s,
                (600.0 * s - Canvas::text_width(&font, 14.0 * s, "No emojis found")) / 2.0,
                (HEAD + LIST_PAD) * s + (200.0 * s - lh) / 2.0,
                lh,
                "No emojis found",
                view.theme.empty_state_color.rgb,
                view.theme.empty_state_color.alpha,
                600.0 * s,
            );
            assert_eq!(cv.buf, expected.buf);
        }
    }

    #[test]
    fn emoji_grid_clips_a_partial_row_at_the_viewport_bottom() {
        // zlaunch's emoji picker is the same overflow-hidden vlist: with the
        // fixed 400px panel a 7px sliver of the 6th emoji row is visible.
        let Some(font) = test_font() else { return };
        let items: Vec<Item> = (0..48).map(|i| app(&format!("a{i}"))).collect();
        let mut icons = Cache::new("", "");
        icons.ensure_input(1);
        let mut cv = Canvas::new(640, PANEL_HEIGHT);
        draw(
            &mut cv,
            &font,
            &View {
                theme_palettes: None,
                literal_prefix: false,
                query: ":smile",
                placeholder: None,
                input_text: None,
                marked: None,
                input: None,
                items: &items,
                sel: 40, // row 5, column 0: the partially visible row
                top: 0,
                scroll_remainder: 0.0,
                vp: LIST_VP,
                theme: Theme::default(),
                w: 640,
                h: PANEL_HEIGHT,
                scale: 1,
                font_size: 14.0,
                mono_font_family: None,
                icons: &icons,
                bold_font: None,
                back: false,
                scrollbar: crate::scroll::Style::default(),
            },
        );
        // Row 5's selection box starts at 385 and is cut at 392.
        assert_ne!(px(&cv, 80, 391), px(&cv, 80, 395));
        for y in 392..399 {
            assert_eq!(px(&cv, 80, y), px(&cv, 80, 398), "y={y}");
        }
    }
    #[test]
    fn section_headers_use_the_bold_face_when_available() {
        // zlaunch styles section headers EXTRA_BOLD.
        let Some(font) = test_font() else { return };
        let Some(bold) = [
            "/usr/share/fonts/truetype/dejavu/DejaVuSans-Bold.ttf",
            "/usr/share/fonts/dejavu/DejaVuSans-Bold.ttf",
        ]
        .iter()
        .find_map(|p| std::fs::read(p).ok())
        .and_then(Font::try_from_vec) else {
            return;
        };
        assert!(
            Canvas::text_width(&bold, 12.0, "Applications")
                > Canvas::text_width(&font, 12.0, "Applications")
        );
        let items = sectioned(4);
        let drawn_with = |bf: Option<&Font>| {
            let mut icons = Cache::new("", "");
            icons.ensure_input(1);
            let mut cv = Canvas::new(640, PANEL_HEIGHT);
            draw(
                &mut cv,
                &font,
                &View {
                    theme_palettes: None,
                    literal_prefix: false,
                    query: "a",
                    placeholder: None,
                    input_text: None,
                    marked: None,
                    input: None,
                    items: &items,
                    sel: 0,
                    top: 0,
                    scroll_remainder: 0.0,
                    vp: LIST_VP,
                    theme: Theme::default(),
                    w: 640,
                    h: PANEL_HEIGHT,
                    scale: 1,
                    font_size: 14.0,
                    mono_font_family: None,
                    icons: &icons,
                    bold_font: bf,
                    back: false,
                    scrollbar: crate::scroll::Style::default(),
                },
            );
            cv
        };
        let plain = drawn_with(None);
        let bolded = drawn_with(Some(&bold));
        // The "Best Match" header band (HEAD + LIST_PAD .. +HDR) differs.
        let band_differs =
            (66..90).any(|y| (20..200).any(|x| px(&plain, x, y) != px(&bolded, x, y)));
        assert!(band_differs);
    }
    #[test]
    fn headers_need_two_sections() {
        let plain: Vec<Item> = (0..4).map(|i| app(&format!("a{i}"))).collect();
        assert!(!show_headers(&plain));
        assert!(show_headers(&sectioned(4)));
        let sl = slots(&sectioned(4), 0, 400.0);
        assert_eq!(sl[0].item, None);
        assert_eq!(sl[0].header, "Best Match");
        assert_eq!(sl[2].header, "Applications");
        assert_eq!(sl.iter().filter(|s| s.item.is_some()).count(), 4);
    }
    #[test]
    fn scroll_limit_matches_linear_search_for_section_boundaries() {
        for len in 0..65 {
            for pattern in 0..4 {
                let mut items: Vec<Item> = (0..len).map(|i| app(&format!("a{i}"))).collect();
                for (i, item) in items.iter_mut().enumerate() {
                    item.best = pattern == 1 && i == 0 || pattern == 3 && i % 3 == 0;
                    if pattern >= 2 && i % 2 == 0 {
                        item.tag = "Basic";
                    }
                }
                for vp in [
                    -1.0,
                    0.0,
                    49.98,
                    49.99,
                    50.0,
                    50.01,
                    77.98,
                    78.0,
                    100.0,
                    178.0,
                    300.0,
                    456.0,
                    5000.0,
                    f32::INFINITY,
                    f32::NAN,
                ] {
                    let last = items.len().saturating_sub(1);
                    let expected = (0..items.len())
                        .find(|&t| {
                            slots(&items, t, vp)
                                .last()
                                .is_some_and(|s| s.item == Some(last))
                        })
                        .unwrap_or(0);
                    assert_eq!(
                        max_top(&items, vp),
                        expected,
                        "len={len}, pattern={pattern}, vp={vp}"
                    );
                }
            }
        }
    }
    #[test]
    fn theme_icon_shows_palette_samples() {
        let th = Theme {
            bg_alpha: 1.0,
            text_alpha: 1.0,
            select_alpha: 1.0,
            border_alpha: 1.0,
            ..crate::config::theme("nord")
        };
        let mut cv = Canvas::new(24, 24);
        theme_icon(&mut cv, 0.0, 0.0, 24.0, &th);
        // The canvas is premultiplied BGR; every alpha is 1 here, so channels are exact.
        let px = |x: usize, y: usize| {
            let o = (y * 24 + x) * 4;
            [cv.buf[o + 2], cv.buf[o + 1], cv.buf[o]]
        };
        assert_eq!(px(7, 7), th.text, "title colour square");
        assert_eq!(px(18, 7), lighten(th.select, 0.1), "selection square");
        assert_eq!(px(12, 17), th.border, "border bar");
        assert_eq!(px(1, 12), lighten(th.bg, 0.05), "window background");
    }
    #[test]
    fn custom_theme_layout_drives_slots_hits_scroll_bounds_and_emoji_cells() {
        let metrics = crate::config::ThemeMetrics {
            item_margin_y: 3.0,
            item_padding_y: 10.0,
            item_content_height: 58.0,
            section_header_margin_top: 12.0,
            section_header_margin_bottom: 5.0,
            emoji_cell_size: 42.0,
            emoji_cell_gap: 3.0,
            emoji_columns: 4,
            ..crate::config::ThemeMetrics::default()
        };
        let layout = Layout::new(&metrics);
        assert_eq!((layout.row, layout.header), (84.0, 33.0));
        let items = ["Basic", "Application", "Application"]
            .into_iter()
            .map(|tag| Item {
                title: tag.into(),
                sub: String::new(),
                tag,
                icon: None,
                action: Action::View(String::new()),
                best: false,
            })
            .collect::<Vec<_>>();
        let rows = layout.slots(&items, 0, 200.0);
        assert_eq!((rows[0].h, rows[1].y, rows[1].h), (33.0, 33.0, 84.0));
        assert_eq!(layout.item_at(&items, 0, 200.0, 32.0), None);
        assert_eq!(layout.item_at(&items, 0, 200.0, 33.0), Some(0));
        assert_eq!(layout.item_at(&items, 0, 200.0, 150.0), Some(1));
        assert_eq!(layout.scroll_offset(&items, items.len(), false), 318.0);
        assert_eq!(layout.scroll_item_y(&items, 1, false), 150.0);
        assert_eq!(layout.max_top(&items, 100.0), 2);
        let geometry = layout
            .scroll_geometry(&items, "", 600, 200.0, 1, 5.0)
            .unwrap();
        assert_eq!(geometry.content, 318.0);
        assert_eq!(layout.emoji_columns(600), 4);
        let left = (600.0 - (4.0 * 45.0 - 3.0)) / 2.0;
        assert_eq!(
            layout.emoji_hit(600, 3, 0, 12, left + 1.0, HEAD + EMOJI_PAD + 43.0),
            Some(4)
        );
        assert_eq!(
            layout.emoji_hit(600, 3, 0, 12, left + 43.0, HEAD + EMOJI_PAD + 1.0),
            None
        );
    }

    #[test]
    fn pixel_scroll_clip_applies_to_shapes_symbols_and_icons() {
        let mut c = Canvas::new(10, 10);
        c.set_clip_top(3);
        c.set_clip_y(7);
        c.rrect(0.0, 0.0, 10.0, 10.0, 0.0, [255, 0, 0], 1.0);
        let icon = Icon {
            symbol: false,
            size: 10,
            px: vec![255; 400],
        };
        c.icon(0, 0, &icon);
        c.symbol(0, 0, &icon, [0, 255, 0], 1.0);
        for y in 0..10 {
            assert_eq!(
                c.buf[(y * 10 + 5) * 4 + 3],
                if (3..7).contains(&y) { 255 } else { 0 }
            );
        }
        c.clear_clip();
        c.icon(0, 0, &icon);
        assert!(c.buf.chunks(4).all(|p| p[3] == 255));
    }

    #[test]
    fn rrect_clips_to_canvas() {
        let mut c = Canvas::new(20, 20);
        c.rrect(-5.0, -5.0, 100.0, 100.0, 4.0, [255, 0, 0], 1.0);
        assert_eq!(c.buf[((10 * 20 + 10) * 4 + 2) as usize], 255);
        assert_eq!(c.buf.len(), 20 * 20 * 4);
    }
}
