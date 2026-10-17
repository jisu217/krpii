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
mod secret;

use std::ops::Range;

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

    /// Returns the byte range to report for a validated candidate.
    ///
    /// Defaults to the whole match. A recognizer overrides it when only part
    /// of the match is sensitive, such as the password inside a connection
    /// string, so masking leaves the surrounding context intact.
    fn span(&self, captures: &Captures<'_>) -> Range<usize> {
        captures
            .get(0)
            .expect("group 0 always participates")
            .range()
    }

    /// Returns every validated entity of this recognizer's type in `text`.
    fn find(&self, text: &str) -> Vec<Entity> {
        let pattern = self.pattern();
        let mut entities = Vec::new();
        let mut position = 0;
        while let Some(captures) = pattern.captures_at(text, position) {
            let matched = captures.get(0).expect("group 0 always participates");
            if let Some(score) = self.score(text, &captures) {
                let span = self.span(&captures);
                entities.push(Entity {
                    entity_type: self.entity_type(),
                    start: span.start,
                    end: span.end,
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
        EntityType::Secret => &secret::SecretRecognizer,
    }
}

/// Returns whether the span `start..end` is not glued to adjacent identifier
/// characters.
///
/// Korean text attaches particles directly to tokens ("010-1234-5678로"), so
/// Unicode word boundaries are unreliable. A numeric entity is instead
/// considered isolated when it is neither preceded by an ASCII letter, digit,
/// or hyphen nor followed by an ASCII letter or digit, or by a hyphen and a
/// digit. Hangul on either side is allowed, while digit runs inside hashes,
/// UUIDs, and other identifiers are rejected.
pub(crate) fn has_digit_boundaries(text: &str, start: usize, end: usize) -> bool {
    let bytes = text.as_bytes();
    let preceded = start
        .checked_sub(1)
        .is_some_and(|i| bytes[i].is_ascii_alphanumeric() || bytes[i] == b'-');
    let followed = match bytes.get(end) {
        Some(b) if b.is_ascii_alphanumeric() => true,
        Some(b'-') => bytes.get(end + 1).is_some_and(u8::is_ascii_digit),
        _ => false,
    };
    !preceded && !followed
}

/// Labels that, written immediately before a number, identify it as a business
/// identifier rather than personal data.
///
/// Only compound labels are listed. Bare words such as "고객" (customer) or
/// "회원" (member) are excluded on purpose: "고객 010-1234-5678" is a
/// customer's phone number, not an identifier.
const NON_PII_LABELS: &[&str] = &[
    "주문번호",
    "송장번호",
    "운송장번호",
    "운송장",
    "회원번호",
    "고객번호",
    "상품번호",
    "상품코드",
    "접수번호",
    "예약번호",
    "결제번호",
    "거래번호",
    "승인번호",
    "일련번호",
    "모델번호",
    "order id",
    "order no",
    "tracking no",
    "invoice no",
];

/// Returns whether the number starting at `start` is labelled as a business
/// identifier, as in "주문번호: 2026..." or "회원번호는 0420...".
///
/// The label must end directly before the number, allowing only whitespace,
/// label punctuation, and a topic particle in between, so a label elsewhere in
/// the sentence never suppresses an unrelated number.
pub(crate) fn is_labeled_non_pii(text: &str, start: usize) -> bool {
    let before = text[..start]
        .trim_end_matches(|c: char| c.is_whitespace() || matches!(c, ':' | '：' | '#' | '=' | '.'));
    let before = before
        .strip_suffix('는')
        .or_else(|| before.strip_suffix('은'))
        .unwrap_or(before)
        .trim_end();
    NON_PII_LABELS
        .iter()
        .any(|label| ends_with_ignore_ascii_case(before, label))
}

fn ends_with_ignore_ascii_case(text: &str, suffix: &str) -> bool {
    text.len() >= suffix.len()
        && text.is_char_boundary(text.len() - suffix.len())
        && text[text.len() - suffix.len()..].eq_ignore_ascii_case(suffix)
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
    fn labels_directly_before_a_number_mark_it_as_non_pii() {
        for text in [
            "주문번호 2026",
            "주문번호: 2026",
            "주문번호는 2026",
            "송장번호 #2026",
            "Order ID: 2026",
        ] {
            assert!(is_labeled_non_pii(text, text.len() - 4), "input: {text}");
        }
    }

    #[test]
    fn distant_or_bare_labels_do_not_suppress_numbers() {
        for text in [
            "주문번호 확인 후 연락처 2026", // Label does not precede the number
            "고객 2026",                    // Bare word, not a compound label
            "회원 2026",
            "2026",
        ] {
            assert!(!is_labeled_non_pii(text, text.len() - 4), "input: {text}");
        }
    }

    #[test]
    fn digit_boundaries_reject_digit_runs_inside_identifiers() {
        let hash = "8f13b2b0213660407c7ba5e79dc600f66296911e";
        let start = hash.find("0213660407").unwrap();
        assert!(!has_digit_boundaries(hash, start, start + 10));
    }

    #[test]
    fn digit_boundaries_reject_adjacent_digits() {
        assert!(!has_digit_boundaries("91234", 1, 5));
        assert!(!has_digit_boundaries("1234-5", 0, 4));
        assert!(has_digit_boundaries("1234-", 0, 4));
    }
}
