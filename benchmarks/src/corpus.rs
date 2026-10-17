//! Synthetic corpus assembly with ground-truth annotations.

use krpii_core::EntityType;

use crate::generators::{self, DecoyKind};
use crate::rng::Rng;

/// What a labelled span contains.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Label {
    /// Real PII that a detector must find.
    Pii(EntityType),
    /// Non-PII text that a detector must leave untouched.
    Decoy(DecoyKind),
}

/// A labelled byte range within a sample's text.
#[derive(Debug, Clone, Copy)]
pub struct Span {
    pub label: Label,
    pub start: usize,
    pub end: usize,
}

/// One annotated input string.
#[derive(Debug, Clone)]
pub struct Sample {
    pub text: String,
    pub spans: Vec<Span>,
}

/// Korean sentence frames as `(prefix, suffix)` around a PII value. Suffixes
/// attach particles directly to the value, as real Korean text does.
const PII_FRAMES: &[(&str, &str)] = &[
    ("연락처: ", ""),
    ("", "로 연락 주세요"),
    ("담당자 정보는 ", "입니다"),
    ("확인 부탁드립니다 (", ")"),
    ("고객 ", " 등록 완료"),
    ("", ""),
];

/// Frames that place each decoy in a realistic context.
fn decoy_frame(kind: DecoyKind) -> (&'static str, &'static str) {
    match kind {
        DecoyKind::OrderId => ("주문번호 ", ""),
        DecoyKind::TrackingNumber => ("송장번호 ", "로 발송되었습니다"),
        DecoyKind::MemberId => ("회원번호 ", ""),
        DecoyKind::Amount => ("결제금액 ", "원"),
        DecoyKind::Timestamp => ("", " 기준"),
        DecoyKind::ProductCode => ("상품코드 ", ""),
        DecoyKind::ServiceNumber => ("고객센터 ", ""),
        DecoyKind::PostalCode => ("우편번호 ", ""),
        DecoyKind::IpAddress => ("접속 IP ", ""),
    }
}

/// Share of segments that contain PII rather than a decoy.
const PII_RATIO: f64 = 0.6;
const MAX_SEGMENTS_PER_SAMPLE: usize = 3;

/// Generates `count` samples deterministically from `seed`.
pub fn generate(seed: u64, count: usize) -> Vec<Sample> {
    let mut rng = Rng::new(seed);
    (0..count).map(|_| sample(&mut rng)).collect()
}

fn sample(rng: &mut Rng) -> Sample {
    let mut builder = SampleBuilder::default();
    for index in 0..rng.range(1, MAX_SEGMENTS_PER_SAMPLE) {
        if index > 0 {
            let separator: &str = rng.pick(&[", ", " / ", ". "]);
            builder.push_text(separator);
        }
        if rng.chance(PII_RATIO) {
            let entity_type = *rng.pick(&EntityType::ALL);
            let (prefix, suffix) = *rng.pick(PII_FRAMES);
            let value = generators::entity(rng, entity_type);
            builder.push_framed(prefix, &value, suffix, Label::Pii(entity_type));
        } else {
            let kind = *rng.pick(&DecoyKind::ALL);
            let (prefix, suffix) = decoy_frame(kind);
            let value = generators::decoy(rng, kind);
            builder.push_framed(prefix, &value, suffix, Label::Decoy(kind));
        }
    }
    builder.finish()
}

#[derive(Default)]
struct SampleBuilder {
    text: String,
    spans: Vec<Span>,
}

impl SampleBuilder {
    fn push_text(&mut self, text: &str) {
        self.text.push_str(text);
    }

    fn push_framed(&mut self, prefix: &str, value: &str, suffix: &str, label: Label) {
        self.text.push_str(prefix);
        let start = self.text.len();
        self.text.push_str(value);
        self.spans.push(Span {
            label,
            start,
            end: self.text.len(),
        });
        self.text.push_str(suffix);
    }

    fn finish(self) -> Sample {
        Sample {
            text: self.text,
            spans: self.spans,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generation_is_deterministic() {
        let a = generate(42, 50);
        let b = generate(42, 50);
        assert!(a.iter().zip(&b).all(|(x, y)| x.text == y.text));
    }

    #[test]
    fn spans_point_at_non_empty_values_on_char_boundaries() {
        for sample in generate(7, 500) {
            for span in &sample.spans {
                let value = &sample.text[span.start..span.end];
                assert!(!value.trim().is_empty(), "empty span in {:?}", sample.text);
            }
        }
    }

    #[test]
    fn corpus_contains_both_pii_and_decoys() {
        let spans: Vec<Label> = generate(1, 200)
            .into_iter()
            .flat_map(|s| s.spans)
            .map(|s| s.label)
            .collect();
        assert!(spans.iter().any(|l| matches!(l, Label::Pii(_))));
        assert!(spans.iter().any(|l| matches!(l, Label::Decoy(_))));
    }
}
