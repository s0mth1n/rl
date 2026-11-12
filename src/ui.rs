//! CPU drawing: a premultiplied ARGB canvas, rounded rectangles and text.
use crate::config::{Rgb, Theme};
use crate::icons::{Cache, Icon};
use crate::items::{Action, Item};
use rusttype::{point, Font, Scale};

// Layout metrics follow zlaunch (MIT, see NOTICE): 600x400 panel, 12 px corner
// radius, 1 px border, 57 px header (12 + 32 input + 12 + 1 separator), 8 px
// list padding, 50 px rows, 24 px icons, 14 px titles and input text.
pub const HEAD: f32 = 57.0;
pub const ROW: f32 = 50.0;
pub const HDR: f32 = 28.0;
pub const ICON: f32 = 24.0;
pub const LIST_PAD: f32 = 8.0;
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
/// Input text selection: gpui theme.selection from Default Dark, constant
/// across launcher themes (selection.background #1d4ed8).
const INPUT_SELECTION: Rgb = [29, 78, 216];
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
}

#[allow(clippy::too_many_arguments)]
impl Canvas {
    pub fn new(w: u32, h: u32) -> Self {
        Canvas {
            w,
            h,
            buf: vec![0; (w * h * 4) as usize],
            clip_y: i32::MAX,
        }
    }

    /// Clip all painting to y < `max_y` (physical pixels).
    pub fn set_clip_y(&mut self, max_y: i32) {
        self.clip_y = max_y;
    }

    pub fn clear_clip(&mut self) {
        self.clip_y = i32::MAX;
    }

