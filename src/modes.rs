//! Active launcher modes, in the order selected by default_modes.
use crate::config::ConfigModule;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Combined,
    Module(ConfigModule),
}

impl Mode {
    pub fn parse(value: &str) -> Option<Self> {
        use ConfigModule::*;
        Some(match value.to_lowercase().as_str() {
            "combined" => Self::Combined,
            "apps" | "app" | "applications" => Self::Module(Applications),
            "emoji" | "emojis" => Self::Module(Emojis),
            "clipboard" => Self::Module(Clipboard),
            "theme" | "themes" => Self::Module(Themes),
            "window" | "windows" => Self::Module(Windows),
            "action" | "actions" => Self::Module(Actions),
            "calc" | "calculator" => Self::Module(Calculator),
            "search" => Self::Module(Search),
            _ => return None,
        })
    }

    pub fn routes_prefixes(self) -> bool {
        matches!(
            self,
            Self::Combined
                | Self::Module(
                    ConfigModule::Emojis | ConfigModule::Clipboard | ConfigModule::Themes
                )
        )
    }

    pub fn prefix(self) -> &'static str {
        match self {
            Self::Module(ConfigModule::Emojis) => ":",
            Self::Module(ConfigModule::Clipboard) => ";",
            Self::Module(ConfigModule::Themes) => "#",
            _ => "",
        }
    }

    pub fn placeholder(self) -> &'static str {
        use ConfigModule::*;
        match self {
            Self::Combined => "Search anything...",
            Self::Module(Applications) => "Search applications...",
            Self::Module(Windows) => "Search windows...",
            Self::Module(Actions) => "Search actions...",
            Self::Module(Emojis) => "Search emojis...",
            Self::Module(Clipboard) => "Search clipboard...",
            Self::Module(Themes) => "Search themes...",
            Self::Module(Search) => "Search the web...",
            Self::Module(Calculator) => "Calculate...",
        }
    }
}

pub struct State {
    modes: Vec<Mode>,
    index: usize,
}

impl State {
    pub fn new(names: Option<&[String]>) -> Self {
        let mut modes: Vec<_> = names
            .unwrap_or_default()
            .iter()
            .filter_map(|s| Mode::parse(s))
            .collect();
        if modes.is_empty() {
            modes.push(Mode::Combined);
        }
        Self { modes, index: 0 }
    }

    pub fn current(&self) -> Mode {
        self.modes[self.index]
    }

    pub fn cycle(&mut self, previous: bool) -> bool {
        if self.modes.len() <= 1 {
            return false;
        }
        self.index =
            (self.index + if previous { self.modes.len() - 1 } else { 1 }) % self.modes.len();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configured_modes_keep_order_aliases_and_duplicates() {
        let names = ["emoji", "invalid", "APPS", "emoji"].map(str::to_string);
        let mut s = State::new(Some(&names));
        assert_eq!(s.current().prefix(), ":");
        assert!(s.cycle(false));
        assert_eq!(s.current(), Mode::Module(ConfigModule::Applications));
        s.cycle(true);
        assert_eq!(s.current().prefix(), ":");
        s.cycle(true);
        assert_eq!(s.current().prefix(), ":");
        s.cycle(true);
        assert_eq!(s.current(), Mode::Module(ConfigModule::Applications));
    }
    #[test]
    fn empty_invalid_and_single_mode_lists_do_not_cycle() {
        for names in [None, Some(vec![]), Some(vec!["unknown".into()])] {
            let mut s = State::new(names.as_deref());
            assert_eq!(s.current(), Mode::Combined);
            assert!(!s.cycle(false));
            assert!(!s.cycle(true));
        }
        let mut s = State::new(Some(&["clipboard".into()]));
        assert_eq!(s.current().prefix(), ";");
        assert!(!s.cycle(false));
    }
}
