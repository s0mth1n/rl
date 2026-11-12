//! Caret and selection state for the single-line search input.
use std::time::{Duration, Instant};
use unicode_segmentation::UnicodeSegmentation;

type Entry = (String, usize, usize, usize);

#[derive(Default)]
pub struct Edit {
    pub caret: usize,
    pub literal_prefix: bool,
    pub anchor: Option<usize>,
    snapshot: String,
    selected_word: Option<std::ops::Range<usize>>,
    undo: Vec<Entry>,
    redo: Vec<Entry>,
    version: usize,
    last_change: Option<Instant>,
}

impl Edit {
    pub fn prefix_len(&self, text: &str) -> usize {
        usize::from(!self.literal_prefix && text.starts_with([':', ';', '#']))
    }

    pub fn sync(&mut self, text: &str) {
        if self.snapshot != text {
            self.caret = text.len();
            self.anchor = None;
            self.selected_word = None;
            self.snapshot = text.into();
            self.undo.clear();
            self.redo.clear();
            self.last_change = None;
        }
    }

    pub fn range(&self) -> Option<std::ops::Range<usize>> {
        self.anchor
            .filter(|&a| a != self.caret)
            .map(|a| a.min(self.caret)..a.max(self.caret))
    }

    pub fn move_to(&mut self, text: &str, to: usize, extend: bool) {
        let mut to = to.min(text.len()).max(self.prefix_len(text));
        while !text.is_char_boundary(to) {
            to -= 1;
        }
        self.anchor = if extend {
            Some(self.anchor.unwrap_or(self.caret).max(self.prefix_len(text)))
        } else {
            None
        };
        self.caret = to;
        if extend {
            if let Some(word) = self.selected_word.clone() {
                let range = self.range().unwrap_or(to..to);
                let start = range.start.min(word.start);
                let end = range.end.max(word.end);
                if to < word.start {
                    self.caret = start;
                    self.anchor = Some(end);
                } else {
                    self.caret = end;
                    self.anchor = Some(start);
                }
            }
        } else {
            self.selected_word = None;
        }
    }

    pub fn end_drag(&mut self) {
        self.selected_word = None;
    }

    pub fn select_all(&mut self, text: &str) {
        self.selected_word = None;
        self.anchor = Some(self.prefix_len(text));
        self.caret = text.len();
    }

    pub fn continue_group(&mut self) {
        if self.last_change.is_some() {
            self.last_change = Some(Instant::now());
        }
    }

    pub fn replace(&mut self, text: &mut String, insert: &str) {
        self.replace_at(text, insert, Instant::now());
    }

    fn replace_at(&mut self, text: &mut String, insert: &str, now: Instant) {
        let mut r = self.range().unwrap_or(self.caret..self.caret);
        let start = self.prefix_len(text);
        r.start = r.start.max(start);
        r.end = r.end.max(start);
        if text[r.clone()] == *insert {
            return;
        }
        if self
            .last_change
            .is_none_or(|at| now.saturating_duration_since(at) > Duration::from_secs(1))
        {
            self.version += 1;
        }
        self.last_change = Some(now);
        if self.undo.len() == 1000 {
            self.undo.remove(0);
        }
        self.undo
            .push((text.clone(), r.end, r.start + insert.len(), self.version));
        self.redo.clear();
        text.replace_range(r.clone(), insert);
        self.caret = r.start + insert.len();
        self.anchor = None;
        self.selected_word = None;
        self.snapshot.clone_from(text);
    }

