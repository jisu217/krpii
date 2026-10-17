//! Recognizer for Korean passport numbers (여권번호).
//!
//! Two formats are in circulation. The current format, issued since December
//! 2021, embeds a letter (`M123A4567`) and is distinctive on its own. The
//! previous format, a letter followed by eight digits (`M12345678`), is
//! indistinguishable from many product serials, so it is reported only when a
//! passport keyword precedes it.

use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::{MASK_CHAR, Recognizer, has_digit_boundaries, is_labeled_non_pii, preceded_by_keyword};
use crate::entity::EntityType;

/// The leading letter encodes the passport type: M (multiple), S (single),
/// R (residence), O (official), and D (diplomatic).
static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?P<current>[MSROD][0-9]{3}[A-Z][0-9]{4})|(?P<previous>[MSROD][0-9]{8})")
        .expect("passport pattern is valid")
});

/// Keywords that identify a nearby number as a passport number.
const KEYWORDS: &[&str] = &["여권", "passport"];

const SCORE_CURRENT: f64 = 0.85;
const SCORE_WITH_CONTEXT: f64 = 0.95;
const SCORE_PREVIOUS_WITH_CONTEXT: f64 = 0.9;

/// Detects passport numbers in the current format, and in the previous format
/// when a passport keyword precedes them.
pub(crate) struct PassportRecognizer;

impl Recognizer for PassportRecognizer {
    fn entity_type(&self) -> EntityType {
        EntityType::Passport
    }

    fn pattern(&self) -> &Regex {
        &PATTERN
    }

    fn score(&self, text: &str, captures: &Captures<'_>) -> Option<f64> {
        let matched = captures.get(0)?;
        if !has_digit_boundaries(text, matched.start(), matched.end())
            || is_labeled_non_pii(text, matched.start())
        {
            return None;
        }
        let context = preceded_by_keyword(text, matched.start(), KEYWORDS);
        if captures.name("current").is_some() {
            Some(if context {
                SCORE_WITH_CONTEXT
            } else {
                SCORE_CURRENT
            })
        } else {
            context.then_some(SCORE_PREVIOUS_WITH_CONTEXT)
        }
    }

    fn mask_partial(&self, value: &str) -> String {
        // The leading letter reveals only the passport type.
        let mut chars = value.chars();
        let first = chars.next().unwrap_or(MASK_CHAR);
        let hidden = chars.count();
        format!("{first}{}", MASK_CHAR.to_string().repeat(hidden))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<&str> {
        PassportRecognizer
            .find(text)
            .iter()
            .map(|e| e.as_str(text))
            .collect()
    }

    #[test]
    fn detects_the_current_format_without_context() {
        for text in ["M123A4567", "확인 부탁드립니다 S987B6543입니다"] {
            assert_eq!(found(text).len(), 1, "input: {text}");
        }
    }

    #[test]
    fn detects_the_previous_format_only_with_a_passport_keyword() {
        assert_eq!(found("여권번호: M12345678"), ["M12345678"]);
        assert_eq!(found("Passport No. R87654321"), ["R87654321"]);
        assert!(
            found("시리얼 S12345678").is_empty(),
            "serial without keyword"
        );
        assert!(found("M12345678").is_empty(), "bare previous format");
    }

    #[test]
    fn rejects_numbers_inside_identifiers_or_labelled_as_codes() {
        for text in ["XM123A4567", "M123A45678", "상품코드 M123A4567"] {
            assert!(found(text).is_empty(), "input: {text}");
        }
    }

    #[test]
    fn mask_partial_keeps_the_type_letter() {
        assert_eq!(PassportRecognizer.mask_partial("M123A4567"), "M********");
    }
}
