//! Combined-list scoring ported from zlaunch ee71f96 (MIT, see NOTICE).
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
        self.score_text_match(name, query, kind, false)
            .or_else(|| self.score_text_match(description, query, kind, true))
    }
    fn score_text_match(
        &self,
        text: &str,
        query: &str,
        kind: Kind,
        is_description: bool,
    ) -> Option<i64> {
        let query_lower = query.to_lowercase();
        let text_lower = text.to_lowercase();

        let match_result = self.matcher.fuzzy_indices(text, query);

        let match_result = match_result.or_else(|| {
            if query.contains(' ') {
                let no_spaces: String = query.chars().filter(|c| *c != ' ').collect();
                if let Some(result) = self.matcher.fuzzy_indices(text, &no_spaces) {
                    return Some(result);
                }

                let with_hyphens = query.replace(' ', "-");
                if let Some(result) = self.matcher.fuzzy_indices(text, &with_hyphens) {
                    return Some(result);
                }
            }
            None
        });

        let (base_score, indices) = match_result?;
        let mut score = base_score;

        if !is_description {
            if text_lower == query_lower {
                score += self.config.exact_match_bonus;
            } else if text_lower.starts_with(&query_lower) {
                score += self.config.prefix_match_bonus;
            } else if self.matches_word_start(text, query) {
                score += self.config.word_prefix_bonus;
            }
        }

        score += self.calculate_contiguity_bonus(&indices);

        if is_description {
            score = (score as f64 * self.config.description_penalty) as i64;
        }

        score = self.apply_item_multiplier(score, kind);

        Some(score)
    }

    fn calculate_contiguity_bonus(&self, indices: &[usize]) -> i64 {
        if indices.len() <= 1 {
            return self.config.contiguity_bonus;
        }

        let mut adjacent_count = 0;
        for window in indices.windows(2) {
            if window[1] == window[0] + 1 {
                adjacent_count += 1;
            }
        }

        let contiguity_ratio = adjacent_count as f64 / (indices.len() - 1) as f64;
        (contiguity_ratio * self.config.contiguity_bonus as f64) as i64
    }

    fn matches_word_start(&self, text: &str, query: &str) -> bool {
        let query_lower = query.to_lowercase();
        text.split_whitespace()
            .any(|word| word.to_lowercase().starts_with(&query_lower))
    }

    fn apply_item_multiplier(&self, score: i64, kind: Kind) -> i64 {
        let multiplier = match kind {
            Kind::Action => self.config.action_score_multiplier,
            Kind::Submenu => self.config.submenu_score_multiplier,
            _ => 1.0,
        };
        (score as f64 * multiplier) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base(text: &str, query: &str) -> (i64, Vec<usize>) {
        SkimMatcherV2::default().fuzzy_indices(text, query).unwrap()
    }
    #[test]
    fn exact_prefix_word_and_contiguity_bonuses_match_reference() {
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
    fn description_penalty_and_type_multipliers_apply_in_reference_order() {
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
