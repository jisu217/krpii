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

/// Frames for credentials, in the places they usually leak from: environment
/// files, shell exports, HTTP headers, and prose.
const SECRET_FRAMES: &[(&str, &str)] = &[
    ("OPENAI_API_KEY=", ""),
    ("export GITHUB_TOKEN=", ""),
    ("Authorization: Bearer ", ""),
    ("키 값은 ", "입니다"),
    ("", ""),
];

/// Frames that name the document a passport or licence number belongs to.
const PASSPORT_FRAMES: &[(&str, &str)] = &[
    ("여권번호: ", ""),
    ("Passport No. ", ""),
    ("여권 ", " 확인 완료"),
];
const DRIVER_LICENSE_FRAMES: &[(&str, &str)] = &[
    ("운전면허번호 ", ""),
    ("면허번호: ", ""),
    ("Driver license ", ""),
];

/// Share of passport and licence numbers framed with a document keyword. The
/// rest use generic frames, so formats that krpii reports only with a keyword
/// count as misses there instead of being hidden from the benchmark.
const DOCUMENT_KEYWORD_RATIO: f64 = 0.7;

/// Share of credentials generated as a connection-string password rather than
/// a token or private key.
const DB_PASSWORD_RATIO: f64 = 0.25;

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
        DecoyKind::CommitHash => ("커밋 ", " 기준으로 배포"),
        DecoyKind::Uuid => ("request_id=", ""),
        DecoyKind::SerialNumber => ("시리얼 ", ""),
        DecoyKind::PlaceholderKey => ("예시: ", ""),
    }
}

/// Punctuation placed between segments of a multi-segment sample.
const SEGMENT_SEPARATORS: &[&str] = &[", ", " / ", ". "];

/// Share of segments that contain PII rather than a decoy.
const PII_RATIO: f64 = 0.6;

/// Share of decoys written without their label (for example a bare order ID
/// instead of "주문번호 ..."). Real logs often omit labels, so a detector must not
/// be rewarded for label rules beyond what such text would allow.
const UNLABELED_DECOY_RATIO: f64 = 0.3;
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
            // Bind before use: passing the result straight to `push_text` lets
            // Rust 1.85 infer `T = str` from the parameter type and fail.
            let separator = *rng.pick(SEGMENT_SEPARATORS);
            builder.push_text(separator);
        }
        if rng.chance(PII_RATIO) {
            let entity_type = *rng.pick(&EntityType::ALL);
            let (prefix, value, suffix) = if entity_type != EntityType::Secret {
                let frames = match entity_type {
                    EntityType::Passport if rng.chance(DOCUMENT_KEYWORD_RATIO) => PASSPORT_FRAMES,
                    EntityType::DriverLicense if rng.chance(DOCUMENT_KEYWORD_RATIO) => {
                        DRIVER_LICENSE_FRAMES
                    }
                    _ => PII_FRAMES,
                };
                let (prefix, suffix) = *rng.pick(frames);
                (prefix, generators::entity(rng, entity_type), suffix)
            } else if rng.chance(DB_PASSWORD_RATIO) {
                // Only the password is sensitive, so the URL around it is the frame.
                let password = generators::db_password(rng);
                (
                    "DATABASE_URL=postgres://app:",
                    password,
                    "@db.internal:5432/app",
                )
            } else {
                let (prefix, suffix) = *rng.pick(SECRET_FRAMES);
                (prefix, generators::entity(rng, entity_type), suffix)
            };
            builder.push_framed(prefix, &value, suffix, Label::Pii(entity_type));
        } else {
            let kind = *rng.pick(&DecoyKind::ALL);
            let (prefix, suffix) = if rng.chance(UNLABELED_DECOY_RATIO) {
                ("", "")
            } else {
                decoy_frame(kind)
            };
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
