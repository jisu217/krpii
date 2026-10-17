//! Compiled patterns with per-pattern engine configuration.
//!
//! Recognizers compile their patterns on first use, and a CLI process used as
//! an editor hook compiles them on every invocation. Measured on the built-in
//! patterns, most of that cost is spent extracting literal prefilters, which
//! only pay off for patterns anchored on rare literals such as `@` or `sk-`.
//! Building on `regex-automata` directly, the engine behind the `regex` crate,
//! lets each recognizer decide whether its prefilter is worth compiling.
//! Matching semantics are unchanged: leftmost-first, linear time.

use std::ops::Range;

use regex_automata::util::captures::Captures as Slots;
use regex_automata::{Input, meta};

pub(crate) use regex_automata::meta::BuildError;

/// Whether a pattern compiles a literal prefilter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Prefilter {
    /// Compile it: the pattern is anchored on a rare literal, and the prefilter
    /// skips most of the input during search.
    Enabled,
    /// Skip it: the pattern starts with a character class such as `[0-9]`, so
    /// the prefilter saves little search time but costs most of the compile time.
    Disabled,
}

/// A compiled pattern.
#[derive(Debug)]
pub(crate) struct Pattern {
    regex: meta::Regex,
}

impl Pattern {
    pub(crate) fn new(source: &str, prefilter: Prefilter) -> Result<Self, Box<BuildError>> {
        let config = meta::Config::new().auto_prefilter(prefilter == Prefilter::Enabled);
        let regex = meta::Builder::new()
            .configure(config)
            .build(source)
            .map_err(Box::new)?;
        Ok(Self { regex })
    }

    /// Returns the leftmost-first match starting at or after byte `start`.
    pub(crate) fn captures_at<'t>(&self, text: &'t str, start: usize) -> Option<Captures<'t>> {
        let mut slots = self.regex.create_captures();
        let input = Input::new(text).span(start..text.len());
        self.regex.search_captures(&input, &mut slots);
        slots.is_match().then_some(Captures { text, slots })
    }

    /// Returns the leftmost-first match in `text`.
    pub(crate) fn captures<'t>(&self, text: &'t str) -> Option<Captures<'t>> {
        self.captures_at(text, 0)
    }
}

/// The groups of one match, borrowing the searched text.
#[derive(Debug)]
pub(crate) struct Captures<'t> {
    text: &'t str,
    slots: Slots,
}

impl<'t> Captures<'t> {
    /// Returns group `index`, where group 0 is the whole match.
    pub(crate) fn get(&self, index: usize) -> Option<Group<'t>> {
        self.slots
            .get_group(index)
            .map(|span| Group::new(self.text, span.range()))
    }

    /// Returns the named group, or `None` if it did not participate.
    pub(crate) fn name(&self, name: &str) -> Option<Group<'t>> {
        self.slots
            .get_group_by_name(name)
            .map(|span| Group::new(self.text, span.range()))
    }
}

/// One matched group: a byte range of the searched text.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Group<'t> {
    text: &'t str,
    start: usize,
    end: usize,
}

impl<'t> Group<'t> {
    fn new(text: &'t str, range: Range<usize>) -> Self {
        Self {
            text,
            start: range.start,
            end: range.end,
        }
    }

    pub(crate) fn start(&self) -> usize {
        self.start
    }

    pub(crate) fn end(&self) -> usize {
        self.end
    }

    pub(crate) fn range(&self) -> Range<usize> {
        self.start..self.end
    }

    pub(crate) fn len(&self) -> usize {
        self.end - self.start
    }

    pub(crate) fn as_str(&self) -> &'t str {
        &self.text[self.start..self.end]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_named_groups_from_a_start_offset() {
        let pattern = Pattern::new(
            r"(?P<area>0[0-9]{2})-(?P<rest>[0-9]{4})",
            Prefilter::Disabled,
        )
        .expect("test pattern is valid");
        let text = "010-1234 / 031-5678";
        let second = pattern.captures_at(text, 5).expect("second match");
        assert_eq!(second.get(0).map(|g| g.as_str()), Some("031-5678"));
        assert_eq!(second.name("area").map(|g| g.range()), Some(11..14));
        assert!(second.name("missing").is_none());
    }

    #[test]
    fn prefilter_setting_does_not_change_matches() {
        let text = "mail kim@example.com and lee@example.org";
        for prefilter in [Prefilter::Enabled, Prefilter::Disabled] {
            let pattern = Pattern::new(r"[a-z]+@[a-z]+\.[a-z]+", prefilter).expect("valid");
            let found = pattern
                .captures(text)
                .and_then(|c| c.get(0))
                .map(|g| g.as_str());
            assert_eq!(found, Some("kim@example.com"));
        }
    }
}
