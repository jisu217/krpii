//! Masking: replace detected entities with obscured or labelled text.

use std::fmt;
use std::str::FromStr;

use crate::detector::Detector;
use crate::entity::Entity;
use crate::error::ParseError;
use crate::recognizers::recognizer_for;

/// How each detected entity is rewritten.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum MaskStyle {
    /// Obscures the identifying digits while keeping the format, e.g.
    /// `010-****-5678`. Suited to logs and user-facing displays.
    #[default]
    Partial,
    /// Replaces the entity with its type label, e.g. `<PHONE>`. Suited to LLM
    /// prompts, where the model should know *that* a value was present
    /// without seeing it.
    Tag,
}

impl MaskStyle {
    const NAMES: &'static [&'static str] = &["partial", "tag"];

    /// The lower-case name used in CLI flags and configuration.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Partial => "partial",
            Self::Tag => "tag",
        }
    }
}

impl fmt::Display for MaskStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for MaskStyle {
    type Err = ParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        [Self::Partial, Self::Tag]
            .into_iter()
            .find(|style| style.as_str().eq_ignore_ascii_case(value))
            .ok_or_else(|| ParseError::new("mask style", value, Self::NAMES))
    }
}

/// Returns `text` with every detected entity masked, using default detection settings.
#[must_use]
pub fn mask(text: &str, style: MaskStyle) -> String {
    Detector::default().mask(text, style)
}

/// Rewrites pre-computed `entities` within `text`.
///
/// Exposed separately so callers can inspect or filter the output of
/// [`detect`](crate::detect) before masking, without scanning the text twice.
/// Entities may be given in any order; any entity overlapping an earlier one
/// is skipped, so the output is always well-formed.
#[must_use]
pub fn apply_mask(text: &str, entities: &[Entity], style: MaskStyle) -> String {
    let mut ordered: Vec<&Entity> = entities.iter().collect();
    ordered.sort_by_key(|entity| entity.start);

    let mut masked = String::with_capacity(text.len());
    let mut cursor = 0;
    for entity in ordered {
        if entity.start < cursor {
            continue;
        }
        masked.push_str(&text[cursor..entity.start]);
        match style {
            MaskStyle::Tag => {
                masked.push('<');
                masked.push_str(entity.entity_type.as_str());
                masked.push('>');
            }
            MaskStyle::Partial => masked
                .push_str(&recognizer_for(entity.entity_type).mask_partial(entity.as_str(text))),
        }
        cursor = entity.end;
    }
    masked.push_str(&text[cursor..]);
    masked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EntityType, detect};

    const SAMPLE: &str = "홍길동 010-1234-5678, 주민번호 900101-1234567로 가입";

    #[test]
    fn partial_style_obscures_identifying_digits() {
        assert_eq!(
            mask(SAMPLE, MaskStyle::Partial),
            "홍길동 010-****-5678, 주민번호 900101-*******로 가입"
        );
    }

    #[test]
    fn tag_style_replaces_with_type_labels() {
        assert_eq!(
            mask(SAMPLE, MaskStyle::Tag),
            "홍길동 <PHONE>, 주민번호 <RRN>로 가입"
        );
    }

    #[test]
    fn type_filter_leaves_other_entities_untouched() {
        let masked = Detector::new()
            .with_types([EntityType::Rrn])
            .mask(SAMPLE, MaskStyle::Partial);
        assert_eq!(
            masked,
            "홍길동 010-1234-5678, 주민번호 900101-*******로 가입"
        );
    }

    #[test]
    fn text_without_pii_is_unchanged() {
        let text = "오늘 회의는 3시에 시작합니다.";
        assert_eq!(mask(text, MaskStyle::Partial), text);
    }

    #[test]
    fn partial_masking_preserves_length() {
        assert_eq!(mask(SAMPLE, MaskStyle::Partial).len(), SAMPLE.len());
    }

    #[test]
    fn apply_mask_reuses_filtered_detection_results() {
        let entities: Vec<Entity> = detect(SAMPLE)
            .into_iter()
            .filter(|e| e.score >= 0.9)
            .collect();
        assert_eq!(
            apply_mask(SAMPLE, &entities, MaskStyle::Tag),
            "홍길동 <PHONE>, 주민번호 900101-1234567로 가입"
        );
    }

    #[test]
    fn mask_style_parses_case_insensitively() {
        assert_eq!("TAG".parse::<MaskStyle>(), Ok(MaskStyle::Tag));
        assert!("redact".parse::<MaskStyle>().is_err());
    }
}
