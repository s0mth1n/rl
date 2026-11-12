//! Transactional text-input state. Protocol indices are UTF-8 byte offsets.
use crate::input::Edit;
use std::ops::Range;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preedit {
    pub text: String,
    pub begin: i32,
    pub end: i32,
}

#[derive(Default)]
pub struct State {
    pub preedit: Option<Preedit>,
    pub pending_preedit: Option<Preedit>,
    pub commit: Option<String>,
    pub delete: (u32, u32),
    removed_selection: bool,
}

pub fn boundary(text: &str, index: usize) -> usize {
    let mut index = index.min(text.len());
    while !text.is_char_boundary(index) {
        index -= 1;
    }
    index
}

impl State {
    pub fn clear(&mut self) -> bool {
        let changed = self.removed_selection;
        *self = Self::default();
        changed
    }

    pub fn apply(&mut self, query: &mut String, edit: &mut Edit) -> bool {
        edit.sync(query);
        self.preedit = None;
        let mut changed = self.removed_selection
            && self
                .pending_preedit
                .as_ref()
                .is_none_or(|p| p.text.is_empty());
        let selected = edit.range().unwrap_or(edit.caret..edit.caret);
        let prefix = edit.prefix_len(query);
        let before = self.delete.0 as usize;
        let after = self.delete.1 as usize;
        if before > 0 || after > 0 {
            let start = boundary(query, selected.start.saturating_sub(before)).max(prefix);
            let mut end = selected.end.saturating_add(after).min(query.len());
            while !query.is_char_boundary(end) {
                end += 1;
            }
            let reversed = edit.anchor.is_some_and(|anchor| anchor > edit.caret);
            edit.move_to(query, selected.end, false);
            edit.move_to(query, end, true);
            edit.replace(query, "");
            edit.move_to(query, start, false);
            edit.move_to(query, selected.start, true);
            edit.replace(query, "");
            let remaining_end = start + selected.len();
            edit.move_to(query, if reversed { remaining_end } else { start }, false);
            edit.move_to(query, if reversed { start } else { remaining_end }, true);
            changed = true;
        }
        self.delete = (0, 0);
        if let Some(text) = self.commit.take() {
            if self.removed_selection {
                edit.continue_group();
            }
            self.removed_selection = false;
            edit.replace(query, &crate::input::paste_text(&text));
            changed = true;
        }
        if let Some(preedit) = self.pending_preedit.take().filter(|p| !p.text.is_empty()) {
            if edit.range().is_some() {
                edit.replace(query, "");
                self.removed_selection = true;
            }
            self.preedit = Some(preedit);
        } else {
            self.removed_selection = false;
        }
        changed
    }

    pub fn display(&self, query: &str, edit: &Edit) -> Option<Display> {
        let preedit = self.preedit.as_ref()?;
        let at = boundary(query, edit.caret);
        let mut text = query.to_string();
        text.insert_str(at, &preedit.text);
        let hidden = preedit.begin < 0 && preedit.end < 0;
        let begin = boundary(&preedit.text, preedit.begin.max(0) as usize) + at;
        let end = boundary(&preedit.text, preedit.end.max(0) as usize) + at;
        Some(Display {
            text,
            caret: begin,
            anchor: (begin != end).then_some(end),
            marked: at..at + preedit.text.len(),
            hidden,
        })
    }
}

#[derive(PartialEq, Eq)]
pub struct Snapshot {
    pub surrounding: Option<(String, i32, i32)>,
    pub rect: (i32, i32, i32, i32),
}

pub struct Display {
    pub text: String,
    pub caret: usize,
    pub anchor: Option<usize>,
    pub marked: Range<usize>,
    pub hidden: bool,
}

/// Keep at most 4000 bytes without cutting a code point or the selection.
#[cfg(test)]
pub fn surrounding(query: &str, caret: usize, anchor: usize) -> Option<(String, i32, i32)> {
    surrounding_with_prefix(
        query,
        caret,
        anchor,
        usize::from(query.starts_with([':', ';', '#'])),
    )
}

