//! Keyboard rules for the launcher, kept free of Wayland so they can be tested with plain key symbols.
use crate::items::Action;
use xkbcommon::xkb::keysyms as k;

pub struct KeyIn<'a> {
    pub sym: u32,
    pub utf8: &'a str,
    pub ctrl: bool,
}

/// What the key handler may read and change.
pub struct KeyState<'a> {
    pub query: &'a mut String,
    pub sel: &'a mut usize,
    pub len: usize,
    pub rows: usize,
    /// Cells per row, 1 for a plain list.
    pub columns: usize,
    /// The emoji grid: Left and Right move one cell, Up and Down one row.
    pub grid: bool,
    pub pop_on_backspace: bool,
    pub close_on_escape: bool,
    pub selected: Option<&'a Action>,
}

#[derive(Debug, PartialEq)]
pub enum Effect {
    /// Nothing to do and the key should not repeat.
    Ignore,
    /// The query changed: search again and redraw.
    Edited,
    /// The selection moved: redraw only.
    Moved,
    Activate,
    Hide,
    /// Left or Right on a settings row: step it by `dir`.
    Step(String, i32),
    /// Delete the highlighted clipboard entry.
    DeleteClip,
}

impl Effect {
    pub fn repeats(&self) -> bool {
        matches!(self, Effect::Edited | Effect::Moved)
    }
}

/// Where Backspace goes from a view with nothing typed in it, `None` when the query is not a bare view root.
pub fn view_root_parent(q: &str) -> Option<&'static str> {
    match q.trim_end() {
        ":" | ";" | "settings" => Some(""),
        "settings apps" => Some("settings"),
        _ => None,
    }
}

/// Drop the last visible character: a base character with any marks, joiners,
/// variation selectors or skin tones attached to it goes together.
pub fn pop_cluster(s: &mut String) {
    while let Some(c) = s.pop() {
        let attached = matches!(c as u32,
            0x0300..=0x036F | 0x200D | 0xFE00..=0xFE0F | 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F);
        // A base character stops the loop unless a joiner links it to the one before.
        if !attached && !s.ends_with('\u{200D}') {
            break;
        }
    }
}

/// Delete the word before the end, like Ctrl+Backspace: trailing spaces go with it.
pub fn pop_word(s: &mut String) {
    let t = s.trim_end().rfind(' ').map_or(0, |i| i + 1);
    s.truncate(t);
}

