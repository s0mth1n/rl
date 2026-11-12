//! Caret and selection state for the single-line search input.
use unicode_segmentation::UnicodeSegmentation;

#[derive(Default)]
pub struct Edit {
    pub caret: usize,
    pub anchor: Option<usize>,
    snapshot: String,
    undo: Vec<(String, usize, Option<usize>)>,
    redo: Vec<(String, usize, Option<usize>)>,
}

impl Edit {
    pub fn sync(&mut self, text: &str) {
        if self.snapshot != text {
            self.caret = text.len();
            self.anchor = None;
            self.snapshot = text.into();
            self.undo.clear();
            self.redo.clear();
        }
    }

    pub fn range(&self) -> Option<std::ops::Range<usize>> {
        self.anchor
            .filter(|&a| a != self.caret)
            .map(|a| a.min(self.caret)..a.max(self.caret))
    }

    pub fn move_to(&mut self, text: &str, to: usize, extend: bool) {
        let mut to = to.min(text.len()).max(usize::from(text.starts_with('#')));
        while !text.is_char_boundary(to) {
            to -= 1;
        }
        self.anchor = if extend {
            Some(
                self.anchor
                    .unwrap_or(self.caret)
                    .max(usize::from(text.starts_with('#'))),
            )
        } else {
            None
        };
        self.caret = to;
    }

    pub fn select_all(&mut self, text: &str) {
        self.anchor = Some(usize::from(text.starts_with('#')));
        self.caret = text.len();
    }

    pub fn replace(&mut self, text: &mut String, insert: &str) {
        let mut r = self.range().unwrap_or(self.caret..self.caret);
        let start = usize::from(text.starts_with('#'));
        r.start = r.start.max(start);
        r.end = r.end.max(start);
        if text[r.clone()] == *insert {
            return;
        }
        if self.undo.len() == 100 {
            self.undo.remove(0);
        }
        self.undo.push((text.clone(), self.caret, self.anchor));
        self.redo.clear();
        text.replace_range(r.clone(), insert);
        self.caret = r.start + insert.len();
        self.anchor = None;
        self.snapshot.clone_from(text);
    }

    pub fn step(&mut self, text: &str, right: bool, word: bool, extend: bool) {
        let to = if let Some(r) = self.range().filter(|_| !extend) {
            if right {
                r.end
            } else {
                r.start
            }
        } else if word {
            if right {
                text.split_word_bound_indices()
                    .find(|(i, s)| *i + s.len() > self.caret && !s.trim().is_empty())
                    .map_or(text.len(), |(i, s)| i + s.len())
            } else {
                text.split_word_bound_indices()
                    .rev()
                    .find(|(i, s)| *i < self.caret && !s.trim().is_empty())
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
        let Some((saved, caret, anchor)) = from.pop() else {
            return false;
        };
        to.push((std::mem::replace(text, saved), self.caret, self.anchor));
        self.caret = caret;
        self.anchor = anchor;
        self.snapshot.clone_from(text);
        true
    }

    pub fn select_word(&mut self, text: &str, at: usize) {
        let r = text
            .split_word_bound_indices()
            .find(|(i, s)| *i <= at && at < *i + s.len())
            .map(|(i, s)| i..i + s.len())
            .unwrap_or(text.len()..text.len());
        self.anchor = Some(r.start.max(usize::from(text.starts_with('#'))));
        self.caret = r.end;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn theme_mode_marker_is_outside_editable_text() {
        let mut text = "#nord".to_string();
        let mut e = Edit::default();
        e.sync(&text);
        e.select_all(&text);
        assert_eq!(&text[e.range().unwrap()], "nord");
        e.replace(&mut text, "dracula");
        assert_eq!(text, "#dracula");
        e.move_to(&text, 0, false);
        assert_eq!(e.caret, 1);
        e.delete(&mut text, false, true);
        assert_eq!(text, "#dracula");
        e.select_all(&text);
        e.replace(&mut text, "");
        assert_eq!(text, "#");
        e.delete(&mut text, true, false);
        e.delete(&mut text, false, false);
        assert_eq!(text, "#");
        assert!(e.history(&mut text, false));
        assert_eq!(text, "#dracula");
        e.anchor = Some(0);
        e.caret = text.len();
        e.replace(&mut text, "nord");
        assert_eq!(text, "#nord");
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
        assert_eq!(e.range(), Some(0..3));
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
