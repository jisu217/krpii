//! Built-in recognizers, one module per entity type.
//!
//! Each recognizer pairs a permissive regular expression (cheap candidate
//! generation) with a strict `score` function (validation). Keeping the two
//! apart is what holds the false-positive rate down.
//!
//! The `regex` crate guarantees linear-time matching and therefore offers no
//! look-around or back-references. Digit boundaries and separator consistency
//! are consequently enforced in code, and [`Recognizer::find`] retries from the
//! next character whenever a candidate is rejected so that no valid match
//! overlapping a rejected one is skipped.

mod brn;
mod card;
mod email;
mod phone;
mod rrn;

use regex::{Captures, Regex};

use crate::entity::{Entity, EntityType};

/// The character substituted for each obscured digit or letter.
pub(crate) const MASK_CHAR: char = '*';

/// Locates and validates one category of PII.
pub(crate) trait Recognizer: Sync {
    /// The entity type this recognizer produces.
    fn entity_type(&self) -> EntityType;

    /// The candidate-generating pattern.
    fn pattern(&self) -> &Regex;

    /// Returns a confidence score for a candidate, or `None` to reject it.
    ///
    /// `text` is the full input, so implementations can inspect the
    /// characters surrounding the match.
    fn score(&self, text: &str, captures: &Captures<'_>) -> Option<f64>;

    /// Returns `value` with its identifying portion obscured.
    ///
    /// The result must keep the original character count and separators so
    /// that the masked text remains readable.
    fn mask_partial(&self, value: &str) -> String;

    /// Returns every validated entity of this recognizer's type in `text`.
    fn find(&self, text: &str) -> Vec<Entity> {
        let pattern = self.pattern();
        let mut entities = Vec::new();
        let mut position = 0;
        while let Some(captures) = pattern.captures_at(text, position) {
            let matched = captures.get(0).expect("group 0 always participates");
            if let Some(score) = self.score(text, &captures) {
                entities.push(Entity {
                    entity_type: self.entity_type(),
                    start: matched.start(),
                    end: matched.end(),
                    score,
                });
                position = matched.end();
            } else {
                position = next_char_boundary(text, matched.start());
            }
        }
        entities
    }
}

/// Returns the recognizer responsible for `entity_type`.
pub(crate) fn recognizer_for(entity_type: EntityType) -> &'static dyn Recognizer {
    match entity_type {
        EntityType::Phone => &phone::PhoneRecognizer,
        EntityType::Email => &email::EmailRecognizer,
        EntityType::Card => &card::CardRecognizer,
        EntityType::Rrn => &rrn::ResidentRegistrationNumberRecognizer,
        EntityType::Brn => &brn::BusinessRegistrationNumberRecognizer,
    }
}

/// Returns whether the span `start..end` is not glued to adjacent digits.
///
/// Korean text attaches particles directly to tokens ("010-1234-5678로"), so
/// word boundaries are unreliable. A numeric entity is instead considered
/// isolated when it is neither preceded by a digit or hyphen nor followed by a
/// digit, optionally after a single hyphen.
pub(crate) fn has_digit_boundaries(text: &str, start: usize, end: usize) -> bool {
    let bytes = text.as_bytes();
    let preceded = start
        .checked_sub(1)
        .is_some_and(|i| bytes[i].is_ascii_digit() || bytes[i] == b'-');
    let followed = match bytes.get(end) {
        Some(b) if b.is_ascii_digit() => true,
        Some(b'-') => bytes.get(end + 1).is_some_and(u8::is_ascii_digit),
        _ => false,
    };
    !preceded && !followed
}

/// Returns the text of an optional capture group, or `""` if it did not participate.
pub(crate) fn group<'t>(captures: &Captures<'t>, name: &str) -> &'t str {
    captures.name(name).map_or("", |m| m.as_str())
}

/// Masks every ASCII digit in `value` except the first `keep_head` and last `keep_tail`.
///
/// Separators are preserved, so `mask_digits("1234-5678", 2, 0)` yields `"12**-****"`.
pub(crate) fn mask_digits(value: &str, keep_head: usize, keep_tail: usize) -> String {
    let digit_count = value.chars().filter(char::is_ascii_digit).count();
    let mut seen = 0;
    value
        .chars()
        .map(|c| {
            if !c.is_ascii_digit() {
                return c;
            }
            let keep = seen < keep_head || seen + keep_tail >= digit_count;
            seen += 1;
            if keep { c } else { MASK_CHAR }
        })
        .collect()
}

/// Returns only the ASCII digits of `value`.
pub(crate) fn only_digits(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).collect()
}

fn next_char_boundary(text: &str, index: usize) -> usize {
    index + text[index..].chars().next().map_or(1, char::len_utf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_digits_preserves_separators() {
        assert_eq!(mask_digits("1234-5678", 2, 0), "12**-****");
        assert_eq!(mask_digits("1234-5678", 0, 4), "****-5678");
        assert_eq!(mask_digits("1234-5678", 4, 4), "1234-5678");
    }

    #[test]
    fn digit_boundaries_ignore_hangul_particles() {
        let text = "번호010-1234-5678로";
        let start = text.find('0').unwrap();
        assert!(has_digit_boundaries(text, start, start + 13));
    }

    #[test]
    fn digit_boundaries_reject_adjacent_digits() {
        assert!(!has_digit_boundaries("91234", 1, 5));
        assert!(!has_digit_boundaries("1234-5", 0, 4));
        assert!(has_digit_boundaries("1234-", 0, 4));
    }
}