pub fn handle(st: &mut KeyState, key: &KeyIn) -> Effect {
    let last = st.len.saturating_sub(1);
    let go = |sel: &mut usize, d: isize| {
        if st.len > 0 {
            *sel = (*sel as isize + d).clamp(0, last as isize) as usize;
        }
        Effect::Moved
    };
    // One step that wraps round at the ends, like zlaunch's Up, Down and Tab.
    let wrap = |sel: &mut usize, d: isize| {
        if st.len > 0 {
            *sel = (*sel as isize + d).rem_euclid(st.len as isize) as usize;
        }
        Effect::Moved
    };
    match key.sym {
        k::KEY_Escape => {
            // A view (emoji, clipboard, settings) steps back to the main list; the main list hides at once.
            let in_view = st.query.starts_with([':', ';']) || st.query.starts_with("settings");
            if in_view && !st.close_on_escape {
                st.query.clear();
                Effect::Edited
            } else {
                Effect::Hide
            }
        }
        k::KEY_Return | k::KEY_KP_Enter => Effect::Activate,
        k::KEY_BackSpace => {
            if key.ctrl {
                pop_word(st.query);
                Effect::Edited
            } else if let Some(back) = view_root_parent(st.query) {
                if st.pop_on_backspace {
                    *st.query = back.into();
                    Effect::Edited
                } else {
                    Effect::Moved
                }
            } else {
                pop_cluster(st.query);
                Effect::Edited
            }
        }
        k::KEY_comma if key.ctrl => {
            *st.query = "settings".into();
            Effect::Edited
        }
        k::KEY_Down | k::KEY_Up if st.grid => {
            *st.sel =
                crate::ui::emoji_vertical(*st.sel, st.len, st.columns, key.sym == k::KEY_Down);
            Effect::Moved
        }
        k::KEY_Right | k::KEY_Left if st.grid => {
            *st.sel = crate::ui::emoji_horizontal(*st.sel, st.len, key.sym == k::KEY_Right);
            Effect::Moved
        }
        k::KEY_Left | k::KEY_Right => match st.selected {
            Some(Action::Setting(name)) => {
                Effect::Step(name.clone(), if key.sym == k::KEY_Left { -1 } else { 1 })
            }
            _ => Effect::Ignore,
        },
        k::KEY_Delete if st.query.starts_with(';') => Effect::DeleteClip,
        k::KEY_Down | k::KEY_Tab => wrap(st.sel, 1),
        k::KEY_Up | k::KEY_ISO_Left_Tab => wrap(st.sel, -1),
        k::KEY_Page_Down => go(st.sel, (st.rows * st.columns) as isize),
        k::KEY_Page_Up => go(st.sel, -((st.rows * st.columns) as isize)),
        k::KEY_Home => {
            *st.sel = 0;
            Effect::Moved
        }
        k::KEY_End => {
            *st.sel = last;
            Effect::Moved
        }
        _ if key.ctrl => match key.utf8 {
            "\u{15}" => {
                st.query.clear();
                Effect::Edited
            }
            "\u{17}" => {
                pop_word(st.query);
                Effect::Edited
            }
            "\u{e}" | "\u{a}" => wrap(st.sel, 1),
            "\u{10}" | "\u{b}" => wrap(st.sel, -1),
            "\u{7}" => Effect::Hide,
            "\u{18}" if st.query.starts_with(';') => Effect::DeleteClip,
            _ => Effect::Ignore,
        },
        _ => {
            if !key.utf8.is_empty() && !key.utf8.chars().any(|c| c.is_control()) {
                st.query.push_str(key.utf8);
                Effect::Edited
            } else {
                Effect::Ignore
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Rig {
        query: String,
        sel: usize,
        len: usize,
        pop: bool,
        close: bool,
        columns: usize,
        grid: bool,
        selected: Option<Action>,
    }
    impl Rig {
        fn new(query: &str, len: usize) -> Self {
            Rig {
                query: query.into(),
                sel: 0,
                len,
                pop: true,
                close: false,
                columns: 1,
                grid: false,
                selected: None,
            }
        }
        fn press(&mut self, sym: u32, utf8: &str, ctrl: bool) -> Effect {
            let mut st = KeyState {
                query: &mut self.query,
                sel: &mut self.sel,
                len: self.len,
                rows: 8,
                columns: self.columns,
                grid: self.grid,
                pop_on_backspace: self.pop,
                close_on_escape: self.close,
                selected: self.selected.as_ref(),
            };
            handle(&mut st, &KeyIn { sym, utf8, ctrl })
        }
        fn typ(&mut self, s: &str) {
            for c in s.chars() {
                assert_eq!(self.press(c as u32, &c.to_string(), false), Effect::Edited);
            }
        }
    }

    #[test]
    fn typing_and_backspace_edit_the_query() {
        let mut r = Rig::new("", 3);
        r.typ("fire");
        assert_eq!(r.query, "fire");
        r.press(k::KEY_BackSpace, "", false);
        assert_eq!(r.query, "fir");
    }
    #[test]
    fn control_characters_are_not_typed() {
        let mut r = Rig::new("a", 1);
        assert_eq!(r.press(k::KEY_Tab, "\t", false), Effect::Moved);
        assert_eq!(r.press(k::KEY_F1, "", false), Effect::Ignore);
        assert_eq!(r.query, "a");
    }
    #[test]
    fn ctrl_backspace_and_ctrl_w_delete_a_word() {
        let mut r = Rig::new("one two ", 1);
        r.press(k::KEY_BackSpace, "", true);
        assert_eq!(r.query, "one ");
        r.press(k::KEY_w, "\u{17}", true);
        assert_eq!(r.query, "");
        let mut r = Rig::new("abc", 1);
        r.press(k::KEY_u, "\u{15}", true);
        assert_eq!(r.query, "");
    }
    #[test]
    fn backspace_removes_a_whole_emoji() {
        let mut s = "a👍🏽".to_string();
        pop_cluster(&mut s);
        assert_eq!(s, "a");
        let mut s = "x👨\u{200D}👩\u{200D}👧".to_string();
        pop_cluster(&mut s);
        assert_eq!(s, "x");
        let mut s = "e\u{301}".to_string();
        pop_cluster(&mut s);
        assert_eq!(s, "");
        let mut s = String::new();
        pop_cluster(&mut s);
        assert_eq!(s, "");
    }
    #[test]
    fn arrows_paging_and_home_end_clamp() {
        let mut r = Rig::new("", 20);
        r.press(k::KEY_Up, "", false);
        assert_eq!(r.sel, 19);
        r.press(k::KEY_Down, "", false);
        assert_eq!(r.sel, 0);
        r.press(k::KEY_ISO_Left_Tab, "", false);
        assert_eq!(r.sel, 19);
        r.press(k::KEY_Tab, "\t", false);
        assert_eq!(r.sel, 0);
        r.press(k::KEY_Page_Down, "", false);
        assert_eq!(r.sel, 8);
        r.press(k::KEY_End, "", false);
        assert_eq!(r.sel, 19);
        r.press(k::KEY_Page_Down, "", false);
        assert_eq!(r.sel, 19);
        r.press(k::KEY_Home, "", false);
        assert_eq!(r.sel, 0);
        r.press(k::KEY_n, "\u{e}", true);
        assert_eq!(r.sel, 1);
        r.press(k::KEY_p, "\u{10}", true);
        assert_eq!(r.sel, 0);
        let mut empty = Rig::new("", 0);
        assert_eq!(empty.press(k::KEY_End, "", false), Effect::Moved);
        assert_eq!(empty.sel, 0);
    }
    #[test]
    fn escape_steps_out_of_a_view_and_hides_from_the_main_list() {
        let mut r = Rig::new(":rocket", 1);
        assert_eq!(r.press(k::KEY_Escape, "", false), Effect::Edited);
        assert_eq!(r.query, "");
        assert_eq!(r.press(k::KEY_Escape, "", false), Effect::Hide);
        let mut r = Rig::new("fire", 1);
        assert_eq!(r.press(k::KEY_Escape, "", false), Effect::Hide);
        let mut r = Rig::new(";", 1);
        r.close = true;
        assert_eq!(r.press(k::KEY_Escape, "", false), Effect::Hide);
    }
    #[test]
    fn backspace_on_a_view_root_follows_the_setting() {
        let mut r = Rig::new("settings apps ", 1);
        assert_eq!(r.press(k::KEY_BackSpace, "", false), Effect::Edited);
        assert_eq!(r.query, "settings");
        r.press(k::KEY_BackSpace, "", false);
        assert_eq!(r.query, "");
        let mut r = Rig::new(":", 1);
        r.pop = false;
        assert_eq!(r.press(k::KEY_BackSpace, "", false), Effect::Moved);
        assert_eq!(r.query, ":");
        let mut r = Rig::new(":ro", 1);
        r.pop = false;
        r.press(k::KEY_BackSpace, "", false);
        assert_eq!(r.query, ":r");
    }
    #[test]
    fn emoji_grid_moves_by_cell_and_row() {
        let mut r = Rig::new(":", 100);
        r.grid = true;
        r.columns = 8;
        r.press(k::KEY_Right, "", false);
        assert_eq!(r.sel, 1);
        r.press(k::KEY_Down, "", false);
        assert_eq!(r.sel, 9);
        r.press(k::KEY_Up, "", false);
        assert_eq!(r.sel, 1);
        r.press(k::KEY_Left, "", false);
        assert_eq!(r.sel, 0);
        r.press(k::KEY_Page_Down, "", false);
        assert_eq!(r.sel, 64);
    }
    #[test]
    fn ctrl_comma_opens_settings() {
        let mut r = Rig::new("fire", 1);
        assert_eq!(r.press(k::KEY_comma, ",", true), Effect::Edited);
        assert_eq!(r.query, "settings");
    }
    #[test]
    fn left_right_step_only_settings_rows() {
        let mut r = Rig::new("settings", 2);
        assert_eq!(r.press(k::KEY_Left, "", false), Effect::Ignore);
        r.selected = Some(Action::Setting("width".into()));
        assert_eq!(
            r.press(k::KEY_Left, "", false),
            Effect::Step("width".into(), -1)
        );
        assert_eq!(
            r.press(k::KEY_Right, "", false),
            Effect::Step("width".into(), 1)
        );
    }
    #[test]
    fn delete_removes_clipboard_entries_only_in_history() {
        let mut r = Rig::new(";", 2);
        assert_eq!(r.press(k::KEY_Delete, "", false), Effect::DeleteClip);
        assert_eq!(r.press(k::KEY_x, "\u{18}", true), Effect::DeleteClip);
        let mut r = Rig::new("fire", 2);
        assert_eq!(r.press(k::KEY_Delete, "", false), Effect::Ignore);
    }
    #[test]
    fn only_edits_and_moves_repeat() {
        assert!(Effect::Edited.repeats() && Effect::Moved.repeats());
        assert!(!Effect::Activate.repeats() && !Effect::Hide.repeats());
    }
}