pub fn surrounding_with_prefix(
    query: &str,
    caret: usize,
    anchor: usize,
    prefix: usize,
) -> Option<(String, i32, i32)> {
    let query = &query[prefix..];
    let caret = boundary(query, caret.saturating_sub(prefix));
    let anchor = boundary(query, anchor.saturating_sub(prefix));
    let left = caret.min(anchor);
    let right = caret.max(anchor);
    if right - left > 4000 {
        return None;
    }
    let mut start = boundary(query, left.saturating_sub((4000 - (right - left)) / 2));
    let mut end = boundary(query, (start + 4000).min(query.len()));
    if end < right {
        end = right;
    }
    if end - start > 4000 {
        start = end - 4000;
        while !query.is_char_boundary(start) {
            start += 1;
        }
    }
    Some((
        query[start..end].to_string(),
        (caret - start) as i32,
        (anchor - start) as i32,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn batches_replace_preedit_and_commit_once_without_leaking_mode_prefix() {
        let mut query = ";café".to_string();
        let mut e = Edit::default();
        e.sync(&query);
        e.select_all(&query);
        let mut s = State {
            pending_preedit: Some(Preedit {
                text: "に".into(),
                begin: 3,
                end: 3,
            }),
            ..State::default()
        };
        assert!(!s.apply(&mut query, &mut e));
        assert_eq!(query, ";");
        let shown = s.display(&query, &e).unwrap();
        assert_eq!(shown.text, ";に");
        assert_eq!(shown.caret, 4);
        assert_eq!(shown.marked, 1..4);
        s.pending_preedit = Some(Preedit {
            text: "日本".into(),
            begin: 0,
            end: 6,
        });
        assert!(!s.apply(&mut query, &mut e));
        assert_eq!(s.display(&query, &e).unwrap().text, ";日本");
        s.commit = Some("日本".into());
        assert!(s.apply(&mut query, &mut e));
        assert_eq!(query, ";日本");
        assert!(s.preedit.is_none());
        assert!(!s.apply(&mut query, &mut e));
        assert_eq!(query, ";日本");
        assert_eq!(
            surrounding(&query, e.caret, e.caret),
            Some(("日本".into(), 6, 6))
        );
    }
    #[test]
    fn delete_around_selection_clamps_unicode_and_protects_prefix() {
        let mut query = ":éab界z".to_string();
        let mut e = Edit::default();
        e.sync(&query);
        e.move_to(&query, 3, false);
        e.move_to(&query, 5, true);
        let mut s = State {
            delete: (1, 2),
            commit: Some("x".into()),
            ..State::default()
        };
        assert!(s.apply(&mut query, &mut e));
        assert_eq!(query, ":xz");
        s.delete = (u32::MAX, u32::MAX);
        s.apply(&mut query, &mut e);
        assert_eq!(query, ":");
    }
    #[test]
    fn surrounding_deletion_preserves_selection_when_nothing_is_committed() {
        let mut query = String::from("xéab界z");
        let mut e = Edit::default();
        e.sync(&query);
        e.move_to(&query, 3, false);
        e.move_to(&query, 5, true);
        let mut s = State {
            delete: (2, 3),
            ..State::default()
        };
        s.apply(&mut query, &mut e);
        assert_eq!(query, "xabz");
        assert_eq!(&query[e.range().unwrap()], "ab");
    }

    #[test]
    fn cancelling_replaced_selection_refreshes_the_query_without_committing_preedit() {
        let mut query = String::from("selected");
        let mut e = Edit::default();
        e.sync(&query);
        e.select_all(&query);
        let mut s = State {
            pending_preedit: Some(Preedit {
                text: "仮".into(),
                begin: 3,
                end: 3,
            }),
            ..State::default()
        };
        assert!(!s.apply(&mut query, &mut e));
        assert_eq!(query, "");
        assert!(s.apply(&mut query, &mut e));
        assert!(s.preedit.is_none());
        assert!(!s.apply(&mut query, &mut e));
        assert_eq!(query, "");
    }

    #[test]
    fn surrounding_window_is_bounded_with_multibyte_text_and_selection() {
        let text = format!("#{}", "é界".repeat(2000));
        for (cursor, anchor) in [(1, 1), (5001, 5001), (10001, 10001), (4501, 5501)] {
            let (s, c, a) = surrounding(&text, cursor, anchor).unwrap();
            assert!(s.len() <= 4000);
            assert!(s.is_char_boundary(c as usize));
            assert!(s.is_char_boundary(a as usize));
            assert!(!s.starts_with('#'));
        }
        assert!(surrounding(&text, 1, 10001).is_none());
    }
    #[test]
    fn cancellation_hidden_cursor_and_invalid_offsets_are_safe() {
        let mut query = String::from("a");
        let mut e = Edit::default();
        e.sync(&query);
        let mut s = State {
            pending_preedit: Some(Preedit {
                text: "é".into(),
                begin: -1,
                end: -1,
            }),
            ..State::default()
        };
        s.apply(&mut query, &mut e);
        assert!(s.display(&query, &e).unwrap().hidden);
        s.pending_preedit = Some(Preedit {
            text: "é".into(),
            begin: 1,
            end: 99,
        });
        s.apply(&mut query, &mut e);
        let d = s.display(&query, &e).unwrap();
        assert_eq!(d.caret, 1);
        assert_eq!(d.anchor, Some(3));
        s.apply(&mut query, &mut e);
        assert!(s.preedit.is_none());
        assert_eq!(query, "a");
    }
}
