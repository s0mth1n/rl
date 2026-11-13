//! Command line: the same commands as zlaunch (show, hide, toggle, quit, reload, theme).
//! Parsing lives here so it can be tested without a daemon.

pub const HELP: &str = "A fast application launcher for Linux

Usage: zl [COMMAND]

Commands:
  show    Show the launcher window
  hide    Hide the launcher window
  toggle  Toggle the launcher window visibility
  quit    Quit the daemon
  reload  Reload the daemon (fully restart the process)
  theme   Theme management
  help    Print this message or the help of the given subcommand(s)

Options:
  -h, --help     Print help
  -V, --version  Print version
";

const MODES_HELP: &str =
    "  -m, --modes <MODES>  Modes to enable (can specify multiple with commas or repeated flags)
  -h, --help           Print help
";

const THEME_HELP: &str = "Theme management

Usage: zl theme [COMMAND]

Commands:
  list  List available themes
  set   Set the active theme
  help  Print this message or the help of the given subcommand(s)

Options:
  -h, --help  Print help
";

#[derive(Debug, PartialEq, Eq)]
pub enum Cmd {
    /// Start the daemon.
    Daemon,
    Show(Option<Vec<Mode>>),
    Hide,
    Toggle(Option<Vec<Mode>>),
    Quit,
    Reload,
    ThemeCurrent,
    ThemeList,
    ThemeSet(String),
    /// Text to print, then exit with the code.
    Print(String, i32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Combined,
    Applications,
    Emojis,
    Calculator,
    Clipboard,
    Actions,
    Search,
    Themes,
    Windows,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.to_lowercase().as_str() {
            "combined" => Self::Combined,
            "applications" | "apps" | "app" => Self::Applications,
            "emojis" | "emoji" => Self::Emojis,
            "calculator" | "calc" => Self::Calculator,
            "clipboard" => Self::Clipboard,
            "actions" | "action" => Self::Actions,
            "search" => Self::Search,
            "themes" | "theme" => Self::Themes,
            "windows" | "window" => Self::Windows,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Combined => "combined",
            Self::Applications => "applications",
            Self::Emojis => "emojis",
            Self::Calculator => "calculator",
            Self::Clipboard => "clipboard",
            Self::Actions => "actions",
            Self::Search => "search",
            Self::Themes => "themes",
            Self::Windows => "windows",
        }
    }
}

fn usage_error(msg: &str, usage: &str) -> Cmd {
    Cmd::Print(
        format!("error: {msg}\n\nUsage: {usage}\n\nFor more information, try '--help'.\n"),
        2,
    )
}

const MODE_NAMES: &str =
    "combined, applications, emojis, calculator, clipboard, actions, search, themes, windows";

fn modes(args: &[String], name: &str) -> Cmd {
    let usage = format!("zl {name} [OPTIONS]");
    let mut out: Vec<Mode> = Vec::new();
    let mut seen = false;
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if (a == "-m" || a == "--modes") && args.get(i + 1).is_some_and(|v| v.starts_with('-')) {
            return usage_error(
                "a value is required for '--modes <MODES>' but none was supplied",
                &usage,
            );
        }
        let value = if a == "-m" || a == "--modes" {
            i += 1;
            match args.get(i) {
                Some(v) => v.clone(),
                None => {
                    return usage_error(
                        "a value is required for '--modes <MODES>' but none was supplied",
                        &usage,
                    )
                }
            }
        } else if let Some(v) = a.strip_prefix("--modes=") {
            v.to_string()
        } else if let Some(v) = a.strip_prefix("-m").filter(|v| !v.is_empty()) {
            v.to_string()
        } else if a == "-h" || a == "--help" {
            let about = if name == "show" {
                "Show the launcher window"
            } else {
                "Toggle the launcher window visibility"
            };
            return Cmd::Print(
                format!("{about}\n\nUsage: {usage}\n\nOptions:\n{MODES_HELP}"),
                0,
            );
        } else {
            return usage_error(&format!("unexpected argument '{a}' found"), &usage);
        };
        seen = true;
        for part in value.split(',') {
            match Mode::parse(part) {
                Some(m) => out.push(m),
                None => {
                    return usage_error(
                        &format!(
                            "invalid value '{part}' for '--modes <MODES>'\n  [possible values: {MODE_NAMES}]"
                        ),
                        &usage,
                    )
                }
            }
        }
        i += 1;
    }
    let modes = seen.then_some(out);
    if name == "show" {
        Cmd::Show(modes)
    } else {
        Cmd::Toggle(modes)
    }
}

