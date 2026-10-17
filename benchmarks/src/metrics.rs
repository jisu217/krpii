//! Scoring detector output against ground-truth annotations.
//!
//! A prediction counts as a true positive only when its type, start, and end
//! all match a gold span exactly. A correct span with the wrong type therefore
//! counts as both a false positive and a false negative. This is the strictest
//! common criterion, so reported numbers are a lower bound.

use std::collections::BTreeMap;
use std::time::{Duration, Instant};

use krpii_core::{Entity, EntityType};

use crate::corpus::{Label, Sample};
use crate::generators::DecoyKind;

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
    pub decoys: BTreeMap<DecoyKind, DecoyCounts>,
    pub errors: Vec<ErrorExample>,
    pub samples: usize,
    pub bytes: usize,
    pub detection_time: Duration,
}

impl Report {
    /// Micro-averaged counts across all entity types.
    pub fn overall(&self) -> Counts {
        let mut total = Counts::default();
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

/// Runs `detect` over `samples` and scores the output.
///
/// Accepting a function rather than a concrete detector lets the same scoring
/// run against krpii and the regex-only baseline.
pub fn evaluate(detect: impl Fn(&str) -> Vec<Entity>, samples: &[Sample]) -> Report {
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

    for sample in samples {
        let started = Instant::now();
        let predictions = detect(&sample.text);
        report.detection_time += started.elapsed();
        report.bytes += sample.text.len();
        score_sample(sample, &predictions, &mut report);
    }
    report
}

fn score_sample(sample: &Sample, predictions: &[Entity], report: &mut Report) {
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
        let hit = gold.iter().position(|&(t, start, end)| {
            t == prediction.entity_type && start == prediction.start && end == prediction.end
        });
        let counts = report
            .per_type
            .get_mut(&prediction.entity_type)
            .expect("all types initialized");
        if let Some(index) = hit {
            matched[index] = true;
            counts.true_positives += 1;
        } else {
            counts.false_positives += 1;
            report.errors.push(ErrorExample {
                kind: ErrorKind::FalsePositive,
                entity_type: prediction.entity_type,
                value: prediction.as_str(&sample.text).to_owned(),
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

    fn krpii(text: &str) -> Vec<Entity> {
        Detector::new().detect(text)
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
        let text = "(02) 123-4567";
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
        let text = "회원번호 1234567891";
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
