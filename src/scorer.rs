//! Fuzzy ranking for names and descriptions, with configurable match bonuses.
use crate::config::FuzzyConfig;
use fuzzy_matcher::{skim::SkimMatcherV2, FuzzyMatcher};

#[derive(Clone, Copy)]
pub enum Kind {
    Regular,
    #[allow(dead_code)]
    Action,
    Submenu,
}

pub struct Scorer<'a> {
    matcher: SkimMatcherV2,
    config: &'a FuzzyConfig,
}

impl<'a> Scorer<'a> {
    pub fn new(config: &'a FuzzyConfig) -> Self {
        Self {
            matcher: SkimMatcherV2::default(),
            config,
        }
    }

    pub fn score(&self, name: &str, description: &str, query: &str, kind: Kind) -> Option<i64> {
        if query.is_empty() {
            return Some(0);
        }
        let cfg = self.config;
        let query_lower = query.to_lowercase();
        for (text, description) in [(name, false), (description, true)] {
            let matched = self.matcher.fuzzy_indices(text, query).or_else(|| {
                if !query.contains(' ') {
                    return None;
                }
                self.matcher
                    .fuzzy_indices(text, &query.replace(' ', ""))
                    .or_else(|| self.matcher.fuzzy_indices(text, &query.replace(' ', "-")))
            });
            let Some((mut score, positions)) = matched else {
                continue;
            };
            if !description {
                let lower = text.to_lowercase();
                score += if lower == query_lower {
                    cfg.exact_match_bonus
                } else if lower.starts_with(&query_lower) {
                    cfg.prefix_match_bonus
                } else if lower
                    .split_whitespace()
                    .any(|word| word.starts_with(&query_lower))
                {
                    cfg.word_prefix_bonus
                } else {
                    0
                };
            }
            let gaps = positions.len().saturating_sub(1);
            let joined = positions
                .windows(2)
                .filter(|pair| pair[1] - pair[0] == 1)
                .count();
            score += if gaps == 0 {
                cfg.contiguity_bonus
            } else {
                (joined as f64 / gaps as f64 * cfg.contiguity_bonus as f64) as i64
            };
            if description {
                score = (score as f64 * cfg.description_penalty) as i64;
            }
            let weight = match kind {
                Kind::Regular => 1.0,
                Kind::Action => cfg.action_score_multiplier,
                Kind::Submenu => cfg.submenu_score_multiplier,
            };
            return Some((score as f64 * weight) as i64);
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base(text: &str, query: &str) -> (i64, Vec<usize>) {
        SkimMatcherV2::default().fuzzy_indices(text, query).unwrap()
    }
    #[test]
    fn exact_prefix_word_and_contiguity_bonuses() {
        let cfg = FuzzyConfig::default();
        let scorer = Scorer::new(&cfg);
        for (text, query, bonus) in [
            ("Firefox", "firefox", 100_000),
            ("Firefox", "fire", 50_000),
            ("Web Firefox", "fire", 25_000),
        ] {
            let raw = base(text, query).0;
            assert_eq!(
                scorer.score(text, "", query, Kind::Regular),
                Some(raw + bonus + 10_000)
            );
        }
        let (raw, indices) = base("FooInRExact", "fire");
        let adjacent = indices.windows(2).filter(|w| w[1] == w[0] + 1).count();
        assert_eq!(
            scorer.score("FooInRExact", "", "fire", Kind::Regular),
            Some(raw + (adjacent as f64 / (indices.len() - 1) as f64 * 10_000.0) as i64)
        );
    }
    #[test]
    fn description_penalty_precedes_type_multiplier() {
        let cfg = FuzzyConfig::default();
        let scorer = Scorer::new(&cfg);
        let raw = base("Web Browser", "browser").0 + 10_000;
        let description = (raw as f64 * 0.3) as i64;
        assert_eq!(
            scorer.score("App", "Web Browser", "browser", Kind::Regular),
            Some(description)
        );
        assert_eq!(
            scorer.score("App", "Web Browser", "browser", Kind::Action),
            Some((description as f64 * 0.8) as i64)
        );
        assert_eq!(
            scorer.score("App", "Web Browser", "browser", Kind::Submenu),
            Some((description as f64 * 0.9) as i64)
        );
        let name = scorer
            .score("Browser", "", "browser", Kind::Regular)
            .unwrap();
        assert_eq!(
            scorer.score("Browser", "Browser", "browser", Kind::Submenu),
            Some((name as f64 * 0.9) as i64)
        );
    }
    #[test]
    fn normalized_queries_use_original_query_for_bonuses() {
        let cfg = FuzzyConfig::default();
        let scorer = Scorer::new(&cfg);
        let (raw, indices) = base("Counter-Strike", "counterstrike");
        let adjacent = indices.windows(2).filter(|w| w[1] == w[0] + 1).count();
        let expected = raw + (adjacent as f64 / (indices.len() - 1) as f64 * 10_000.0) as i64;
        assert_eq!(
            scorer.score("Counter-Strike", "", "counter strike", Kind::Regular),
            Some(expected)
        );
        assert_eq!(
            scorer.score("什么应用", "", "什么", Kind::Regular),
            Some(base("什么应用", "什么").0 + 60_000)
        );
        assert_eq!(scorer.score("App", "", "no-match", Kind::Regular), None);
        assert_eq!(scorer.score("App", "", "", Kind::Regular), Some(0));
    }
    #[test]
    fn configured_scores_are_not_replaced_by_defaults() {
        let cfg: FuzzyConfig = toml::from_str("exact_match_bonus = 7\nprefix_match_bonus = 5\nword_prefix_bonus = 3\ncontiguity_bonus = 1\ndescription_penalty = 0.5\naction_score_multiplier = 0.2\nsubmenu_score_multiplier = 0.4\nshow_best_match = false").unwrap();
        let scorer = Scorer::new(&cfg);
        assert_eq!(
            scorer.score("App", "", "App", Kind::Regular),
            Some(base("App", "App").0 + 8)
        );
        assert!(!cfg.show_best_match);
        assert_eq!(
            scorer.score("X", "App", "App", Kind::Action),
            Some((((base("App", "App").0 + 1) as f64 * 0.5) as i64 as f64 * 0.2) as i64)
        );
    }
}
