use std::time::{Duration, Instant};

#[derive(Clone, Copy, Default)]
pub struct Style {
    pub alpha: f32,
    pub active: bool,
    pub hovered: bool,
}

impl Style {
    #[allow(dead_code)]
    pub fn visible() -> Self {
        Self {
            alpha: 1.0,
            ..Self::default()
        }
    }
}

#[derive(Default)]
pub struct State {
    pub last_scroll: Option<Instant>,
    pub drag: Option<f32>,
    pub hovered: bool,
    pub thumb_hovered: bool,
}

pub fn opacity(elapsed: f32) -> f32 {
    if elapsed <= 2.0 {
        1.0
    } else {
        (1.0 - (elapsed - 2.0).powi(10)).clamp(0.0, 1.0)
    }
}

impl State {
    pub fn changed(&mut self) {
        self.last_scroll = Some(Instant::now());
    }

    pub fn style(&self) -> Style {
        Style {
            alpha: if self.drag.is_some() || self.hovered {
                1.0
            } else {
                self.last_scroll
                    .map_or(0.0, |t| opacity(t.elapsed().as_secs_f32()))
            },
            active: self.drag.is_some() || self.thumb_hovered,
            hovered: self.hovered,
        }
    }

    pub fn next_frame(&self) -> Option<Duration> {
        if self.drag.is_some() || self.hovered {
            return None;
        }
        let elapsed = self.last_scroll?.elapsed();
        Some(if elapsed < Duration::from_secs(2) {
            Duration::from_secs(2) - elapsed
        } else {
            Duration::from_millis(16)
        })
    }

    pub fn finish_frame(&mut self) {
        if !self.hovered
            && self.drag.is_none()
            && self
                .last_scroll
                .is_some_and(|t| t.elapsed() >= Duration::from_secs(3))
        {
            self.last_scroll = None;
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Geometry {
    pub viewport: f32,
    pub content: f32,
    pub start: f32,
    pub length: f32,
}

impl Geometry {
    pub fn new(viewport: f32, content: f32, offset: f32) -> Option<Self> {
        if content <= viewport || viewport <= 8.0 {
            return None;
        }
        let extent = (viewport * viewport / content).max(48.0).min(viewport);
        Some(Self {
            viewport,
            content,
            start: 4.0 + (viewport - extent) * (offset / (content - viewport)).clamp(0.0, 1.0),
            length: extent - 8.0,
        })
    }

    pub fn on_thumb(self, y: f32) -> bool {
        y >= self.start && y < self.start + self.length
    }

    pub fn offset_at(self, y: f32, drag: Option<f32>) -> f32 {
        let denominator = (self.viewport - self.length).max(1.0);
        let percentage = ((y - drag.unwrap_or(self.length / 2.0)) / denominator).clamp(0.0, 1.0);
        let range = self.content - self.viewport;
        (percentage * if drag.is_some() { range } else { self.content }).min(range)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fade_holds_for_two_seconds_then_uses_tenth_power() {
        assert_eq!(opacity(0.0), 1.0);
        assert_eq!(opacity(1.5), 1.0);
        assert_eq!(opacity(2.0), 1.0);
        assert_eq!(opacity(2.5), 1.0 - 0.5f32.powi(10));
        assert_eq!(opacity(3.0), 0.0);
        assert_eq!(opacity(4.0), 0.0);
    }

    #[test]
    fn thumb_geometry_has_four_pixel_insets_and_48_pixel_extent() {
        let g = Geometry::new(327.0, 5000.0, 0.0).unwrap();
        assert_eq!(g.start, 4.0);
        assert_eq!(g.length, 40.0);
        assert!(g.on_thumb(4.0));
        assert!(!g.on_thumb(44.0));
        let bottom = Geometry::new(327.0, 5000.0, 4673.0).unwrap();
        assert_eq!(bottom.start + bottom.length, 323.0);
        assert!(Geometry::new(327.0, 327.0, 0.0).is_none());
        assert!(Geometry::new(0.0, 100.0, 0.0).is_none());
    }

    #[test]
    fn track_click_centers_and_drag_keeps_the_grab_point() {
        let g = Geometry::new(327.0, 1000.0, 0.0).unwrap();
        assert_eq!(g.offset_at(-1.0, None), 0.0);
        assert_eq!(g.offset_at(327.0, None), 673.0);
        assert_eq!(g.offset_at(327.0, Some(5.0)), 673.0);
        let y = 100.0;
        let expected = (y - 5.0) / (327.0 - g.length) * 673.0;
        assert_eq!(g.offset_at(y, Some(5.0)), expected);
        assert_ne!(g.offset_at(y, None), g.offset_at(y, Some(5.0)));
    }

    #[test]
    fn hover_and_drag_keep_thumb_visible_after_idle() {
        let mut state = State::default();
        assert_eq!(state.style().alpha, 0.0);
        assert!(state.next_frame().is_none());
        state.changed();
        assert_eq!(state.style().alpha, 1.0);
        state.last_scroll = Some(Instant::now() - Duration::from_secs(4));
        state.hovered = true;
        assert_eq!(state.style().alpha, 1.0);
        assert!(state.next_frame().is_none());
        state.hovered = false;
        state.drag = Some(5.0);
        assert!(state.style().active);
        assert_eq!(state.style().alpha, 1.0);
        state.drag = None;
        state.finish_frame();
        assert!(state.next_frame().is_none());
    }
}
