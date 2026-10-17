//! Recognizer for payment card numbers.

use std::sync::LazyLock;

use super::{
    Recognizer, group, has_digit_boundaries, is_labeled_non_pii, mask_digits, only_digits,
};
use crate::checksum::luhn_is_valid;
use crate::entity::EntityType;
use crate::pattern::{Captures, Pattern, Prefilter};

static PATTERN: LazyLock<Pattern> = LazyLock::new(|| {
    Pattern::new(
        r"(?x)
        # 16 digits in four groups of four: Visa, Mastercard, JCB, domestic cards.
        [0-9]{4} (?P<s1>[-\ ]?) [0-9]{4} (?P<s2>[-\ ]?) [0-9]{4} (?P<s3>[-\ ]?) [0-9]{4}
        |
        # 15 digits grouped 4-6-5: American Express.
        3[47][0-9]{2} (?P<a1>[-\ ]?) [0-9]{6} (?P<a2>[-\ ]?) [0-9]{5}
        ",
        Prefilter::Disabled,
    )
    .expect("card pattern is valid")
});

const SCORE_SEPARATED: f64 = 0.9;
/// Roughly one in ten arbitrary digit strings passes Luhn, so an unseparated
/// run is still plausibly an order or tracking number.
const SCORE_UNSEPARATED: f64 = 0.75;

/// PCI DSS permits displaying at most the first six and last four digits.
const VISIBLE_HEAD: usize = 6;
const VISIBLE_TAIL: usize = 4;

/// Detects payment card numbers and rejects those failing the Luhn check.
pub(crate) struct CardRecognizer;

impl Recognizer for CardRecognizer {
    fn entity_type(&self) -> EntityType {
        EntityType::Card
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
        let separators: &[&str] = if captures.name("a1").is_some() {
            &[group(captures, "a1"), group(captures, "a2")]
        } else {
            &[
                group(captures, "s1"),
                group(captures, "s2"),
                group(captures, "s3"),
            ]
        };
        if separators.iter().any(|s| *s != separators[0]) {
            return None;
        }
        let digits = only_digits(matched.as_str());
        if !luhn_is_valid(&digits) {
            return None;
        }
        // A run of one repeated digit, such as all zeros, passes Luhn trivially.
        if digits.bytes().all(|b| b == digits.as_bytes()[0]) {
            return None;
        }
        Some(if separators[0].is_empty() {
            SCORE_UNSEPARATED
        } else {
            SCORE_SEPARATED
        })
    }

    fn mask_partial(&self, value: &str) -> String {
        mask_digits(value, VISIBLE_HEAD, VISIBLE_TAIL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<&str> {
        CardRecognizer
            .find(text)
            .iter()
            .map(|e| e.as_str(text))
            .collect()
    }

    #[test]
    fn detects_luhn_valid_numbers() {
        for number in [
            "4111-1111-1111-1111",
            "4111 1111 1111 1111",
            "5555555555554444",
            "3782-822463-10005",
        ] {
            let text = format!("카드번호 {number}로 결제");
            assert_eq!(found(&text), [number], "input: {text}");
        }
    }

    #[test]
    fn rejects_invalid_numbers() {
        for text in [
            "4111-1111-1111-1112",      // Fails Luhn
            "0000-0000-0000-0000",      // Placeholder that passes Luhn
            "4111-1111 1111-1111",      // Inconsistent separators
            "4111-1111-1111-1111-1111", // Part of a longer digit sequence
        ] {
            assert!(found(text).is_empty(), "input: {text}");
        }
    }

    #[test]
    fn rejects_luhn_valid_numbers_labelled_as_order_ids() {
        // 2026101712345679 passes Luhn by chance; the label marks it as an order ID.
        for text in ["주문번호 2026101712345679", "주문번호: 2026101712345679"] {
            assert!(found(text).is_empty(), "input: {text}");
        }
        assert_eq!(found("결제 2026101712345679"), ["2026101712345679"]);
    }

    #[test]
    fn mask_partial_follows_pci_dss() {
        assert_eq!(
            CardRecognizer.mask_partial("4111-1111-1111-1111"),
            "4111-11**-****-1111"
        );
    }
}
