//! Recognizer for Korean driver's licence numbers (운전면허번호).
//!
//! A licence number has 12 digits laid out as region, year of issue, serial,
//! and check digits: `11-12-345678-90`. Older licences print the region as a
//! name instead of a code: `서울 12-345678-90`. The check-digit algorithm is
//! not public, so validation relies on the region and the layout.
//!
//! The grouped layout is distinctive and is reported on its own. Twelve
//! unseparated digits look exactly like a parcel tracking number, so they are
//! reported only when a licence keyword precedes them.

use std::sync::LazyLock;

use super::{
    Recognizer, group, has_digit_boundaries, is_labeled_non_pii, mask_digits, preceded_by_keyword,
};
use crate::entity::EntityType;
use crate::pattern::{Captures, Pattern, Prefilter};

static PATTERN: LazyLock<Pattern> = LazyLock::new(|| {
    Pattern::new(
        r"(?x)
        (?:
            # Region code assigned by the National Police Agency.
            (?P<code> 1[1-9] | 2[0-6] | 28 ) (?P<s1>[-\ ]?)
          |
            # Region name printed on older licences.
            (?P<name> 서울|부산|경기|강원|충북|충남|전북|전남|경북|경남|제주|대구|인천|광주|대전|울산 ) \ ?
        )
        [0-9]{2} (?P<s2>[-\ ]?) [0-9]{6} (?P<s3>[-\ ]?) [0-9]{2}
        ", Prefilter::Disabled)
    .expect("driver licence pattern is valid")
});

/// Keywords that identify a nearby number as a licence number.
const KEYWORDS: &[&str] = &["면허", "driver", "license", "licence"];

const SCORE_GROUPED: f64 = 0.9;
const SCORE_WITH_CONTEXT: f64 = 0.95;
const SCORE_UNSEPARATED_WITH_CONTEXT: f64 = 0.85;

/// Detects driver's licence numbers in the grouped layout, and unseparated
/// numbers when a licence keyword precedes them.
pub(crate) struct DriverLicenseRecognizer;

impl Recognizer for DriverLicenseRecognizer {
    fn entity_type(&self) -> EntityType {
        EntityType::DriverLicense
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
        let separator = group(captures, "s2");
        let consistent = separator == group(captures, "s3")
            && (captures.name("name").is_some() || separator == group(captures, "s1"));
        if !consistent {
            return None;
        }
        let context = preceded_by_keyword(text, matched.start(), KEYWORDS);
        let grouped = !separator.is_empty() || captures.name("name").is_some();
        match (grouped, context) {
            (true, true) => Some(SCORE_WITH_CONTEXT),
            (true, false) => Some(SCORE_GROUPED),
            (false, true) => Some(SCORE_UNSEPARATED_WITH_CONTEXT),
            (false, false) => None,
        }
    }

    fn mask_partial(&self, value: &str) -> String {
        // Keep the region, which is shared by everyone licensed there.
        let visible = if value.starts_with(|c: char| c.is_ascii_digit()) {
            2
        } else {
            0
        };
        mask_digits(value, visible, 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<&str> {
        DriverLicenseRecognizer
            .find(text)
            .iter()
            .map(|e| e.as_str(text))
            .collect()
    }

    #[test]
    fn detects_grouped_layouts_without_context() {
        for number in ["11-12-345678-90", "28 12 345678 90", "서울 12-345678-90"] {
            let text = format!("확인 {number}입니다");
            assert_eq!(found(&text), [number], "input: {text}");
        }
    }

    #[test]
    fn detects_unseparated_digits_only_with_a_licence_keyword() {
        assert_eq!(found("운전면허번호: 111234567890"), ["111234567890"]);
        assert!(found("111234567890").is_empty(), "bare 12 digits");
    }

    #[test]
    fn rejects_invalid_regions_and_inconsistent_separators() {
        for text in [
            "10-12-345678-90",
            "27-12-345678-90",
            "11-12 345678-90",
            "송장번호 11-12-345678-90",
        ] {
            assert!(found(text).is_empty(), "input: {text}");
        }
    }

    #[test]
    fn mask_partial_keeps_the_region() {
        assert_eq!(
            DriverLicenseRecognizer.mask_partial("11-12-345678-90"),
            "11-**-******-**"
        );
        assert_eq!(
            DriverLicenseRecognizer.mask_partial("서울 12-345678-90"),
            "서울 **-******-**"
        );
    }
}