    pub fn step(&mut self, text: &str, right: bool, word: bool, extend: bool) {
        let to = if let Some(r) = self.range().filter(|_| !extend && !word) {
            if right {
                r.end
            } else {
                r.start
            }
        } else if word {
            if right {
                text[self.caret..]
                    .split_word_bound_indices()
                    .find(|(_, s)| !s.trim().is_empty())
                    .map_or(text.len(), |(i, s)| self.caret + i + s.len())
            } else {
                let at = self.range().map_or(self.caret, |r| r.start);
                text[..at]
                    .split_word_bound_indices()
                    .rev()
                    .find(|(_, s)| !s.trim().is_empty())
                    .map_or(0, |(i, _)| i)
            }
        } else if right {
            text.grapheme_indices(true)
                .map(|(i, _)| i)
                .find(|&i| i > self.caret)
                .unwrap_or(text.len())
        } else {
            text.grapheme_indices(true)
                .map(|(i, _)| i)
                .take_while(|&i| i < self.caret)
                .last()
                .unwrap_or(0)
        };
        self.move_to(text, to, extend);
    }

    pub fn delete(&mut self, text: &mut String, right: bool, word: bool) {
        if self.range().is_none() {
            self.step(text, right, word, true);
        }
        self.replace(text, "");
    }

    pub fn history(&mut self, text: &mut String, redo: bool) -> bool {
        let (from, to) = if redo {
            (&mut self.redo, &mut self.undo)
        } else {
            (&mut self.undo, &mut self.redo)
        };
        let Some(entry) = from.pop() else {
            return false;
        };
        let version = entry.3;
        let mut entry = Some(entry);
        while let Some((saved, caret, opposite, _)) = entry {
            to.push((std::mem::replace(text, saved), opposite, caret, version));
            self.caret = caret;
            self.anchor = None;
            entry = if from.last().is_some_and(|e| e.3 == version) {
                from.pop()
            } else {
                None
            };
        }
        self.selected_word = None;
        self.snapshot.clone_from(text);
        true
    }

    pub fn select_word(&mut self, text: &str, at: usize) {
        let mut at = at.min(text.len());
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        let Some(ch) = text[at..].chars().next() else {
            return;
        };
        let class = selection_class(ch);
        let mut start = at;
        let mut end = at + ch.len_utf8();
        if class != 0 {
            for (index, ch) in text[..at].char_indices().rev().take(128) {
                if selection_class(ch) != class {
                    break;
                }
                start = index;
            }
            for ch in text[end..].chars().take(128) {
                if selection_class(ch) != class {
                    break;
                }
                end += ch.len_utf8();
            }
        }
        self.anchor = Some(start.max(self.prefix_len(text)));
        self.caret = end;
        self.selected_word = self.range();
    }
}

