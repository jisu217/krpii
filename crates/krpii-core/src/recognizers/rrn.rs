//! Recognizer for Korean resident registration numbers (주민등록번호).

use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    Recognizer, group, has_digit_boundaries, is_labeled_non_pii, mask_digits, only_digits,
};
use crate::checksum::rrn_checksum_is_valid;
use crate::entity::EntityType;
use crate::pattern::{Captures, Pattern, Prefilter};

/// Layout: `YYMMDD-GNNNNNN`, where `G` encodes century, sex, and nationality.
static PATTERN: LazyLock<Pattern> = LazyLock::new(|| {
    Pattern::new(
        r"(?x)
        (?P<yy>[0-9]{2}) (?P<mm>0[1-9]|1[0-2]) (?P<dd>0[1-9]|[12][0-9]|3[01])
        (?P<sep>[-\ ]?)
        (?P<gender>[1-8])
        [0-9]{6}
        ",
        Prefilter::Disabled,
    )
    .expect("resident registration number pattern is valid")
});

const SCORE_CHECKSUM_VALID: f64 = 0.95;
/// Numbers issued since October 2020 need not satisfy the legacy checksum, so
/// a mismatch lowers confidence instead of rejecting the candidate outright.
const SCORE_CHECKSUM_INVALID: f64 = 0.8;
const UNSEPARATED_PENALTY: f64 = 0.1;

/// Only the date of birth stays visible; the remaining seven digits are masked.
const VISIBLE_HEAD: usize = 6;

/// Detects resident registration numbers whose birth date is real and not in the future.
pub(crate) struct ResidentRegistrationNumberRecognizer;

impl Recognizer for ResidentRegistrationNumberRecognizer {
    fn entity_type(&self) -> EntityType {
        EntityType::Rrn
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
        let birth_date = parse_birth_date(captures)?;
        if birth_date > today_utc() {
            return None;
        }
        let base = if rrn_checksum_is_valid(&only_digits(matched.as_str())) {
            SCORE_CHECKSUM_VALID
        } else {
            SCORE_CHECKSUM_INVALID
        };
        Some(if group(captures, "sep").is_empty() {
            base - UNSEPARATED_PENALTY
        } else {
            base
        })
    }

    fn mask_partial(&self, value: &str) -> String {
        mask_digits(value, VISIBLE_HEAD, 0)
    }
}

/// A calendar date as `(year, month, day)`, ordered chronologically by tuple comparison.
type Date = (i64, u32, u32);

/// Resolves the captured birth date to a calendar date, or `None` if it does not exist.
fn parse_birth_date(captures: &Captures<'_>) -> Option<Date> {
    // Gender digits 1-4 are issued to citizens and 5-8 to registered foreigners;
    // odd/even pairs within each group share a century.
    let century = match group(captures, "gender") {
        "1" | "2" | "5" | "6" => 1900,
        "3" | "4" | "7" | "8" => 2000,
        _ => return None,
    };
    let year = century + group(captures, "yy").parse::<i64>().ok()?;
    let month = group(captures, "mm").parse::<u32>().ok()?;
    let day = group(captures, "dd").parse::<u32>().ok()?;
    // Rejects dates such as February 30th that the pattern alone cannot exclude.
    (day <= days_in_month(year, month)).then_some((year, month, day))
}

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        2 if is_leap_year(year) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Returns today's date in UTC.
///
/// UTC may lag Korea Standard Time by one day, which at worst accepts a
/// birth date of "tomorrow" for a few hours; the trade-off avoids a
/// time-zone dependency.
fn today_utc() -> Date {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    civil_from_days(i64::try_from(seconds / 86_400).unwrap_or(i64::MAX))
}

/// Converts days since 1970-01-01 to a proleptic Gregorian date.
///
/// Implements Howard Hinnant's `civil_from_days` algorithm
/// (<https://howardhinnant.github.io/date_algorithms.html>).
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn civil_from_days(days: i64) -> Date {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1_460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = (day_of_year - (153 * shifted_month + 2) / 5 + 1) as u32;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    } as u32;
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn found(text: &str) -> Vec<&str> {
        ResidentRegistrationNumberRecognizer
            .find(text)
            .iter()
            .map(|e| e.as_str(text))
            .collect()
    }

    #[test]
    fn detects_numbers_with_valid_birth_dates() {
        for number in [
            "900101-1234568", // Citizen born in 1990, legacy checksum valid
            "900101-1234567", // Post-2020 style: checksum invalid but still an RRN
            "050505-3123456", // Citizen born in 2005
            "880229-5123456", // Foreigner born in 1988 (leap day)
            "9001011234568",  // Unseparated
        ] {
            let text = format!("주민번호 {number}입니다");
            assert_eq!(found(&text), [number], "input: {text}");
        }
    }

    #[test]
    fn rejects_impossible_dates() {
        for text in [
            "900230-1234568", // February 30th
            "890229-1234568", // 1989 is not a leap year
            "901301-1234568", // Month 13
            "900101-9234568", // Gender digit outside 1-8
            "990101-3234568", // Born in 2099, which is in the future
        ] {
            assert!(found(text).is_empty(), "input: {text}");
        }
    }

    #[test]
    fn checksum_mismatch_lowers_score_without_rejecting() {
        let valid = &ResidentRegistrationNumberRecognizer.find("900101-1234568")[0];
        let mismatch = &ResidentRegistrationNumberRecognizer.find("900101-1234567")[0];
        assert!(mismatch.score < valid.score);
    }

    #[test]
    fn mask_partial_keeps_only_birth_date() {
        assert_eq!(
            ResidentRegistrationNumberRecognizer.mask_partial("900101-1234568"),
            "900101-*******"
        );
    }

    #[test]
    fn civil_from_days_matches_known_dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(11_016), (2000, 2, 29));
        assert_eq!(civil_from_days(20_743), (2026, 10, 17));
    }
}
