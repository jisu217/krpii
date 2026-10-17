//! Recognizer for Korean business registration numbers (사업자등록번호).

use std::sync::LazyLock;

use super::{
    Recognizer, group, has_digit_boundaries, is_labeled_non_pii, mask_digits, only_digits,
};
use crate::checksum::brn_is_valid;
use crate::entity::EntityType;
use crate::pattern::{Captures, Pattern, Prefilter};

/// Canonical layout is `XXX-XX-XXXXX`: tax office, business type, serial and check digit.
static PATTERN: LazyLock<Pattern> = LazyLock::new(|| {
    Pattern::new(
        r"[0-9]{3}(?P<s1>[- ]?)[0-9]{2}(?P<s2>[- ]?)[0-9]{5}",
        Prefilter::Disabled,
    )
    .expect("business registration number pattern is valid")
});

const SCORE_SEPARATED: f64 = 0.9;
const SCORE_UNSEPARATED: f64 = 0.7;

/// The trailing five digits (serial and check digit) identify the business.
const MASKED_TAIL: usize = 5;
const DIGIT_COUNT: usize = 10;

/// Detects business registration numbers validated by the NTS checksum.
pub(crate) struct BusinessRegistrationNumberRecognizer;

impl Recognizer for BusinessRegistrationNumberRecognizer {
    fn entity_type(&self) -> EntityType {
        EntityType::Brn
    }

    fn pattern(&self) -> &Pattern {
        &PATTERN
    }

    fn may_match(&self, text: &str) -> bool {
        super::contains_digit(text)
    }

    fn score(&self, text: &str, captures: &Captures<'_>) -> Option<f64> {
        let matched = captures.get(0)?;
        if !has_digit_boundaries(text, matched.start(), matched.end())
            || is_labeled_non_pii(text, matched.start())
        {
            return None;
        }
        let separator = group(captures, "s1");
        if separator != group(captures, "s2") {
            return None;
        }
        if !brn_is_valid(&only_digits(matched.as_str())) {
            return None;
        }
        Some(if separator.is_empty() {
            SCORE_UNSEPARATED
        } else {
            SCORE_SEPARATED
        })
    }

    fn mask_partial(&self, value: &str) -> String {
        mask_digits(value, DIGIT_COUNT - MASKED_TAIL, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<&str> {
        BusinessRegistrationNumberRecognizer
            .find(text)
            .iter()
            .map(|e| e.as_str(text))
            .collect()
    }

    #[test]
    fn detects_checksum_valid_numbers() {
        for number in ["123-45-67891", "1234567891"] {
            let text = format!("사업자등록번호: {number}");
            assert_eq!(found(&text), [number], "input: {text}");
        }
    }

    #[test]
    fn rejects_invalid_numbers() {
        for text in ["123-45-67890", "123-4567891"] {
            assert!(found(text).is_empty(), "input: {text}");
        }
    }

    #[test]
    fn rejects_checksum_valid_numbers_labelled_as_member_ids() {
        // 5506073448 passes the BRN checksum by chance; the label marks it as a member ID.
        assert!(
            found("회원번호 5506073448").is_empty(),
            "labelled member ID"
        );
        assert_eq!(found("사업자 5506073448"), ["5506073448"]);
    }

    #[test]
    fn mask_partial_hides_serial_and_check_digit() {
        assert_eq!(
            BusinessRegistrationNumberRecognizer.mask_partial("123-45-67891"),
            "123-45-*****"
        );
    }
}