fn help_path(path: &[String]) -> Cmd {
    if path.is_empty() {
        return Cmd::Print(HELP.into(), 0);
    }
    let first = path[0].as_str();
    let (about, usage) = match first {
        "show" | "toggle" if path.len() == 1 => return modes(&["--help".into()], first),
        "hide" | "quit" | "reload" if path.len() == 1 => {
            return parse(&[first.into(), "--help".into()])
        }
        "theme" if path.len() == 1 => return Cmd::Print(THEME_HELP.into(), 0),
        "theme" if path.len() == 2 && path[1] == "list" => {
            ("List available themes", "zl theme list")
        }
        "theme" if path.len() == 2 && path[1] == "set" => {
            ("Set the active theme", "zl theme set <NAME>")
        }
        _ => {
            return usage_error(
                &format!("unrecognized subcommand '{}'", path.last().unwrap()),
                "zl [COMMAND]",
            )
        }
    };
    let arguments = if path.last().is_some_and(|p| p == "set") {
        "Arguments:\n  <NAME>  Name of the theme to set\n\n"
    } else {
        ""
    };
    Cmd::Print(
        format!("{about}\n\nUsage: {usage}\n\n{arguments}Options:\n  -h, --help  Print help\n"),
        0,
    )
}

pub fn parse(args: &[String]) -> Cmd {
    let Some(first) = args.first() else {
        return Cmd::Daemon;
    };
    let rest = &args[1..];
    match first.as_str() {
        "show" => modes(rest, "show"),
        "toggle" => modes(rest, "toggle"),
        "hide" | "quit" | "reload" if rest.is_empty() => match first.as_str() {
            "hide" => Cmd::Hide,
            "quit" => Cmd::Quit,
            _ => Cmd::Reload,
        },
        "hide" | "quit" | "reload" => {
            if rest[0] == "-h" || rest[0] == "--help" {
                let about = match first.as_str() {
                    "hide" => "Hide the launcher window",
                    "quit" => "Quit the daemon",
                    _ => "Reload the daemon (fully restart the process)",
                };
                Cmd::Print(
                    format!("{about}\n\nUsage: zl {first}\n\nOptions:\n  -h, --help  Print help\n"),
                    0,
                )
            } else {
                usage_error(
                    &format!("unexpected argument '{}' found", rest[0]),
                    &format!("zl {first}"),
                )
            }
        }
        "theme" if rest.first().is_some_and(|s| s == "help") => help_path(
            &std::iter::once("theme".into())
                .chain(rest[1..].iter().cloned())
                .collect::<Vec<_>>(),
        ),
        "theme"
            if rest.first().is_some_and(|s| s == "set" || s == "list")
                && rest
                    .iter()
                    .skip(1)
                    .take_while(|s| s.as_str() != "--")
                    .any(|s| s == "--help" || s == "-h") =>
        {
            help_path(&["theme".into(), rest[0].clone()])
        }
        "theme" => match rest.first().map(String::as_str) {
            None => Cmd::ThemeCurrent,
            Some("list") if rest.len() == 1 => Cmd::ThemeList,
            Some("set") if rest.get(1).is_some_and(|s| s == "--") && rest.len() == 3 => {
                Cmd::ThemeSet(rest[2].clone())
            }
            Some("set") if rest.get(1).is_some_and(|s| s.starts_with('-')) => usage_error(
                &format!("unexpected argument '{}' found", rest[1]),
                "zl theme set <NAME>",
            ),
            Some("set") => match rest.get(1) {
                Some(name) if rest.len() == 2 => Cmd::ThemeSet(name.clone()),
                Some(extra) => usage_error(
                    &format!("unexpected argument '{}' found", extra),
                    "zl theme set <NAME>",
                ),
                None => usage_error(
                    "the following required arguments were not provided:\n  <NAME>",
                    "zl theme set <NAME>",
                ),
            },
            Some("-h" | "--help" | "help") => Cmd::Print(THEME_HELP.into(), 0),
            Some(other) => usage_error(
                &format!("unrecognized subcommand '{other}'"),
                "zl theme [COMMAND]",
            ),
        },
        "help" => help_path(rest),
        "-h" | "--help" => Cmd::Print(HELP.into(), 0),
        "-V" | "--version" => Cmd::Print(format!("zl {}\n", env!("CARGO_PKG_VERSION")), 0),
        other => usage_error(
            &format!("unrecognized subcommand '{other}'"),
            "zl [COMMAND]",
        ),
    }
}

