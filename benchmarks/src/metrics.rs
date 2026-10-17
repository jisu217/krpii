//! Scoring detector output against ground-truth annotations.
//!
//! A prediction counts as a true positive only when its type, start, and end
//! all match a gold span exactly. A correct span with the wrong type therefore
//! counts as both a false positive and a false negative. This is the strictest
//! common criterion, so reported numbers are a lower bound.
//!
//! Before matching, every predicted span is trimmed of surrounding whitespace
//! and of unbalanced brackets at its edges. Some tools report `(010-1234-5678`
//! for a number written in parentheses; the number itself was found, so the
//! stray punctuation should not turn a correct detection into an error.
//! Balanced brackets that belong to the value, as in `(02) 123-4567`, are kept.
//! The rule applies identically to every system.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use krpii_core::{Entity, EntityType};

use crate::corpus::{Label, Sample};
use crate::generators::DecoyKind;

/// A detection to be scored, produced by krpii, the baseline, or an external tool.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Prediction {
    /// `None` for types outside krpii's taxonomy, such as passport numbers
    /// reported by another tool. These can never be correct, so they always
    /// count as false positives.
    pub entity_type: Option<EntityType>,
    /// Inclusive start byte offset.
    pub start: usize,
    /// Exclusive end byte offset.
    pub end: usize,
}

impl From<Entity> for Prediction {
    fn from(entity: Entity) -> Self {
        Self {
            entity_type: Some(entity.entity_type),
            start: entity.start,
            end: entity.end,
        }
    }
}

/// Confusion counts for one entity type.
#[derive(Debug, Default, Clone, Copy)]
pub struct Counts {
    pub true_positives: usize,
    pub false_positives: usize,
    pub false_negatives: usize,
}

impl Counts {
    /// Number of gold entities of this type.
    pub fn support(&self) -> usize {
        self.true_positives + self.false_negatives
    }

    pub fn precision(&self) -> f64 {
        ratio(
            self.true_positives,
            self.true_positives + self.false_positives,
        )
    }

    pub fn recall(&self) -> f64 {
        ratio(self.true_positives, self.support())
    }

    /// Harmonic mean of precision and recall.
    pub fn f1(&self) -> f64 {
        let (p, r) = (self.precision(), self.recall());
        if p + r == 0.0 {
            0.0
        } else {
            2.0 * p * r / (p + r)
        }
    }

    fn add(&mut self, other: Self) {
        self.true_positives += other.true_positives;
        self.false_positives += other.false_positives;
        self.false_negatives += other.false_negatives;
    }
}

/// How often detections overlapped decoys of one kind.
#[derive(Debug, Default, Clone, Copy)]
pub struct DecoyCounts {
    pub total: usize,
    pub flagged: usize,
}

impl DecoyCounts {
    pub fn rate(&self) -> f64 {
        ratio(self.flagged, self.total)
    }
}

/// A single misclassification, kept for error analysis.
#[derive(Debug, Clone)]
pub struct ErrorExample {
    pub kind: ErrorKind,
    pub entity_type: EntityType,
    pub value: String,
    pub context: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorKind {
    FalsePositive,
    FalseNegative,
}

/// Aggregated benchmark results.
#[derive(Debug, Default)]
pub struct Report {
    pub per_type: BTreeMap<EntityType, Counts>,
    /// Detections whose type lies outside krpii's taxonomy.
    pub other_false_positives: usize,
    pub decoys: BTreeMap<DecoyKind, DecoyCounts>,
    pub errors: Vec<ErrorExample>,
    pub samples: usize,
    pub bytes: usize,
    pub detection_time: Duration,
}

impl Report {
    /// Micro-averaged counts across all entity types, including detections of
    /// types outside krpii's taxonomy as false positives.
    pub fn overall(&self) -> Counts {
        let mut total = Counts {
            false_positives: self.other_false_positives,
            ..Counts::default()
        };
        for counts in self.per_type.values() {
            total.add(*counts);
        }
        total
    }

    /// Decoy counts summed across all decoy kinds.
    pub fn decoys_overall(&self) -> DecoyCounts {
        self.decoys
            .values()
            .fold(DecoyCounts::default(), |total, counts| DecoyCounts {
                total: total.total + counts.total,
                flagged: total.flagged + counts.flagged,
            })
    }