fn selection_class(c: char) -> u8 {
    if c == '_'
        || c.is_ascii_alphanumeric()
        || matches!(c as u32,
        0xC0..=0x24F | 0x300..=0x36F | 0x400..=0x4FF | 0x1E00..=0x1EFF)
    {
        1
    } else if c.is_whitespace() && c != '\n' && c != '\r' {
        2
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composition_can_finish_the_selection_replacement_history_group() {
        let mut text = String::from("original");
        let mut e = Edit::default();
        e.sync(&text);
        e.select_all(&text);
        e.replace(&mut text, "");
        e.last_change = Some(Instant::now() - Duration::from_secs(2));
        e.continue_group();
        e.replace(&mut text, "committed");
        assert!(e.history(&mut text, false));
        assert_eq!(text, "original");
        assert!(e.history(&mut text, true));
        assert_eq!(text, "committed");
    }

    #[test]
    fn clear_is_undoable_without_leaving_the_mode() {
        for initial in ["sample", ":sample", ";sample", "#sample"] {
            let mut text = initial.to_string();
            let mut e = Edit::default();
            e.sync(&text);
            e.select_all(&text);
            e.replace(&mut text, "");
            assert_eq!(
                text,
                &initial[..usize::from(initial.starts_with([':', ';', '#']))]
            );
            assert!(e.history(&mut text, false));
            assert_eq!(text, initial);
            assert!(e.history(&mut text, true));
            assert_eq!(
                text,
                &initial[..usize::from(initial.starts_with([':', ';', '#']))]
            );
        }
    }

    #[test]
    fn double_click_drag_keeps_the_initial_word_until_release() {
        let text = "one two three";
        let mut e = Edit::default();
        e.sync(text);
        e.select_word(text, 5);
        e.move_to(text, 5, true);
        assert_eq!(e.range(), Some(4..7));
        e.move_to(text, 1, true);
        assert_eq!(e.range(), Some(1..7));
        e.move_to(text, 11, true);
        assert_eq!(e.range(), Some(4..11));
        e.end_drag();
        e.move_to(text, 5, true);
        assert_eq!(e.range(), Some(4..5));
        e.select_word(text, 5);
        e.move_to(text, 5, false);
        assert!(e.range().is_none());
    }

    #[test]
    fn double_click_groups_gpui_letters_and_spaces_but_not_punctuation() {
        let mut e = Edit::default();
        for (text, at, expected) in [
            ("can't", 0, "can"),
            ("can't", 3, "'"),
            ("café_42", 3, "café_42"),
            ("Привет мир", 4, "Привет"),
            ("a  \tb", 2, "  \t"),
            ("中文", 0, "中"),
            ("😀😀", 0, "😀"),
            ("e\u{301}", 1, "e\u{301}"),
        ] {
            e.sync(text);
            e.select_word(text, at);
            assert_eq!(&text[e.range().unwrap()], expected);
        }
        let text = "x".repeat(300);
        e.sync(&text);
        e.select_word(&text, 150);
        assert_eq!(e.range(), Some(22..279));
        let range = e.range();
        e.select_word(&text, text.len());
        assert_eq!(e.range(), range);
    }
    #[test]
    fn control_right_splits_words_from_the_caret() {
        let mut e = Edit::default();
        let text = "can't stop";
        e.sync(text);
        e.move_to(text, 3, false);
        e.step(text, true, true, false);
        assert_eq!(e.caret, 4);
        e.step(text, true, true, false);
        assert_eq!(e.caret, 5);
        e.step(text, true, true, true);
        assert_eq!(e.caret, text.len());
        assert_eq!(&text[e.range().unwrap()], " stop");
        e.step(text, true, true, false);
        assert_eq!(e.caret, text.len());
        assert!(e.range().is_none());
    }

    #[test]
    fn control_word_moves_do_not_only_collapse_selection() {
        let mut e = Edit::default();
        let text = "one two three";
        e.sync(text);
        e.move_to(text, 0, false);
        e.move_to(text, 4, true);
        e.step(text, true, true, false);
        assert_eq!(e.caret, 7);
        assert!(e.range().is_none());
        e.move_to(text, 4, false);
        e.move_to(text, 7, true);
        e.step(text, false, true, false);
        assert_eq!(e.caret, 0);
    }

    #[test]
    fn undo_redo_places_the_caret_at_the_replaced_range_end() {
        let mut e = Edit::default();
        let mut text = "one two".to_string();
        e.sync(&text);
        e.move_to(&text, 4, false);
        e.move_to(&text, 7, true);
        e.replace(&mut text, "x");
        e.move_to(&text, 0, false);
        assert!(e.history(&mut text, false));
        assert_eq!(text, "one two");
        assert_eq!(e.caret, 7);
        assert!(e.range().is_none());
        e.move_to(&text, 0, false);
        assert!(e.history(&mut text, true));
        assert_eq!(text, "one x");
        assert_eq!(e.caret, 5);
        assert!(e.range().is_none());
    }

    #[test]
    fn undo_groups_edits_within_one_second_and_redo_replays_them() {
        let mut e = Edit::default();
        let mut text = String::new();
        e.sync(&text);
        let at = Instant::now();
        e.replace_at(&mut text, "a", at);
        e.replace_at(&mut text, "b", at + Duration::from_millis(500));
        e.replace_at(&mut text, "c", at + Duration::from_millis(1500));
        e.replace_at(&mut text, "d", at + Duration::from_millis(2501));
        assert!(e.history(&mut text, false));
        assert_eq!(text, "abc");
        assert!(e.history(&mut text, false));
        assert_eq!(text, "");
        assert!(e.history(&mut text, true));
        assert_eq!(text, "abc");
        assert!(e.history(&mut text, true));
        assert_eq!(text, "abcd");
    }

    #[test]
    fn mode_markers_are_outside_editable_text() {
        for marker in [':', ';', '#'] {
            let mut text = format!("{marker}nord");
            let mut e = Edit::default();
            e.sync(&text);
            e.select_all(&text);
            assert_eq!(&text[e.range().unwrap()], "nord");
            e.replace(&mut text, "dracula");
            assert_eq!(text, format!("{marker}dracula"));
            e.move_to(&text, 0, false);
            assert_eq!(e.caret, 1);
            e.delete(&mut text, false, true);
            assert_eq!(text, format!("{marker}dracula"));
            e.select_all(&text);
            e.replace(&mut text, "");
            assert_eq!(text, marker.to_string());
            e.delete(&mut text, true, false);
            e.delete(&mut text, false, false);
            assert_eq!(text, marker.to_string());
            assert!(e.history(&mut text, false));
            assert_eq!(text, format!("{marker}nord"));
            e.anchor = Some(0);
            e.caret = text.len();
            e.replace(&mut text, "nord");
            assert_eq!(text, format!("{marker}nord"));
        }
    }
    #[test]
    fn undo_redo_restores_selection_and_replacement() {
        let mut e = Edit::default();
        let mut t = "one".to_string();
        e.sync(&t);
        e.select_all(&t);
        e.replace(&mut t, "two");
        assert!(e.history(&mut t, false));
        assert_eq!(t, "one");
        assert_eq!(e.range(), None);
        assert_eq!(e.caret, 3);
        assert!(e.history(&mut t, true));
        assert_eq!(t, "two");
        e.replace(&mut t, "!");
        assert!(!e.history(&mut t, true));
        e.sync("external");
        assert!(!e.history(&mut t, false));
    }

    #[test]
    fn typing_replaces_selection_at_a_unicode_caret() {
        let mut text = "a👩🏽‍💻e\u{301}z".to_string();
        let mut e = Edit::default();
        e.sync(&text);
        e.step(&text, false, false, false);
        e.step(&text, false, false, true);
        assert_eq!(&text[e.range().unwrap()], "e\u{301}");
        e.replace(&mut text, "é");
        assert_eq!(text, "a👩🏽‍💻éz");
        e.delete(&mut text, false, false);
        assert_eq!(text, "a👩🏽‍💻z");
        e.delete(&mut text, false, false);
        assert_eq!(text, "az");
        e.delete(&mut text, true, false);
        assert_eq!(text, "a");
        e.select_all(&text);
        e.replace(&mut text, "");
        assert_eq!(text, "");
        e.delete(&mut text, false, false);
        assert_eq!(e.caret, 0);
    }
    #[test]
    fn words_selection_collapse_and_external_resets() {
        let mut e = Edit::default();
        let mut t = "one café three".to_string();
        e.sync(&t);
        e.step(&t, false, true, true);
        assert_eq!(&t[e.range().unwrap()], "three");
        e.step(&t, false, false, false);
        assert_eq!(e.caret, 10);
        assert!(e.range().is_none());
        e.select_word(&t, 5);
        assert_eq!(&t[e.range().unwrap()], "café");
        e.replace(&mut t, "two");
        assert_eq!(t, "one two three");
        e.sync(":");
        assert_eq!(e.caret, 1);
        assert!(e.anchor.is_none());
    }
}

pub fn paste_text(text: &str) -> String {
    text.replace('\n', "")
}

#[cfg(test)]
mod paste_tests {
    #[test]
    fn single_line_paste_drops_newlines_without_replacing_other_characters() {
        assert_eq!(super::paste_text("one\ntwo\tthree\r\n"), "onetwo\tthree\r");
        assert_eq!(super::paste_text("café 😀"), "café 😀");
    }
}
