//! The single-line input's Cut, Copy, Paste and Select All menu.
use crate::ui::Canvas;
use rusttype::Font;

pub const LABELS: [&str; 4] = ["Cut", "Copy", "Paste", "Select All"];
const TOPS: [f64; 4] = [5.0, 27.0, 49.0, 79.0];
const WIDTH: f64 = 133.0;
const HEIGHT: f64 = 104.0;

pub struct Menu {
    pub x: f64,
    pub y: f64,
    pub selected: Option<usize>,
    pub pressed: Option<usize>,
    pub enabled: [bool; 4],
}

impl Menu {
    pub fn new(x: f64, y: f64, width: u32, height: u32, selection: bool, paste: bool) -> Self {
        Self {
            x: x.clamp(8.0, (width as f64 - WIDTH - 8.0).max(8.0)),
            y: y.clamp(8.0, (height as f64 - HEIGHT - 8.0).max(8.0)),
            selected: None,
            pressed: None,
            enabled: [selection, selection, paste, true],
        }
    }

    pub fn at(&self, x: f64, y: f64) -> Option<usize> {
        if x < self.x + 4.0 || x >= self.x + WIDTH - 4.0 {
            return None;
        }
        TOPS.iter()
            .position(|top| y >= self.y + top && y < self.y + top + 20.0)
    }

    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + WIDTH && y >= self.y && y < self.y + HEIGHT
    }

    pub fn step(&mut self, previous: bool) {
        let mut index = self.selected.unwrap_or(if previous { 0 } else { 3 });
        for _ in 0..4 {
            index = (index + if previous { 3 } else { 1 }) % 4;
            if self.enabled[index] {
                self.selected = Some(index);
                break;
            }
        }
    }

    pub fn draw(&self, cv: &mut Canvas, font: &Font, scale: u32) {
        let s = scale as f32;
        let x = self.x as f32 * s;
        let y = self.y as f32 * s;
        // gpui shadow_lg: black at 0.1 alpha, offsets 10px/4px, blur 15px/6px, spread -3px/-4px.
        for (offset, blur, spread) in [(10.0_f32, 15.0_f32, -3.0_f32), (4.0, 6.0, -4.0)] {
            cv.shadow(
                (x, y, WIDTH as f32 * s, HEIGHT as f32 * s),
                (offset - 3.0_f32).max(0.0) * s,
                // Fitted to the 0.5.1 release's pixel profile (below and beside the menu):
                // a tight gaussian (sigma = blur / 10) with the spread 3px larger than the
                // nominal gpui number, with the release tail 3px shorter.
                blur * 0.2 * s,
                (spread + 3.0) * s,
                0.1,
            );
        }
        cv.rrect(
            x,
            y,
            WIDTH as f32 * s,
            HEIGHT as f32 * s,
            6.0 * s,
            [10, 10, 10],
            1.0,
        );
        cv.rrect_outline(
            x,
            y,
            WIDTH as f32 * s,
            HEIGHT as f32 * s,
            6.0 * s,
            s,
            [35, 35, 35],
            1.0,
        );
        cv.rrect(
            x + s,
            y + 74.0 * s,
            (WIDTH as f32 - 2.0) * s,
            s,
            0.0,
            [57, 57, 57],
            1.0,
        );
        for (i, label) in LABELS.iter().enumerate() {
            let top = y + TOPS[i] as f32 * s;
            if self.selected == Some(i) && self.enabled[i] {
                cv.rrect(
                    x + 4.0 * s,
                    top,
                    (WIDTH as f32 - 8.0) * s,
                    20.0 * s,
                    3.0 * s,
                    [38, 38, 38],
                    1.0,
                );
            }
            cv.text(
                font,
                14.0 * s,
                x + 13.0 * s,
                top + 3.0 * s,
                label,
                if self.enabled[i] {
                    [250, 250, 250]
                } else {
                    [115, 115, 115]
                },
                1.0,
                (WIDTH as f32 - 24.0) * s,
            );
            let shortcut = ["Ctrl+X", "Ctrl+C", "Ctrl+V", "Ctrl+A"][i];
            let w = Canvas::text_width(font, 12.0 * s, shortcut);
            cv.text(
                font,
                12.0 * s,
                x + (WIDTH as f32 - 12.0) * s - w,
                top + 4.0 * s,
                shortcut,
                [115, 115, 115],
                1.0,
                w + 1.0,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn menu_clamps_to_window_and_skips_disabled_actions() {
        let mut m = Menu::new(590.0, 390.0, 600, 400, false, false);
        assert_eq!((m.x, m.y), (459.0, 288.0));
        m.step(false);
        assert_eq!(m.selected, Some(3));
        m.step(true);
        assert_eq!(m.selected, Some(3));
        assert_eq!(m.at(m.x + 10.0, m.y + 5.0), Some(0));
        assert_eq!(m.at(m.x + 10.0, m.y + 74.0), None);
        assert!(!m.contains(m.x - 1.0, m.y));
    }
}