    /// Detection throughput in mebibytes per second.
    #[allow(clippy::cast_precision_loss)]
    pub fn throughput_mib_per_sec(&self) -> f64 {
        let seconds = self.detection_time.as_secs_f64();
        if seconds == 0.0 {
            0.0
        } else {
            self.bytes as f64 / (1024.0 * 1024.0) / seconds
        }
    }
}

/// Runs `predict` over `samples` and scores the output.
///
/// `predict` receives each sample's index and text. Accepting a function
/// rather than a concrete detector lets one scoring implementation serve
/// krpii, the regex-only baseline, and precomputed external predictions, so
/// every system is judged by identical rules.
pub fn evaluate(
    mut predict: impl FnMut(usize, &str) -> Vec<Prediction>,
    samples: &[Sample],
) -> Report {
    let mut report = Report {
        per_type: EntityType::ALL
            .into_iter()
            .map(|t| (t, Counts::default()))
            .collect(),
        decoys: DecoyKind::ALL
            .into_iter()
            .map(|k| (k, DecoyCounts::default()))
            .collect(),
        samples: samples.len(),
        ..Report::default()
    };

    for (index, sample) in samples.iter().enumerate() {
        let started = Instant::now();
        let predictions = predict(index, &sample.text);
        report.detection_time += started.elapsed();
        let predictions: Vec<Prediction> = predictions
            .into_iter()
            .filter_map(|p| trim_span(&sample.text, p))
            .collect();
        report.bytes += sample.text.len();
        score_sample(sample, &predictions, &mut report);
    }
    report
}

fn score_sample(sample: &Sample, predictions: &[Prediction], report: &mut Report) {
    let gold: Vec<(EntityType, usize, usize)> = sample
        .spans
        .iter()
        .filter_map(|span| match span.label {
            Label::Pii(entity_type) => Some((entity_type, span.start, span.end)),
            Label::Decoy(_) => None,
        })
        .collect();
    let mut matched = vec![false; gold.len()];

    for prediction in predictions {
        let Some(entity_type) = prediction.entity_type else {
            report.other_false_positives += 1;
            continue;
        };
        let hit = gold.iter().position(|&(t, start, end)| {
            t == entity_type && start == prediction.start && end == prediction.end
        });
        let counts = report
            .per_type
            .get_mut(&entity_type)
            .expect("all types initialized");
        if let Some(index) = hit {
            matched[index] = true;
            counts.true_positives += 1;
        } else {
            counts.false_positives += 1;
            report.errors.push(ErrorExample {
                kind: ErrorKind::FalsePositive,
                entity_type,
                value: sample.text[prediction.start..prediction.end].to_owned(),
                context: sample.text.clone(),
            });
        }
    }

    for (&(entity_type, start, end), _) in gold.iter().zip(&matched).filter(|(_, m)| !**m) {
        report
            .per_type
            .get_mut(&entity_type)
            .expect("all types initialized")
            .false_negatives += 1;
        report.errors.push(ErrorExample {
            kind: ErrorKind::FalseNegative,
            entity_type,
            value: sample.text[start..end].to_owned(),
            context: sample.text.clone(),
        });
    }

    for span in &sample.spans {
        if let Label::Decoy(kind) = span.label {
            let counts = report.decoys.get_mut(&kind).expect("all kinds initialized");
            counts.total += 1;
            if predictions
                .iter()
                .any(|p| p.start < span.end && span.start < p.end)
            {
                counts.flagged += 1;
            }
        }
    }
}

/// Shrinks `prediction` past surrounding whitespace and unbalanced edge
/// brackets, or returns `None` if nothing remains.
fn trim_span(text: &str, prediction: Prediction) -> Option<Prediction> {
    const PAIRS: &[(char, char)] = &[('(', ')'), ('[', ']'), ('{', '}'), ('<', '>')];
    let (mut start, mut end) = (prediction.start, prediction.end);
    loop {
        let value = &text[start..end];
        let first = value.chars().next()?;
        let last = value.chars().next_back()?;
        let stray_opener = PAIRS
            .iter()
            .any(|&(open, close)| first == open && !value.contains(close));
        let stray_closer = PAIRS
            .iter()
            .any(|&(open, close)| last == close && !value.contains(open));
        if first.is_whitespace() || stray_opener {
            start += first.len_utf8();
        } else if last.is_whitespace() || stray_closer {
            end -= last.len_utf8();
        } else {
            return Some(Prediction {
                start,
                end,
                ..prediction
            });
        }
    }
}

#[allow(clippy::cast_precision_loss)]
fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[cfg(test)]
mod tests {
    use krpii_core::Detector;

