//! Action panel overlay, adapted from the original launcher.
use crate::{
    items::Item,
    ui::{self, Canvas},
};
use rusttype::Font;
use ui::{HEAD, ROW};

pub struct PanelView<'a> {
    pub title: &'a str,
    pub actions: &'a [Item],
    pub sel: usize,
    pub filter: &'a str,
}

pub const PANEL_HEAD: f32 = 24.0;
/// Room below the action rows for the filter box and its margins.
pub const PANEL_FOOT: f32 = 36.0;

/// Geometry of the action panel overlay, bottom-anchored above the footer's Actions button.
pub struct PanelGeom {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    /// Action rows that fit.
    pub cap: usize,
    /// Filtered index of the first drawn row.
    pub off: usize,
}

pub fn panel_layout(w: f32, list_h: f32, n: usize, sel: usize, s: f32) -> PanelGeom {
    let pw = (w * 0.46).min(380.0 * s);
    let max_h = (list_h * s - 8.0 * s).max(ROW * s * 2.0);
    let cap = (((max_h - (PANEL_HEAD + PANEL_FOOT) * s) / (ROW * s)).floor() as usize).max(1);
    let rows = n.min(cap);
    let ph = ((PANEL_HEAD + PANEL_FOOT) * s + rows as f32 * ROW * s).min(max_h);
    let px = w - pw - 6.0 * s;
    let py = HEAD * s + 4.0 * s + (list_h * s - 8.0 * s - ph).max(0.0);
    let off = if rows > 0 && sel >= rows {
        sel + 1 - rows
    } else {
        0
    };
    PanelGeom {
        x: px,
        y: py,
        w: pw,
        h: ph,
        cap: rows,
        off,
    }
}

pub fn draw(cv: &mut Canvas, font: &Font, v: &ui::View, pv: &PanelView) {
    let s = v.scale as f32;
    let w = v.w as f32 * s;
    let th = &v.theme;
    let vis: Vec<usize> = pv
        .actions
        .iter()
        .enumerate()
        .filter(|(_, a)| a.title.to_lowercase().contains(pv.filter))
        .map(|(i, _)| i)
        .collect();
    let g = panel_layout(w, v.vp, vis.len(), pv.sel, s);
    let (px, py, pw, ph) = (g.x, g.y, g.w, g.h);
    cv.rrect(px, py, pw, ph, 10.0 * s, th.bg, 1.0);
    cv.rrect_outline(px, py, pw, ph, 10.0 * s, 1.0 * s, th.border, 1.0);
    let box_h = 30.0 * s;
    cv.text(
        font,
        11.0 * s,
        px + 14.0 * s,
        py + 9.0 * s,
        pv.title,
        th.dim,
        th.dim_alpha,
        pw - 28.0 * s,
    );
    let rows_y = py + PANEL_HEAD * s;
    let row_h = ROW * s;
    let sel = pv.sel.min(vis.len().saturating_sub(1));
    let mut y = rows_y;
    for (k, &ai) in vis.iter().enumerate().skip(g.off).take(g.cap) {
        let a = &pv.actions[ai];
        if k == sel {
            cv.rrect(
                px + 5.0 * s,
                y,
                pw - 10.0 * s,
                row_h,
                8.0 * s,
                th.select,
                th.select_alpha,
            );
        }
        let tx = px + 14.0 * s;
        let rx = px + pw - 12.0 * s;
        let ty = y + (row_h - v.font_size * s * 1.2) / 2.0 - 1.0 * s;
        let tw = cv.text(
            font,
            v.font_size * s,
            tx,
            ty,
            &a.title,
            th.text,
            th.text_alpha,
            rx - tx - 8.0 * s,
        );
        if !a.sub.is_empty() && rx - tx - tw - 16.0 * s > 30.0 * s {
            cv.text(
                font,
                12.5 * s,
                tx + tw + 8.0 * s,
                ty + 0.5 * s,
                &a.sub,
                th.dim,
                th.dim_alpha,
                rx - tx - tw - 16.0 * s,
            );
        }
        y += row_h;
    }
    if vis.is_empty() {
        cv.text(
            font,
            12.5 * s,
            px + 14.0 * s,
            rows_y + 6.0 * s,
            "No matching actions",
            th.dim,
            th.dim_alpha,
            pw - 28.0 * s,
        );
    }
    // The filter box at the bottom narrows the actions as you type.
    let by = py + ph - box_h - 5.0 * s;
    cv.rrect_outline(
        px + 8.0 * s,
        by,
        pw - 16.0 * s,
        box_h,
        7.0 * s,
        1.0 * s,
        th.border,
        1.0,
    );
    if pv.filter.is_empty() {
        cv.text(
            font,
            12.5 * s,
            px + 18.0 * s,
            by + 8.0 * s,
            "Filter actions...",
            th.dim,
            th.dim_alpha,
            pw - 36.0 * s,
        );
    } else {
        cv.text(
            font,
            12.5 * s,
            px + 18.0 * s,
            by + 8.0 * s,
            pv.filter,
            th.text,
            th.text_alpha,
            pw - 36.0 * s,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn panel_scrolls_to_selected_action() {
        let g = panel_layout(640.0, 400.0, 30, 29, 1.0);
        assert!(g.cap > 0);
        assert_eq!(g.off + g.cap, 30);
        assert!(g.x >= 0.0 && g.x + g.w <= 640.0);
        assert!(g.y >= HEAD && g.y + g.h <= HEAD + 400.0);
        let empty = panel_layout(640.0, 400.0, 0, 0, 1.0);
        assert_eq!((empty.cap, empty.off), (0, 0));
    }
}
