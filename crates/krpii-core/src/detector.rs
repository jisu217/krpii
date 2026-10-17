//! Entity detection: run recognizers and resolve overlapping candidates.

use crate::entity::{Entity, EntityType};
use crate::masking::{MaskStyle, apply_mask};
use crate::recognizers::recognizer_for;

/// Configurable entry point for detection and masking.
///
/// A default `Detector` enables every entity type with no score threshold.
/// It is cheap to construct; compiled patterns are shared process-wide.
#[derive(Debug, Clone)]
pub struct Detector {
    types: Vec<EntityType>,
    min_score: f64,
}

impl Default for Detector {
    fn default() -> Self {
        Self {
            types: EntityType::ALL.to_vec(),
            min_score: 0.0,
        }
    }
}

impl Detector {
    /// Creates a detector with every entity type enabled.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Restricts detection to `types`. Duplicates are ignored.
    #[must_use]
    pub fn with_types(mut self, types: impl IntoIterator<Item = EntityType>) -> Self {
        let mut types: Vec<EntityType> = types.into_iter().collect();
        types.sort_unstable();
        types.dedup();
        self.types = types;
        self
    }

    /// Discards entities whose confidence is below `min_score`.
    #[must_use]
    pub fn with_min_score(mut self, min_score: f64) -> Self {
        self.min_score = min_score;
        self
    }

    /// Returns non-overlapping entities in `text`, ordered by start offset.
    #[must_use]
    pub fn detect(&self, text: &str) -> Vec<Entity> {
        let candidates = self
            .types
            .iter()
            .flat_map(|&entity_type| recognizer_for(entity_type).find(text))
            .filter(|entity| entity.score >= self.min_score)
            .collect();
        resolve_overlaps(candidates)
    }

    /// Returns `text` with every detected entity rewritten according to `style`.
    #[must_use]
    pub fn mask(&self, text: &str, style: MaskStyle) -> String {
        apply_mask(text, &self.detect(text), style)
    }
}

/// Detects entities of every type in `text` using default settings.
#[must_use]
pub fn detect(text: &str) -> Vec<Entity> {
    Detector::default().detect(text)
}

/// Keeps the strongest candidate wherever spans overlap.
///
/// Candidates are ranked by score, then span length, then position, and
/// accepted greedily. For instance, `0212345678` matches both the phone and
/// business-number patterns; the higher-confidence interpretation wins.
fn resolve_overlaps(mut candidates: Vec<Entity>) -> Vec<Entity> {
    candidates.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| b.len().cmp(&a.len()))
            .then_with(|| a.start.cmp(&b.start))
    });
    let mut accepted: Vec<Entity> = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        if !accepted.iter().any(|kept| kept.overlaps(&candidate)) {
            accepted.push(candidate);
        }
    }
    accepted.sort_by_key(|entity| entity.start);
    accepted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_multiple_types_in_order() {
        let text = "홍길동 010-1234-5678, hong@example.com, 주민번호 900101-1234568";
        let types: Vec<EntityType> = detect(text).iter().map(|e| e.entity_type).collect();
        assert_eq!(
            types,
            [EntityType::Phone, EntityType::Email, EntityType::Rrn]
        );
    }

    #[test]
    fn offsets_point_into_original_text() {
        let text = "번호는 010-1234-5678입니다";
        let entities = detect(text);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].as_str(text), "010-1234-5678");
    }

    #[test]
    fn type_filter_limits_results() {
        let text = "010-1234-5678 hong@example.com";
        let entities = Detector::new().with_types([EntityType::Email]).detect(text);
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_type, EntityType::Email);
    }

    #[test]
    fn min_score_filters_low_confidence_entities() {
        let text = "01012345678";
        assert_eq!(detect(text).len(), 1);
        assert_eq!(Detector::new().with_min_score(0.99).detect(text), []);
    }

    #[test]
    fn overlapping_candidates_keep_one_entity() {
        let entities = detect("123-45-67891");
        assert_eq!(entities.len(), 1);
        assert_eq!(entities[0].entity_type, EntityType::Brn);
    }
}
