//! Recognizer for Korean telephone numbers.

use std::sync::LazyLock;

use regex::{Captures, Regex};

use super::{MASK_CHAR, Recognizer, group, has_digit_boundaries};
use crate::entity::EntityType;

/// Matches the domestic trunk prefix `0` or the international prefix `+82`,
/// followed by a subscriber area code. Service prefixes such as `1588` or
/// `080` are intentionally excluded because they do not identify a person.
static PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x)
        (?: 0 | \+82[-.\ ]? )
        (?P<area>
            1[016789]                       # mobile
          | 2                               # Seoul
          | 3[1-3] | 4[1-4] | 5[1-5] | 6[1-4] # regional
          | 70                              # VoIP
        )
        (?P<sep1>[-.\ ]?)
        (?P<middle>[0-9]{3,4})
        (?P<sep2>[-.\ ]?)
        (?P<last>[0-9]{4})
        ",
    )
    .expect("phone pattern is valid")
});

const SCORE_MOBILE: f64 = 0.95;
const SCORE_LANDLINE: f64 = 0.85;
/// Unseparated digit runs collide with order numbers and other identifiers,
/// so they are reported with lower confidence than formatted numbers.
const UNSEPARATED_PENALTY: f64 = 0.15;

/// Detects mobile, landline, and `VoIP` numbers in common Korean notations,
/// such as `010-1234-5678`, `02 123 4567`, `01012345678`, and `+82 10-1234-5678`.
pub(crate) struct PhoneRecognizer;

impl Recognizer for PhoneRecognizer {
    fn entity_type(&self) -> EntityType {
        EntityType::Phone
    }

    fn pattern(&self) -> &Regex {
        &PATTERN
    }

    fn score(&self, text: &str, captures: &Captures<'_>) -> Option<f64> {
        let matched = captures.get(0)?;
        if !has_digit_boundaries(text, matched.start(), matched.end()) {
            return None;
        }
        let separator = group(captures, "sep1");
        if separator != group(captures, "sep2") {
            return None;
        }
        let area = group(captures, "area");
        // Current 010 numbers always carry a 4-digit middle group; only the
        // legacy 01[16789] prefixes may have three digits.
        if area == "10" && group(captures, "middle").len() != 4 {
            return None;
        }
        let base = if area.starts_with('1') {
            SCORE_MOBILE
        } else {
            SCORE_LANDLINE
        };
        Some(if separator.is_empty() {
            base - UNSEPARATED_PENALTY
        } else {
            base
        })
    }

    fn mask_partial(&self, value: &str) -> String {
        let Some(middle) = PATTERN.captures(value).and_then(|c| c.name("middle")) else {
            return value.to_owned();
        };
        let mut masked = String::with_capacity(value.len());
        masked.push_str(&value[..middle.start()]);
        masked.extend(std::iter::repeat_n(MASK_CHAR, middle.len()));
        masked.push_str(&value[middle.end()..]);
        masked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<&str> {
        PhoneRecognizer
            .find(text)
            .iter()
            .map(|e| e.as_str(text))
            .collect()
    }

    #[test]
    fn detects_supported_formats() {
        for number in [
            "010-1234-5678",
            "010 1234 5678",
            "010.1234.5678",
            "01012345678",
            "011-123-4567",
            "02-123-4567",
            "02-1234-5678",
            "031-123-4567",
            "064-1234-5678",
            "070-1234-5678",
            "+82 10-1234-5678",
            "+82-2-1234-5678",
        ] {
            let text = format!("연락처: {number}로 연락 주세요");
            assert_eq!(found(&text), [number], "input: {text}");
        }
    }

    #[test]
    fn rejects_non_subscriber_or_malformed_numbers() {
        for text in [
            "010-123-4567",   // Modern 010 numbers have a 4-digit middle group
            "010-1234.5678",  // Inconsistent separators
            "1588-1234",      // Service number
            "080-123-4567",   // Toll-free prefix
            "010-1234-56789", // Part of a longer digit run
            "9010-1234-5678", // Part of a longer digit run
        ] {
            assert!(found(text).is_empty(), "input: {text}");
        }
    }

    #[test]
    fn unseparated_numbers_score_lower() {
        let separated = &PhoneRecognizer.find("010-1234-5678")[0];
        let unseparated = &PhoneRecognizer.find("01012345678")[0];
        assert!(unseparated.score < separated.score);
    }

    #[test]
    fn mask_partial_hides_middle_group() {
        for (value, expected) in [
            ("010-1234-5678", "010-****-5678"),
            ("01012345678", "010****5678"),
            ("02-123-4567", "02-***-4567"),
            ("+82 10-1234-5678", "+82 10-****-5678"),
        ] {
            assert_eq!(PhoneRecognizer.mask_partial(value), expected);
        }
    }
}