    #[inline]
    fn blend(&mut self, x: i32, y: i32, c: Rgb, a: f32) {
        if x < 0
            || y < 0
            || x >= self.w as i32
            || y >= self.h as i32
            || y >= self.clip_y
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
            while !text.is_empty() && Self::text_width(font, size, &format!("{text}...")) > max_w {
                text.pop();
            }
            text.push_str("...");
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
    pub query: &'a str,
    pub input: Option<(usize, Option<usize>, bool)>,
    pub items: &'a [Item],
    pub sel: usize,
    pub top: usize,
    /// List viewport height in unscaled pixels, from `viewport`.
    pub vp: f32,
    pub theme: Theme,
    pub w: u32,
    pub h: u32,
    pub scale: u32,
    pub font_size: f32,
    pub icons: &'a Cache,
    /// Bold face for section headers (zlaunch styles them EXTRA_BOLD); falls
    /// back to the regular face when no bold font was found.
    pub bold_font: Option<&'a Font<'a>>,
    /// The sub-view was entered from the main list, so the input prefix is a
    /// clickable back arrow instead of the magnifier (zlaunch mode origin).
    pub back: bool,
    /// Draw the scrollbar thumb; false once scrolling has been idle for a second.
    pub show_scrollbar: bool,
}

pub fn icon_px(scale: u32) -> u32 {
    ICON as u32 * scale
}

/// Section an item is listed under, if its kind has one.
pub fn section_of(it: &Item) -> Option<&'static str> {
    if it.best {
        return Some("Best Match");
    }
    match it.tag {
        "Application" => Some("Applications"),
        "Window" => Some("Windows"),
        "Basic" => Some("Commands"),
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
pub fn slots(items: &[Item], top: usize, vp: f32) -> Vec<Slot> {
    let headers = show_headers(items);
    let mut out = Vec::new();
    let mut y = 0.0;
    for (i, item) in items.iter().enumerate().skip(top) {
        let sec = section_of(item);
        let starts = i == 0 || sec != section_of(&items[i - 1]);
        if headers && starts {
            if let Some(name) = sec {
                if y + HDR >= vp {
                    break;
                }
                out.push(Slot {
                    y,
                    h: HDR,
                    item: None,
                    header: name,
                });
                y += HDR;
            }
        }
        if y >= vp {
            break;
        }
        out.push(Slot {
            y,
            h: ROW.min(vp - y),
            item: Some(i),
            header: "",
        });
        y += ROW;
    }
    out
}

/// Fixed panel height: zlaunch sizes every panel to `launcher_size`
/// (default 600x400) instead of to its content (config/types.rs, render.rs).
pub const PANEL_HEIGHT: u32 = 400;
/// Fixed height of the list viewport inside the 400 px panel.
pub const LIST_VP: f32 = PANEL_HEIGHT as f32 - HEAD - LIST_PAD * 2.0;
/// Emoji grid rows that fit in the fixed panel.
pub const EMOJI_ROWS: usize = 5; // (400 - HEAD - 2*EMOJI_PAD) / EMOJI_CELL

/// Largest first item that still leaves no gap below the last row.
pub fn max_top(items: &[Item], vp: f32) -> usize {
    let last = items.len().saturating_sub(1);
    let (mut lo, mut hi) = (0, items.len());
    while lo < hi {
        let mid = lo + (hi - lo) / 2;
        if slots(items, mid, vp)
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

/// First visible item so that `sel` is fully shown, moving `top` as little as possible.
/// Item under list-relative y (unscaled), if any.
pub fn item_at(items: &[Item], top: usize, vp: f32, y: f32) -> Option<usize> {
    slots(items, top, vp)
        .into_iter()
        .find(|s| y >= s.y && y < s.y + s.h)
        .and_then(|s| s.item)
}

fn action_label(a: &Action) -> &'static str {
    match a {
        Action::App(_) => "Open",
        Action::Window(_) => "Switch",
        Action::Clip(_) | Action::ClipImg(_) => "Copy",
        Action::View(_) => "Open",
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
fn enter_key(cv: &mut Canvas, s: f32, right: f32, mid: f32, th: &Theme) -> f32 {
    let (kw, kh) = (18.0 * s, 15.0 * s);
    let (x, y) = (right - kw, mid - kh / 2.0);
    let bg = th.key_background;
    cv.rrect(x, y, kw, kh, 3.0 * s, bg.rgb, bg.alpha);
    let bd = th.key_border;
    cv.rrect_outline(x, y, kw, kh, 3.0 * s, 1.0 * s, bd.rgb, bd.alpha);
    let (cx, cy) = (x + kw / 2.0, y + kh / 2.0);
    let t = (1.0 * s).max(1.0);
    let kc = th.key_color;
    let (c, a) = (kc.rgb, kc.alpha);
    // right stem, shaft and a two-step arrow head pointing left
    cv.rrect(cx + 3.0 * s, cy - 3.5 * s, t, 5.5 * s, 0.0, c, a);
    cv.rrect(cx - 3.5 * s, cy + 1.5 * s, 7.0 * s + t, t, 0.0, c, a);
    cv.rrect(cx - 2.5 * s, cy + 0.5 * s, t, t, 0.0, c, a);
    cv.rrect(cx - 2.5 * s, cy + 2.5 * s, t, t, 0.0, c, a);
    cv.rrect(cx - 1.5 * s, cy - 0.5 * s, t, t, 0.0, c, a);
    cv.rrect(cx - 1.5 * s, cy + 3.5 * s, t, t, 0.0, c, a);
    kw
}

pub const EMOJI_CELL: f32 = 64.0;
pub const EMOJI_GAP: f32 = 2.0;
pub const EMOJI_PAD: f32 = 8.0;

pub fn emoji_columns(width: u32) -> usize {
    (((width as f32 - 16.0 + EMOJI_GAP) / (EMOJI_CELL + EMOJI_GAP)) as usize).clamp(1, 8)
}

pub fn emoji_rows(count: usize, width: u32) -> usize {
    count.div_ceil(emoji_columns(width)).clamp(1, EMOJI_ROWS)
}

pub fn emoji_icon_px(scale: u32) -> u32 {
    28 * scale
}

pub fn emoji_hit(
    width: u32,
    rows: usize,
    top: usize,
    count: usize,
    x: f32,
    y: f32,
) -> Option<usize> {
    let columns = emoji_columns(width);
    let grid_width = columns as f32 * (EMOJI_CELL + EMOJI_GAP) - EMOJI_GAP;
    let x = x - (width as f32 - grid_width) / 2.0;
    let y = y - HEAD - EMOJI_PAD;
    if x < 0.0 || y < 0.0 || x >= grid_width || y >= rows as f32 * EMOJI_CELL {
        return None;
    }
    let col = (x / (EMOJI_CELL + EMOJI_GAP)) as usize;
    if x % (EMOJI_CELL + EMOJI_GAP) >= EMOJI_CELL {
        return None;
    }
    let index = top + (y / EMOJI_CELL) as usize * columns + col;
    (index < count).then_some(index)
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
    let columns = emoji_columns(v.w);
    let grid_width = columns as f32 * (EMOJI_CELL + EMOJI_GAP) - EMOJI_GAP;
    let left = (v.w as f32 - grid_width) / 2.0;
    if v.items.is_empty() {
        // centered in empty_state_color, like zlaunch's emoji render_empty
        let es = v.font_size * s;
        let tw = Canvas::text_width(font, es, "No emojis found");
        cv.text(
            font,
            es,
            (v.w as f32 * s - tw) / 2.0,
            (HEAD + 14.0) * s,
            "No emojis found",
            v.theme.empty_state_color.rgb,
            v.theme.empty_state_color.alpha,
            v.w as f32 * s,
        );
    }
    // Rows that start above the viewport bottom are drawn and clipped
    // mid-row, like zlaunch's overflow-hidden emoji vlist.
    cv.set_clip_y(((HEAD + LIST_PAD + v.vp) * s) as i32);
    for (offset, item) in v
        .items
        .iter()
        .skip(v.top)
        .take((v.vp / EMOJI_CELL).ceil() as usize * columns)
        .enumerate()
    {
        let x = (left + (offset % columns) as f32 * (EMOJI_CELL + EMOJI_GAP)) * s;
        let y = (HEAD + EMOJI_PAD + (offset / columns) as f32 * EMOJI_CELL) * s;
        if v.top + offset == v.sel {
            cv.rrect(
                x,
                y,
                EMOJI_CELL * s,
                EMOJI_CELL * s,
                6.0 * s,
                v.theme.emoji_selected.rgb,
                v.theme.emoji_selected.alpha,
            );
        }
        if let Some(icon) = item
            .icon
            .as_deref()
            .and_then(|name| v.icons.get(name, emoji_icon_px(v.scale)))
        {
            cv.icon((x + 18.0 * s) as i32, (y + 18.0 * s) as i32, icon);
        }
    }
    cv.clear_clip();
}

/// Hit zone for the back arrow in the input prefix, shown when a sub-view
/// was entered from the main list.
pub fn back_button_at(x: f64, y: f64) -> bool {
    (14.0..40.0).contains(&x) && y >= (HEAD as f64 - 24.0) / 2.0 && y < (HEAD as f64 + 24.0) / 2.0
}

pub fn clear_button_at(width: u32, query: &str, x: f64, y: f64) -> bool {
    !query
        .strip_prefix([':', ';', '#'])
        .unwrap_or(query)
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

pub fn input_at(font: &Font, query: &str, caret: usize, width: u32, x: f64) -> usize {
    let shown = query
        .strip_prefix([':', ';', '#'])
        .map(str::trim_start)
        .unwrap_or(query);
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

fn draw_input(cv: &mut Canvas, font: &Font, v: &View) {
    let s = v.scale as f32;
    let w = v.w as f32 * s;
    let fs = 16.0 * s;
    let y = (HEAD * s - fs) / 2.0;
    // Menu-entered sub-views show a back arrow; the theme picker entered
    // directly shows a palette, as zlaunch's theme mode does.
    let prefix_icon = if v.back {
        crate::icons::BACK
    } else if v.query.trim_start().starts_with('#') {
        crate::icons::PALETTE
    } else {
        crate::icons::SEARCH
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
    let (query, placeholder) = if let Some(query) = v.query.strip_prefix(':') {
        (query.trim_start(), "Search emojis...")
    } else if let Some(query) = v.query.strip_prefix('#') {
        (query.trim_start(), "Search themes...")
    } else if let Some(query) = v.query.strip_prefix(';') {
        (query.trim_start(), "Search clipboard...")
    } else {
        (v.query, "Search anything...")
    };
    let prefix = v.query.len() - query.len();
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
            cv.rrect(x + left, y, width, fs * 1.15, 0.0, INPUT_SELECTION, 1.0);
        }
    }
    if query.is_empty() {
        cv.text(font, fs, x, y, placeholder, INPUT_DIM, 1.0, max_w);
    } else {
        cv.text(font, fs, x, y, &query[start..end], INPUT_FG, 1.0, max_w);
    }
    if visible && anchor.is_none_or(|a| a == caret + prefix) {
        let cx = Canvas::text_width(font, fs, &query[start..caret]);
        cv.rrect(x + cx, y, 2.0 * s, fs * 1.15, 0.0, INPUT_FG, 1.0);
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
    let s = v.scale as f32;
    let th = &v.theme;
    let (w, h) = (v.w as f32 * s, v.h as f32 * s);
    let wr = th.window_border_radius * s;
    cv.rrect(0.0, 0.0, w, h, wr, th.bg, th.bg_alpha);
    // zlaunch's daemon forces theme.window_border transparent (daemon theme.rs),
    // so the window outline is invisible; th.border keeps the TOML value for the
    // theme-picker preview swatch.
    cv.rrect_outline(0.0, 0.0, w, h, wr, 1.0 * s, th.border, 0.0);
    draw_input(cv, font, v);
    cv.rrect(0.0, (HEAD - 1.0) * s, w, s, 0.0, SEPARATOR, SEPARATOR_ALPHA);
    if v.query.starts_with(':') {
        draw_emojis(cv, font, v);
        return;
    }
    if v.items.is_empty() {
        let es = v.font_size * s;
        let label = if crate::items::theme_view(v.query) {
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
            (HEAD + LIST_PAD + 17.0) * s,
            label,
            ec.rgb,
            ec.alpha,
            w,
        );
        return;
    }
    let title_px = 14.0 * s;
    // Clip rows at the viewport bottom so a partial row is cut mid-row,
    // like zlaunch's overflow-hidden vlist.
    cv.set_clip_y(((HEAD + LIST_PAD + v.vp) * s) as i32);
    for sl in slots(v.items, v.top, v.vp) {
        let y = (HEAD + LIST_PAD + sl.y) * s;
        let Some(i) = sl.item else {
            // section header: EXTRA_BOLD in zlaunch; 8px above, 4px below, 16px in
            text_lh(
                cv,
                v.bold_font.unwrap_or(font),
                12.0 * s,
                16.0 * s,
                y + 8.0 * s,
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
                8.0 * s,
                y + 1.0 * s,
                w - 16.0 * s,
                (ROW - 2.0) * s,
                6.0 * s,
                bg,
                bg_alpha,
            );
        }
        let (ix, iy) = (16.0 * s, y + (ROW - ICON) / 2.0 * s);
        let icon_name = it.icon.as_deref();
        if let Some(name) = icon_name.and_then(|n| n.strip_prefix("theme:")) {
            theme_icon(cv, ix, iy, ICON * s, &crate::config::theme(name));
        } else if let Some(ic) = icon_name.and_then(|n| v.icons.get(n, icon_px(v.scale))) {
            if ic.symbol {
                cv.symbol(ix as i32, iy as i32, ic, th.dim, th.dim_alpha);
            } else {
                cv.icon(ix as i32, iy as i32, ic);
            }
        } else {
            let pb = th.icon_placeholder_background;
            cv.rrect(ix, iy, ICON * s, ICON * s, 4.0 * s, pb.rgb, pb.alpha);
            let initial: String = it
                .title
                .chars()
                .find(|c| c.is_alphanumeric())
                .map(|c| c.to_uppercase().to_string())
                .unwrap_or_default();
            let iw = Canvas::text_width(font, 14.0 * s, &initial);
            let pc = th.icon_placeholder_color;
            text_lh(
                cv,
                font,
                14.0 * s,
                ix + ICON * s / 2.0 - iw / 2.0,
                iy,
                ICON * s,
                &initial,
                pc.rgb,
                pc.alpha,
                40.0 * s,
            );
        }
        // trailing action hint, shown on the selected row only
        let mut right = w - 24.0 * s;
        if selected {
            let label = action_label(&it.action);
            let kw = enter_key(cv, s, right, y + ROW / 2.0 * s, th);
            let lw = Canvas::text_width(font, 12.0 * s, label);
            text_lh(
                cv,
                font,
                12.0 * s,
                right - kw - 8.0 * s - lw,
                y + (ROW - 16.0) / 2.0 * s,
                16.0 * s,
                label,
                th.action_label.rgb,
                th.action_label.alpha,
                lw + 4.0,
            );
            right -= kw + 8.0 + lw + 8.0 * s;
        }
        let tx = 48.0 * s;
        let max_t = right - tx - 8.0 * s;
        if it.sub.is_empty() {
            text_lh(
                cv,
                font,
                title_px,
                tx,
                y + (ROW - 16.0) / 2.0 * s,
                16.0 * s,
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
                y + 8.0 * s,
                16.0 * s,
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
                y + 24.0 * s,
                18.0 * s,
                &it.sub,
                th.dim,
                th.dim_alpha,
                max_t,
            );
        }
    }
    cv.clear_clip();
    // scroll hint
    let vp = v.vp * s;
    let max = max_top(v.items, v.vp);
    if max > 0 && v.show_scrollbar {
        let track = vp - 12.0 * s;
        let visible = slots(v.items, v.top, v.vp)
            .iter()
            .filter(|x| x.item.is_some())
            .count();
        let th_h = (track * visible as f32 / v.items.len() as f32).max(18.0 * s);
        let ty = (HEAD + LIST_PAD) * s + 6.0 * s + (track - th_h) * (v.top as f32 / max as f32);
        cv.rrect(
            w - 5.0 * s,
            ty,
            2.5 * s,
            th_h,
            1.2 * s,
            th.dim,
            th.dim_alpha,
        );
    }
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
                query: "a",
                input: None,
                items,
                sel,
                top: 0,
                vp,
                theme: th,
                w: 640,
                h,
                scale: 1,
                font_size: 15.0,
                icons: &icons,
                bold_font: None,
                back: false,
                show_scrollbar: true,
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
    fn input_row_matches_zlaunch_gpui_dark() {
        let Some(font) = test_font() else {
            eprintln!("skip: DejaVu Sans not installed");
            return;
        };
        let items: Vec<Item> = (0..3).map(|i| app(&format!("a{i}"))).collect();
        let cv = drawn(&font, &items, 0, Theme::default());
        // separator: white at 0.1, constant across themes (gpui theme.border)
        assert_eq!(px(&cv, 500, 56), [34, 34, 34, 185]);
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
                query: "",
                input: None,
                back: false,
                show_scrollbar: true,
                items: &items,
                sel: 0,
                top: 0,
                vp,
                theme: Theme::default(),
                w: 640,
                h,
                scale: 1,
                font_size: 15.0,
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
                query: "a",
                input: Some((1, Some(0), true)),
                items: &items,
                sel: 0,
                top: 0,
                vp,
                theme: Theme::default(),
                w: 640,
                h,
                scale: 1,
                font_size: 15.0,
                back: false,
                show_scrollbar: false,
                icons: &icons,
                bold_font: None,
            },
        );
        assert!(
            has_solid(&cv, 50, 62, 20, 40, [216, 78, 29, 255]),
            "selection"
        );
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
        // window border: daemon forces window_border transparent, only panel shows
        assert_eq!(px(&cv, 0, 110), [10, 10, 10, 178]);
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
        assert_eq!((cfg.width, cfg.max_rows, cfg.font_size), (600, 6, 14.0));
        assert_eq!(
            (HEAD, ROW, HDR, ICON, LIST_PAD),
            (57.0, 50.0, 28.0, 24.0, 8.0)
        );
        assert_eq!((INPUT_X, INPUT_RIGHT), (49.7, 90.0));
        // zlaunch's default launcher_size fixes every panel at 600x400.
        assert_eq!(PANEL_HEIGHT, 400);
        assert_eq!(LIST_VP, 327.0);
        assert_eq!(EMOJI_ROWS, 5);
        for scale in 1..=4 {
            assert_eq!(icon_px(scale), 24 * scale);
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
        // The fixed 327px viewport shows full rows plus a clipped last row.
        let Some(font) = test_font() else { return };
        let mut icons = Cache::new("", "");
        icons.ensure_input(1);
        let items = sectioned(30);
        let sl = slots(&items, 0, LIST_VP);
        let last = sl.last().unwrap();
        assert_eq!(last.h, 21.0); // 327 - (2*28 + 5*50)
                                  // The 7th row's box and text stop at the viewport bottom (392px).
        let mut cv = Canvas::new(600, PANEL_HEIGHT);
        draw(
            &mut cv,
            &font,
            &View {
                query: "a",
                input: None,
                items: &items,
                sel: 5,
                top: 0,
                vp: LIST_VP,
                theme: Theme::default(),
                w: 600,
                h: PANEL_HEIGHT,
                scale: 1,
                font_size: 14.0,
                icons: &icons,
                bold_font: None,
                back: false,
                show_scrollbar: false,
            },
        );
        // Pixels below the viewport keep the panel background.
        for y in 392..400 {
            assert_eq!(px(&cv, 300, y), px(&cv, 300, 399), "y={y}");
        }
        assert_ne!(px(&cv, 300, 391), px(&cv, 300, 399));
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
                query: ":smile",
                input: None,
                items: &items,
                sel: 40, // row 5, column 0: the partially visible row
                top: 0,
                vp: LIST_VP,
                theme: Theme::default(),
                w: 640,
                h: PANEL_HEIGHT,
                scale: 1,
                font_size: 14.0,
                icons: &icons,
                bold_font: None,
                back: false,
                show_scrollbar: false,
            },
        );
        // Row 5's selection box starts at 385 and is cut at 392.
        assert_ne!(px(&cv, 80, 391), px(&cv, 80, 395));
        for y in 392..400 {
            assert_eq!(px(&cv, 80, y), px(&cv, 80, 399), "y={y}");
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
                    query: "a",
                    input: None,
                    items: &items,
                    sel: 0,
                    top: 0,
                    vp: LIST_VP,
                    theme: Theme::default(),
                    w: 640,
                    h: PANEL_HEIGHT,
                    scale: 1,
                    font_size: 14.0,
                    icons: &icons,
                    bold_font: bf,
                    back: false,
                    show_scrollbar: false,
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
    fn selection_scrolls_to_the_top_clamped_to_a_full_page() {
        // gpui's ScrollStrategy::Top: the selected row heads the viewport.
        let items = sectioned(30);
        let vp = LIST_VP;
        for sel in 0..30 {
            let top = sel.min(max_top(&items, vp));
            let sl = slots(&items, top, vp);
            assert!(
                sl.iter().any(|s| s.item == Some(sel)),
                "sel {sel} top {top}"
            );
            assert!(sl.last().unwrap().y + sl.last().unwrap().h <= vp + 0.01);
        }
        assert_eq!(item_at(&items, 0, vp, 5.0), None);
        assert_eq!(item_at(&items, 0, vp, HDR + 5.0), Some(0));
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
    fn rrect_clips_to_canvas() {
        let mut c = Canvas::new(20, 20);
        c.rrect(-5.0, -5.0, 100.0, 100.0, 4.0, [255, 0, 0], 1.0);
        assert_eq!(c.buf[((10 * 20 + 10) * 4 + 2) as usize], 255);
        assert_eq!(c.buf.len(), 20 * 20 * 4);
    }
}
