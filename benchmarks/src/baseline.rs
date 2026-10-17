//! Regex-only baseline detector for comparison.
//!
//! The baseline uses the same candidate shapes and digit-boundary rule as
//! krpii but performs no validation: no checksums, no birth-date checks, no
//! separator consistency, and no exclusion of service prefixes. Comparing it
//! with krpii on the same corpus therefore isolates the contribution of the
//! validation stage, rather than pitting krpii against a strawman.

use std::sync::LazyLock;

use krpii_core::{Entity, EntityType};
use regex::Regex;

/// Candidate patterns in priority order; earlier entries win ties on overlap.
static PATTERNS: LazyLock<Vec<(EntityType, Regex)>> = LazyLock::new(|| {
    [
        (
            EntityType::Email,
            r"[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}",
        ),
        (
            EntityType::Card,
            r"[0-9]{4}[- ]?[0-9]{4}[- ]?[0-9]{4}[- ]?[0-9]{4}|3[47][0-9]{2}[- ]?[0-9]{6}[- ]?[0-9]{5}",
        ),
        (EntityType::Rrn, r"[0-9]{6}[- ]?[1-8][0-9]{6}"),
        (
            EntityType::Phone,
            r"(?:(?:\+82[-. ]?|0)(?:1[016789]|2|[3-6][1-5]|70|80)[-. ]?|\(0(?:1[016789]|2|[3-6][1-5]|70|80)\) ?)[0-9]{3,4}[-. ]?[0-9]{4}",
        ),
        (EntityType::Brn, r"[0-9]{3}-?[0-9]{2}-?[0-9]{5}"),
    ]
    .into_iter()
    .map(|(entity_type, pattern)| {
        let regex = Regex::new(pattern).expect("baseline pattern is valid");
        (entity_type, regex)
    })
    .collect()
});

/// Confidence assigned to every baseline match, which has no notion of evidence.
const SCORE: f64 = 1.0;

/// Detects entities using pattern matching alone.
pub fn detect(text: &str) -> Vec<Entity> {
    let mut candidates: Vec<(usize, Entity)> = Vec::new();
    for (priority, (entity_type, regex)) in PATTERNS.iter().enumerate() {
        for found in regex.find_iter(text) {
            if is_isolated(text, found.start(), found.end()) {
                candidates.push((
                    priority,
                    Entity {
                        entity_type: *entity_type,
                        start: found.start(),
                        end: found.end(),
                        score: SCORE,
                    },
                ));
            }
        }
    }

    // Prefer longer spans, then pattern priority, then earlier positions.
    candidates.sort_by_key(|(priority, e)| (std::cmp::Reverse(e.len()), *priority, e.start));
    let mut accepted: Vec<Entity> = Vec::new();
    for (_, candidate) in candidates {
        if !accepted.iter().any(|kept| kept.overlaps(&candidate)) {
            accepted.push(candidate);
        }
    }
    accepted.sort_by_key(|e| e.start);
    accepted
}

/// Mirrors krpii's digit-boundary rule so both detectors see the same candidates.
fn is_isolated(text: &str, start: usize, end: usize) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_checksum_invalid_card() {
        let entities = detect("4111-1111-1111-1112");
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_type, EntityType::Card);
    }

    #[test]
    fn accepts_impossible_birth_date() {
        let entities = detect("900230-1234568");
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_type, EntityType::Rrn);
    }

    #[test]
    fn respects_digit_boundaries() {
        assert_eq!(detect("1234567890123456789"), []);
    }
}
