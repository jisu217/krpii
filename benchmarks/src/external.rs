//! Exchange format for evaluating detectors that run outside this process.
//!
//! The benchmark exports its corpus as JSON Lines, an external tool (for
//! example Presidio, see `benchmarks/presidio/`) writes its detections in the
//! format below, and the benchmark scores them with the same code it uses for
//! krpii. A single scoring implementation guarantees identical rules for every
//! system.
//!
//! Corpus line: `{"id": 0, "text": "..."}`
//!
//! Prediction line: `{"id": 0, "entities": [{"type": "PHONE", "start": 10, "end": 23}]}`
//!
//! Offsets are UTF-8 byte offsets. `type` is one of krpii's entity labels;
//! any other label is accepted and scored as a type krpii does not support.

use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result, bail, ensure};
use krpii_core::EntityType;
use serde::{Deserialize, Serialize};

use crate::corpus::Sample;
use crate::metrics::Prediction;

#[derive(Serialize)]
struct CorpusLine<'a> {
    id: usize,
    text: &'a str,
}

#[derive(Deserialize)]
struct PredictionLine {
    id: usize,
    entities: Vec<ExternalEntity>,
}

#[derive(Deserialize)]
struct ExternalEntity {
    #[serde(rename = "type")]
    entity_type: String,
    start: usize,
    end: usize,
}

/// Writes `samples` to `path` as JSON Lines, without ground-truth labels.
pub fn export_corpus(samples: &[Sample], path: &Path) -> Result<()> {
    let file =
        File::create(path).with_context(|| format!("failed to create {}", path.display()))?;
    let mut out = BufWriter::new(file);
    for (id, sample) in samples.iter().enumerate() {
        serde_json::to_writer(
            &mut out,
            &CorpusLine {
                id,
                text: &sample.text,
            },
        )?;
        out.write_all(b"\n")?;
    }
    out.flush()?;
    Ok(())
}

/// Reads external predictions for `samples` from `path`, indexed by sample id.
///
/// Every sample must have exactly one line, and every span must lie on
/// character boundaries of its sample, so that misaligned offsets are caught
/// instead of silently lowering the external tool's score.
pub fn load_predictions(samples: &[Sample], path: &Path) -> Result<Vec<Vec<Prediction>>> {
    let file = File::open(path).with_context(|| format!("failed to open {}", path.display()))?;
    let mut predictions: Vec<Option<Vec<Prediction>>> = vec![None; samples.len()];

    for (line_number, line) in BufReader::new(file).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: PredictionLine = serde_json::from_str(&line)
            .with_context(|| format!("{}:{}: invalid JSON", path.display(), line_number + 1))?;
        let Some(text) = samples.get(parsed.id).map(|s| s.text.as_str()) else {
            bail!("{}: unknown sample id {}", path.display(), parsed.id);
        };
        let slot = &mut predictions[parsed.id];
        ensure!(
            slot.is_none(),
            "{}: duplicate sample id {}",
            path.display(),
            parsed.id
        );

        let mut entities = Vec::with_capacity(parsed.entities.len());
        for entity in parsed.entities {
            ensure!(
                entity.start < entity.end
                    && text.is_char_boundary(entity.start)
                    && text.is_char_boundary(entity.end),
                "{}: sample {} has a span {}..{} that is empty or not on UTF-8 boundaries",
                path.display(),
                parsed.id,
                entity.start,
                entity.end
            );
            entities.push(Prediction {
                entity_type: entity.entity_type.parse::<EntityType>().ok(),
                start: entity.start,
                end: entity.end,
            });
        }
        *slot = Some(entities);
    }

    predictions
        .into_iter()
        .enumerate()
        .map(|(id, entities)| {
            entities.with_context(|| format!("{}: missing sample id {id}", path.display()))
        })
        .collect()
}
