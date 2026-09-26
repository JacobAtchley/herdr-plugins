//! Fuzzy filtering over script names and commands.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use unicode_segmentation::UnicodeSegmentation;

use crate::project::Script;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ranked {
    pub index: usize,
    /// Grapheme indices into the script name to highlight.
    pub highlights: Vec<usize>,
}

/// Owns the nucleo matcher and scratch buffers so each keystroke reuses them
/// instead of reallocating.
pub struct ScriptMatcher {
    matcher: Matcher,
    buf: Vec<char>,
    indices: Vec<u32>,
    haystack: String,
}

impl Default for ScriptMatcher {
    fn default() -> Self {
        Self { matcher: Matcher::new(Config::DEFAULT), buf: Vec::new(), indices: Vec::new(), haystack: String::new() }
    }
}

impl ScriptMatcher {
    /// An empty query keeps package.json order. Otherwise the best matches
    /// come first and ties keep file order. Commands are searched too, so
    /// `serve api` finds `"start:api": "nx serve api"`.
    pub fn rank(&mut self, query: &str, scripts: &[Script]) -> Vec<Ranked> {
        if query.trim().is_empty() {
            return (0..scripts.len()).map(|index| Ranked { index, highlights: Vec::new() }).collect();
        }
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut scored: Vec<(u32, Ranked)> = scripts
            .iter()
            .enumerate()
            .filter_map(|(index, script)| {
                self.haystack.clear();
                self.haystack.push_str(&script.name);
                self.haystack.push(' ');
                self.haystack.push_str(&script.command);
                self.indices.clear();
                let score = pattern.indices(
                    Utf32Str::new(&self.haystack, &mut self.buf),
                    &mut self.matcher,
                    &mut self.indices,
                )?;
                let name_len = script.name.graphemes(true).count();
                self.indices.sort_unstable();
                self.indices.dedup();
                let highlights = self.indices.iter().map(|&i| i as usize).take_while(|&i| i < name_len).collect();
                Some((score, Ranked { index, highlights }))
            })
            .collect();
        scored.sort_by(|(a, ra), (b, rb)| b.cmp(a).then(ra.index.cmp(&rb.index)));
        scored.into_iter().map(|(_, ranked)| ranked).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scripts(pairs: &[(&str, &str)]) -> Vec<Script> {
        pairs.iter().map(|(n, c)| Script { name: n.to_string(), command: c.to_string() }).collect()
    }

    fn order(ranked: &[Ranked]) -> Vec<usize> {
        ranked.iter().map(|r| r.index).collect()
    }

    #[test]
    fn empty_query_keeps_file_order() {
        let list = scripts(&[("b", "x"), ("a", "y")]);
        assert_eq!(order(&ScriptMatcher::default().rank("  ", &list)), [0, 1]);
    }

    #[test]
    fn filters_and_ranks_best_first() {
        let list = scripts(&[("lint", "nx lint"), ("start:web", "nx serve web"), ("start:api", "nx serve api")]);
        let mut matcher = ScriptMatcher::default();
        assert_eq!(order(&matcher.rank("api", &list)), [2]);
        assert_eq!(order(&matcher.rank("start", &list)), [1, 2]);
    }

    #[test]
    fn command_matches_count_but_do_not_highlight_the_name() {
        let list = scripts(&[("dev", "nx serve web")]);
        let ranked = ScriptMatcher::default().rank("serve", &list);
        assert_eq!(order(&ranked), [0]);
        assert!(ranked[0].highlights.is_empty());
    }

    #[test]
    fn name_matches_are_highlighted() {
        let list = scripts(&[("start:api", "x")]);
        assert_eq!(ScriptMatcher::default().rank("api", &list)[0].highlights, [6, 7, 8]);
    }
}