    use super::*;
    use crate::corpus::Span;

    fn krpii(_index: usize, text: &str) -> Vec<Prediction> {
        Detector::new()
            .detect(text)
            .into_iter()
            .map(Prediction::from)
            .collect()
    }

    fn sample(text: &str, spans: Vec<Span>) -> Sample {
        Sample {
            text: text.to_owned(),
            spans,
        }
    }

    #[test]
    fn exact_match_is_a_true_positive() {
        let text = "연락처: 010-1234-5678";
        let start = text.find('0').unwrap();
        let samples = [sample(
            text,
            vec![Span {
                label: Label::Pii(EntityType::Phone),
                start,
                end: text.len(),
            }],
        )];
        let report = evaluate(krpii, &samples);
        let phone = report.per_type[&EntityType::Phone];
        assert_eq!(
            (
                phone.true_positives,
                phone.false_positives,
                phone.false_negatives
            ),
            (1, 0, 0)
        );
    }

    #[test]
    fn missed_entity_is_a_false_negative() {
        // Spaces around each hyphen are not a supported notation.
        let text = "010 - 1234 - 5678";
        let samples = [sample(
            text,
            vec![Span {
                label: Label::Pii(EntityType::Phone),
                start: 0,
                end: text.len(),
            }],
        )];
        let report = evaluate(krpii, &samples);
        assert_eq!(report.per_type[&EntityType::Phone].false_negatives, 1);
    }

    #[test]
    fn detection_on_decoy_is_flagged() {
        // Unlabelled, so nothing distinguishes it from a business registration number.
        let text = "참고 1234567891";
        let start = text.find('1').unwrap();
        let samples = [sample(
            text,
            vec![Span {
                label: Label::Decoy(DecoyKind::MemberId),
                start,
                end: text.len(),
            }],
        )];
        let report = evaluate(krpii, &samples);
        assert_eq!(report.decoys[&DecoyKind::MemberId].flagged, 1);
        assert_eq!(report.per_type[&EntityType::Brn].false_positives, 1);
    }

    #[test]
    fn unknown_type_counts_as_overall_false_positive() {
        let text = "여권 M12345678";
        let samples = [sample(text, Vec::new())];
        let report = evaluate(
            |_, _| {
                vec![Prediction {
                    entity_type: None,
                    start: 7,
                    end: text.len(),
                }]
            },
            &samples,
        );
        assert_eq!(report.other_false_positives, 1);
        assert_eq!(report.overall().false_positives, 1);
    }

    #[test]
    fn balanced_brackets_that_belong_to_the_value_are_kept() {
        let text = "연락처 (02) 123-4567";
        let start = text.find('(').unwrap();
        let samples = [sample(
            text,
            vec![Span {
                label: Label::Pii(EntityType::Phone),
                start,
                end: text.len(),
            }],
        )];
        let report = evaluate(krpii, &samples);
        assert_eq!(report.per_type[&EntityType::Phone].true_positives, 1);
        assert_eq!(report.per_type[&EntityType::Phone].false_positives, 0);
    }

    #[test]
    fn surrounding_brackets_do_not_turn_a_hit_into_a_miss() {
        let text = "확인 부탁드립니다 (010-1234-5678)";
        let number_start = text.find("010").unwrap();
        let samples = [sample(
            text,
            vec![Span {
                label: Label::Pii(EntityType::Phone),
                start: number_start,
                end: number_start + "010-1234-5678".len(),
            }],
        )];
        let report = evaluate(
            |_, _| {
                vec![Prediction {
                    entity_type: Some(EntityType::Phone),
                    start: number_start - 1,
                    end: number_start + "010-1234-5678".len(),
                }]
            },
            &samples,
        );
        assert_eq!(report.per_type[&EntityType::Phone].true_positives, 1);
    }

    #[test]
    fn f1_is_harmonic_mean() {
        let counts = Counts {
            true_positives: 8,
            false_positives: 2,
            false_negatives: 8,
        };
        // Precision 0.8, recall 0.5 -> F1 = 2 * 0.4 / 1.3.
        assert!((counts.f1() - 0.8 / 1.3).abs() < 1e-12);
    }
}