/// What the wire carries for a show or toggle: the mode names given, comma separated.
/// An empty argument means the configured default modes.
pub fn show_modes(modes: &Option<Vec<Mode>>) -> String {
    modes
        .iter()
        .flatten()
        .map(|m| m.name())
        .collect::<Vec<_>>()
        .join(",")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn p(s: &str) -> Cmd {
        parse(&s.split_whitespace().map(String::from).collect::<Vec<_>>())
    }
    #[test]
    fn commands_and_modes_parse_like_zlaunch() {
        assert_eq!(p(""), Cmd::Daemon);
        assert_eq!(p("show"), Cmd::Show(None));
        assert_eq!(p("hide"), Cmd::Hide);
        assert_eq!(p("quit"), Cmd::Quit);
        assert_eq!(p("reload"), Cmd::Reload);
        assert_eq!(
            p("show -m emoji,clipboard"),
            Cmd::Show(Some(vec![Mode::Emojis, Mode::Clipboard]))
        );
        assert_eq!(
            p("toggle --modes=apps -m theme"),
            Cmd::Toggle(Some(vec![Mode::Applications, Mode::Themes]))
        );
        assert_eq!(p("theme"), Cmd::ThemeCurrent);
        assert_eq!(p("theme list"), Cmd::ThemeList);
        assert_eq!(p("theme set nord"), Cmd::ThemeSet("nord".into()));
        assert!(matches!(p("theme set"), Cmd::Print(_, 2)));
        assert!(matches!(p("show -m bogus"), Cmd::Print(_, 2)));
        assert!(matches!(p("show -m"), Cmd::Print(_, 2)));
        assert!(matches!(p("clear-clipboard"), Cmd::Print(_, 2)));
        assert!(matches!(p("themes"), Cmd::Print(_, 2)));
        assert!(matches!(p("--help"), Cmd::Print(_, 0)));
    }
    #[test]
    fn version_flag_prints_the_crate_version() {
        for flag in ["--version", "-V"] {
            let Cmd::Print(text, 0) = p(flag) else {
                panic!("{flag}")
            };
            assert_eq!(text, format!("zl {}\n", env!("CARGO_PKG_VERSION")));
        }
    }
    #[test]
    fn nested_help_is_local_and_never_a_theme_name() {
        for (args, usage) in [
            ("help show", "Usage: zl show [OPTIONS]"),
            ("help theme set", "Usage: zl theme set <NAME>"),
            ("theme help list", "Usage: zl theme list"),
            ("theme set --help", "Usage: zl theme set <NAME>"),
            ("theme list -h", "Usage: zl theme list"),
        ] {
            let Cmd::Print(text, code) = p(args) else {
                panic!("{args}");
            };
            assert_eq!(code, 0);
            assert!(text.contains(usage), "{text}");
        }
        assert!(matches!(p("theme set --bogus"), Cmd::Print(_, 2)));
        assert!(matches!(p("help bogus"), Cmd::Print(_, 2)));
        assert_eq!(p("theme set -- --help"), Cmd::ThemeSet("--help".into()));
    }

    #[test]
    fn modes_go_over_the_wire_by_name() {
        assert_eq!(show_modes(&None), "");
        assert_eq!(show_modes(&Some(vec![Mode::Emojis])), "emojis");
        assert_eq!(
            show_modes(&Some(vec![Mode::Combined, Mode::Clipboard])),
            "combined,clipboard"
        );
        for name in ["calculator", "actions", "search", "windows", "themes"] {
            let mode = Mode::parse(name).unwrap();
            assert_eq!(mode.name(), name);
        }
    }
}
