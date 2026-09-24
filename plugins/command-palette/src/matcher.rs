//! Fuzzy ranking: nucleo text score plus a capped frecency boost.

use nucleo_matcher::pattern::{CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use unicode_segmentation::UnicodeSegmentation;

use crate::item::Item;

/// Keep the maximum boost below typical gaps between a strong and a weak
/// nucleo match (about 20 points for 5-letter queries), so text relevance
/// wins and frecency mostly breaks ties.
pub const FRECENCY_CAP: f64 = 15.0;
pub const FRECENCY_WEIGHT: f64 = 1.0;

#[derive(Debug, Clone, PartialEq)]
pub struct Ranked {
    pub index: usize,
    pub score: f64,
    /// Grapheme-cluster indices into the item's title to highlight. nucleo
    /// matches over `Utf32Str`, which collapses each grapheme cluster (e.g. a
    /// ZWJ emoji sequence or a base character plus combining marks) to one
    /// unit, so match positions are grapheme indices, not char indices.
    pub highlights: Vec<usize>,
}

pub fn rank(query: &str, items: &[Item], frecency: impl Fn(&str) -> f64) -> Vec<Ranked> {
    let mut ranked: Vec<Ranked> = if query.trim().is_empty() {
        items
            .iter()
            .enumerate()
            .filter(|(_, item)| !item.current)
            .map(|(index, item)| Ranked { index, score: frecency(&item.id), highlights: Vec::new() })
            .collect()
    } else {
        let pattern = Pattern::parse(query, CaseMatching::Smart, Normalization::Smart);
        let mut matcher = Matcher::new(Config::DEFAULT);
        let mut buf = Vec::new();
        let mut indices = Vec::new();
        items
            .iter()
            .enumerate()
            .filter_map(|(index, item)| {
                indices.clear();
                let haystack = item.haystack();
                let score = pattern.indices(Utf32Str::new(&haystack, &mut buf), &mut matcher, &mut indices)?;
                indices.sort_unstable();
                indices.dedup();
                let title_len = item.title.graphemes(true).count();
                let highlights = indices.iter().map(|&i| i as usize).filter(|&i| i < title_len).collect();
                let boost = frecency(&item.id).min(FRECENCY_CAP) * FRECENCY_WEIGHT;
                Some(Ranked { index, score: f64::from(score) + boost, highlights })
            })
            .collect()
    };
    ranked.sort_by(|a, b| {
        let (ia, ib) = (&items[a.index], &items[b.index]);
        b.score.total_cmp(&a.score).then_with(|| ia.kind.cmp(&ib.kind)).then_with(|| ia.title.cmp(&ib.title))
    });
    ranked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::item::{Action, Item, Kind};

    fn item(kind: Kind, id: &str, title: &str) -> Item {
        Item::new(kind, id, title, Action::FocusWorkspace(id.into()))
    }

    fn titles<'a>(items: &'a [Item], ranked: &[Ranked]) -> Vec<&'a str> {
        ranked.iter().map(|r| items[r.index].title.as_str()).collect()
    }

    fn no_frecency(_: &str) -> f64 {
        0.0
    }

    #[test]
    fn empty_query_hides_current_and_sorts_by_frecency_then_kind_then_title() {
        let items = vec![
            item(Kind::Command, "cmd:b", "B command"),
            item(Kind::Workspace, "ws:cur", "Current").current(true),
            item(Kind::Tab, "tab:z", "Z tab"),
            item(Kind::Workspace, "ws:a", "A workspace"),
            item(Kind::Command, "cmd:a", "A command"),
        ];
        let ranked = rank("", &items, |id| if id == "cmd:b" { 5.0 } else { 0.0 });
        assert_eq!(titles(&items, &ranked), ["B command", "A workspace", "Z tab", "A command"]);
        assert!(ranked.iter().all(|r| r.highlights.is_empty()));
    }

    #[test]
    fn whitespace_query_counts_as_empty() {
        let items = vec![item(Kind::Workspace, "ws:cur", "Current").current(true)];
        assert!(rank("   ", &items, no_frecency).is_empty());
    }

    #[test]
    fn query_filters_out_non_matches_and_includes_current() {
        let items = vec![
            item(Kind::Workspace, "ws:1", "herdr-plugins").current(true),
            item(Kind::Workspace, "ws:2", "boardwalk"),
        ];
        assert_eq!(titles(&items, &rank("herdr", &items, no_frecency)), ["herdr-plugins"]);
    }

    #[test]
    fn strong_text_match_beats_frecent_weak_match() {
        let items = vec![
            item(Kind::Command, "cmd:swap", "Swap pane left in tab"),
            item(Kind::Command, "cmd:split", "Split pane right"),
        ];
        let ranked = rank("split", &items, |id| if id == "cmd:swap" { 1000.0 } else { 0.0 });
        assert_eq!(titles(&items, &ranked)[0], "Split pane right");
    }

    #[test]
    fn frecency_breaks_equal_text_matches() {
        let items = vec![item(Kind::Tab, "tab:ct", "CT › Claude"), item(Kind::Tab, "tab:jacob", "jacob › Claude")];
        let ranked = rank("claude", &items, |id| if id == "tab:jacob" { 3.0 } else { 0.0 });
        assert_eq!(titles(&items, &ranked), ["jacob › Claude", "CT › Claude"]);
    }

    #[test]
    fn highlights_are_char_indices_into_the_title() {
        let items = vec![item(Kind::Tab, "tab:ct", "CT › Claude")];
        let ranked = rank("cl", &items, no_frecency);
        assert_eq!(ranked[0].highlights, [5, 6]);
    }

    #[test]
    fn keyword_only_match_has_no_title_highlights() {
        let items = vec![
            Item::new(Kind::Command, "cmd:close-tab", "Close tab", Action::FocusTab("x".into()))
                .keywords(vec!["kill".into()]),
        ];
        let ranked = rank("kill", &items, no_frecency);
        assert_eq!(ranked.len(), 1);
        assert!(ranked[0].highlights.is_empty());
    }

    #[test]
    fn operator_only_queries_do_not_panic() {
        let items = vec![item(Kind::Workspace, "ws:1", "CT"), item(Kind::Tab, "tab:1", "CT › Claude")];
        for query in ["!", "^", "'", "$", "!!", "^$", "' '"] {
            let ranked = rank(query, &items, no_frecency);
            assert!(ranked.len() <= items.len(), "{query}");
        }
    }

    #[test]
    fn highlights_are_grapheme_indices_not_char_indices() {
        // graphemes: ["👨‍💻", " ", "d", "e", "v"] — "👨‍💻" alone is 3 chars.
        let items = vec![item(Kind::Command, "cmd:dev", "👨‍💻 dev")];
        let ranked = rank("d", &items, no_frecency);
        assert_eq!(ranked[0].highlights, [2]);
    }

    #[test]
    fn title_bound_uses_grapheme_count_not_char_count() {
        // The title is a single grapheme spanning 3 chars; a match in the
        // subtitle at collapsed (grapheme) index 2 must not be misread as a
        // title highlight just because 2 < title.chars().count().
        let items = vec![Item::new(Kind::Command, "cmd:x", "👨‍💻", Action::FocusWorkspace("x".into())).subtitle("cat")];
        let ranked = rank("c", &items, no_frecency);
        assert!(ranked[0].highlights.is_empty(), "{:?}", ranked[0].highlights);
    }
}
