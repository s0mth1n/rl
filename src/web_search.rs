//! Web provider triggers and URLs, following zlaunch's search module.
use crate::config::ConfigSearchProvider;

#[derive(Debug, PartialEq)]
pub enum Detection<'a> {
    None,
    Triggered(&'a ConfigSearchProvider, &'a str),
    Fallback(&'a str),
}

pub fn detect<'a>(input: &'a str, providers: &'a [ConfigSearchProvider]) -> Detection<'a> {
    let input = input.trim();
    if input.is_empty() {
        return Detection::None;
    }
    for provider in providers {
        if let Some(query) = input.strip_prefix(&provider.trigger) {
            return if query.trim().is_empty() {
                Detection::None
            } else {
                Detection::Triggered(provider, query.trim())
            };
        }
    }
    Detection::Fallback(input)
}

impl ConfigSearchProvider {
    pub fn build_url(&self, query: &str) -> String {
        use std::fmt::Write;
        let mut encoded = String::new();
        for byte in query.bytes() {
            if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
                encoded.push(char::from(byte));
            } else {
                write!(encoded, "%{byte:02X}").unwrap();
            }
        }
        self.url.replace("{query}", &encoded)
    }

    pub fn icon_name(&self) -> String {
        let name = self.icon.trim().to_ascii_lowercase();
        if crate::icons::phosphor(&name).is_some() {
            format!("phosphor:{name}")
        } else {
            if !name.is_empty() {
                eprintln!(
                    "zl: unknown search provider icon '{}', using magnifying-glass",
                    self.icon
                );
            }
            "phosphor:magnifying-glass".into()
        }
    }
}

pub fn open(url: &str) -> std::io::Result<std::process::Child> {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    let mut command = Command::new("xdg-open");
    command
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        command.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    command.spawn()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn triggers_are_ordered_case_sensitive_prefixes_without_word_boundaries() {
        let providers = crate::config::Config::default().search_providers.unwrap();
        assert_eq!(
            detect("  !grust  ", &providers),
            Detection::Triggered(&providers[0], "rust")
        );
        assert_eq!(detect(" !g   ", &providers), Detection::None);
        assert_eq!(
            detect(" !G rust ", &providers),
            Detection::Fallback("!G rust")
        );
        assert_eq!(
            detect(" !unknown rust ", &providers),
            Detection::Fallback("!unknown rust")
        );
        assert_eq!(detect("  ", &providers), Detection::None);
        assert_eq!(
            detect(" rust  async ", &providers),
            Detection::Fallback("rust  async")
        );
        let mut overlapping = providers[..1].to_vec();
        let mut second = providers[0].clone();
        second.trigger = "!google".into();
        overlapping.push(second);
        assert_eq!(
            detect("!google rust", &overlapping),
            Detection::Triggered(&overlapping[0], "oogle rust")
        );
    }

    #[test]
    fn urls_encode_utf8_reserved_bytes_and_replace_every_placeholder() {
        let provider = ConfigSearchProvider {
            name: "Custom".into(),
            trigger: "!c".into(),
            url: "https://example.invalid/{query}?again={query}".into(),
            icon: " BOOK-OPEN ".into(),
        };
        assert_eq!(provider.build_url("a b+c/&?=é🙂-_.~"), "https://example.invalid/a%20b%2Bc%2F%26%3F%3D%C3%A9%F0%9F%99%82-_.~?again=a%20b%2Bc%2F%26%3F%3D%C3%A9%F0%9F%99%82-_.~");
        assert_eq!(provider.icon_name(), "phosphor:book-open");
        let mut provider = provider;
        provider.icon = "unknown".into();
        provider.url = "https://example.invalid/static".into();
        assert_eq!(provider.icon_name(), "phosphor:magnifying-glass");
        assert_eq!(
            provider.build_url("not inserted"),
            "https://example.invalid/static"
        );
    }
}
