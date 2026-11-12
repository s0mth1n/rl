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
pub const INPUT_X: f32 = 46.0;
/// Right edge the query text must clear (room for the clear button).
pub const INPUT_RIGHT: f32 = 90.0;

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
                if r >= 0.5 && fx >= r && fx <= w - r && fy >= r && fy <= h - r {
                    self.blend(px, py, c, alpha);
                    continue;
                }
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

pub struct View<'a> {
    pub query: &'a str,
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

/// Entries that fit completely in `vp` pixels when the list starts at item `top`.
pub fn slots(items: &[Item], top: usize, vp: f32) -> Vec<Slot> {
    let headers = show_headers(items);
    let mut out = Vec::new();
    let mut y = 0.0;
    for i in top..items.len() {
        let sec = section_of(&items[i]);
        let starts = i == 0 || sec != section_of(&items[i - 1]);
        if headers && starts {
            if let Some(name) = sec {
                if y + HDR + ROW > vp + 0.01 {
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
        if y + ROW > vp + 0.01 {
            break;
        }
        out.push(Slot {
            y,
            h: ROW,
            item: Some(i),
            header: "",
        });
        y += ROW;
    }
    out
}

/// Height of the list area: the first `max_rows` items plus the headers among them.
pub fn viewport(items: &[Item], max_rows: usize) -> f32 {
    let n = items.len().min(max_rows).max(1);
    if items.is_empty() {
        return ROW;
    }
    let big = (n as f32) * (ROW + HDR) + 1.0;
    let sl = slots(items, 0, big);
    sl.iter()
        .rfind(|s| s.item.is_some_and(|i| i < n))
        .map_or(ROW, |s| s.y + s.h)
}

pub fn panel_height(items: &[Item], max_rows: usize) -> u32 {
    (HEAD + LIST_PAD + viewport(items, max_rows) + LIST_PAD) as u32
}

/// Largest first item that still leaves no gap below the last row.
pub fn max_top(items: &[Item], vp: f32) -> usize {
    let last = items.len().saturating_sub(1);
    (0..items.len())
        .find(|&t| {
            slots(items, t, vp)
                .last()
                .is_some_and(|s| s.item == Some(last))
        })
        .unwrap_or(0)
}

/// First visible item so that `sel` is fully shown, moving `top` as little as possible.
pub fn scroll_to(items: &[Item], sel: usize, top: usize, vp: f32) -> usize {
    let mut top = top.min(sel);
    while top < sel && !slots(items, top, vp).iter().any(|s| s.item == Some(sel)) {
        top += 1;
    }
    top.min(max_top(items, vp))
}

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
        Action::Clip(_) | Action::ClipImg(_) => "Copy",
        Action::View(_) | Action::Open(_) => "Open",
        Action::Nothing => "",
        Action::Setting(_) => "Change",
        Action::Exclude(_) => "Toggle",
        Action::ClearClip | Action::PinToggle(_) | Action::ResetRank(_) => "Run",
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

pub fn emoji_rows(count: usize, width: u32, max_rows: usize) -> usize {
    count.div_ceil(emoji_columns(width)).min(max_rows).max(1)
}

pub fn emoji_icon_px(scale: u32) -> u32 {
    28 * scale
}

pub fn emoji_panel_height(rows: usize) -> u32 {
    (HEAD + EMOJI_PAD * 2.0 + rows.max(1) as f32 * EMOJI_CELL) as u32
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
        cv.text(
            font,
            v.font_size * s,
            24.0 * s,
            (HEAD + 14.0) * s,
            "No emojis found",
            v.theme.dim,
            v.theme.dim_alpha,
            (v.w as f32 - 48.0) * s,
        );
    }
    for (offset, item) in v
        .items
        .iter()
        .skip(v.top)
        .take((v.vp / EMOJI_CELL) as usize * columns)
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
}

pub fn draw(cv: &mut Canvas, font: &Font, v: &View) {
    let s = v.scale as f32;
    let th = &v.theme;
    let (w, h) = (v.w as f32 * s, v.h as f32 * s);
    let wr = th.window_border_radius * s;
    cv.rrect(0.0, 0.0, w, h, wr, th.bg, th.bg_alpha);
    cv.rrect_outline(0.0, 0.0, w, h, wr, 1.0 * s, th.border, th.border_alpha);
    // search field: the magnifier and clear button are positioned by
    // the icon code and INPUT_RIGHT; this lays out the query text and cursor
    let fs = v.font_size * s;
    let qy = ((HEAD - 1.0) / 2.0 - 0.6 * v.font_size).max(2.0) * s;
    if v.query.starts_with(':') {
        let query = v.query[1..].trim_start();
        let (c, a) = if query.is_empty() {
            (th.dim, th.dim_alpha)
        } else {
            (th.text, th.text_alpha)
        };
        let tw = cv.text(
            font,
            fs,
            INPUT_X * s,
            qy,
            if query.is_empty() {
                "Search emojis..."
            } else {
                query
            },
            c,
            a,
            w - (INPUT_X + INPUT_RIGHT) * s,
        );
        cv.rrect(
            INPUT_X * s + if query.is_empty() { 0.0 } else { tw } + 2.0 * s,
            qy,
            1.5 * s,
            fs * 1.15,
            0.5 * s,
            th.accent,
            th.accent_alpha,
        );
    } else if v.query.is_empty() {
        cv.text(
            font,
            fs,
            INPUT_X * s,
            qy,
            "Search applications - : emoji - ; clipboard - settings",
            th.dim,
            th.dim_alpha,
            w - (INPUT_X + INPUT_RIGHT) * s,
        );
    } else {
        let tw = cv.text(
            font,
            fs,
            INPUT_X * s,
            qy,
            v.query,
            th.text,
            th.text_alpha,
            w - (INPUT_X + INPUT_RIGHT) * s,
        );
        cv.rrect(
            INPUT_X * s + tw + 2.0 * s,
            qy,
            1.5 * s,
            fs * 1.15,
            0.5 * s,
            th.accent,
            th.accent_alpha,
        );
    }
    cv.rrect(
        0.0,
        (HEAD - 1.0) * s,
        w,
        1.0 * s,
        0.0,
        th.border,
        th.border_alpha,
    );
    if v.query.starts_with(':') {
        draw_emojis(cv, font, v);
        return;
    }
    if v.items.is_empty() {
        let es = v.font_size * s;
        let tw = Canvas::text_width(font, es, "No results");
        let ec = th.empty_state_color;
        cv.text(
            font,
            es,
            (w - tw) / 2.0,
            (HEAD + LIST_PAD + 17.0) * s,
            "No results",
            ec.rgb,
            ec.alpha,
            w,
        );
        return;
    }
    let title_px = v.font_size * s;
    for sl in slots(v.items, v.top, v.vp) {
        let y = (HEAD + LIST_PAD + sl.y) * s;
        let Some(i) = sl.item else {
            // section header: 8px above, 4px below, 16px in
            text_lh(
                cv,
                font,
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
        // item box: 1px margin, 8px side margin, 6px corners
        if selected {
            cv.rrect(
                8.0 * s,
                y + 1.0 * s,
                w - 16.0 * s,
                (ROW - 2.0) * s,
                6.0 * s,
                th.select,
                th.select_alpha,
            );
        }
        let (ix, iy) = (16.0 * s, y + (ROW - ICON) / 2.0 * s);
        let icon = it
            .icon
            .as_deref()
            .and_then(|n| v.icons.get(n, icon_px(v.scale)));
        if let Some(ic) = icon {
            cv.icon(ix as i32, iy as i32, ic);
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
        if selected && !matches!(it.action, Action::Nothing) {
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
    // scroll hint
    let vp = v.vp * s;
    let max = max_top(v.items, v.vp);
    if max > 0 {
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
        assert_eq!(emoji_rows(0, 640, 8), 1);
        assert_eq!(emoji_rows(9, 640, 8), 2);
        assert_eq!(emoji_rows(1000, 640, 8), 8);
        assert_eq!(emoji_panel_height(2), (HEAD + 16.0 + 128.0) as u32);
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
    fn height_follows_rows() {
        let two: Vec<Item> = (0..2).map(|i| app(&format!("a{i}"))).collect();
        let eight: Vec<Item> = (0..8).map(|i| app(&format!("a{i}"))).collect();
        assert_eq!(panel_height(&[], 8), panel_height(&two[..1], 8));
        assert!(panel_height(&eight, 8) > panel_height(&two, 8));
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
    fn scrolling_keeps_selection_visible() {
        let items = sectioned(30);
        let vp = viewport(&items, 8);
        let mut top = 0;
        for sel in 0..30 {
            top = scroll_to(&items, sel, top, vp);
            let sl = slots(&items, top, vp);
            assert!(
                sl.iter().any(|s| s.item == Some(sel)),
                "sel {sel} top {top}"
            );
            assert!(sl.last().unwrap().y + sl.last().unwrap().h <= vp + 0.01);
        }
        assert_eq!(top, max_top(&items, vp));
        assert_eq!(item_at(&items, 0, vp, 5.0), None);
        assert_eq!(item_at(&items, 0, vp, HDR + 5.0), Some(0));
    }
    #[test]
    fn rrect_clips_to_canvas() {
        let mut c = Canvas::new(20, 20);
        c.rrect(-5.0, -5.0, 100.0, 100.0, 4.0, [255, 0, 0], 1.0);
        assert_eq!(c.buf[((10 * 20 + 10) * 4 + 2) as usize], 255);
        assert_eq!(c.buf.len(), 20 * 20 * 4);
    }
}
