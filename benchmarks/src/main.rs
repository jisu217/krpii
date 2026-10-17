//! Evaluation benchmark for krpii.
//!
//! Generates a deterministic synthetic corpus of Korean text containing PII
//! and decoys, runs krpii and a regex-only baseline over it, and prints
//! per-entity precision, recall, and F1, decoy false-positive rates, and
//! throughput as Markdown suitable for the project README.
//!
//! The corpus is regenerated from a seed on every run rather than stored, so
//! the repository never ships thousands of checksum-valid identifiers.
//!
//! External detectors are compared by exporting the corpus, running the tool
//! on it, and scoring its predictions here (see [`external`]).
//!
//! ```console
//! $ cargo run --release -p krpii-bench
//! $ cargo run --release -p krpii-bench -- --samples 20000 --show-errors 5
//! $ cargo run --release -p krpii-bench -- --export-corpus target/corpus.jsonl
//! $ cargo run --release -p krpii-bench -- --predictions "Presidio=target/presidio.jsonl"
//! ```

mod baseline;
mod corpus;
mod external;
mod generators;
mod metrics;
mod rng;

use std::fmt::Write as _;
use std::path::PathBuf;

use anyhow::Result;
use clap::Parser;
use krpii_core::{Detector, EntityType};

use crate::metrics::{ErrorKind, Prediction, Report};

/// Benchmark krpii on a synthetic Korean corpus.
#[derive(Debug, Parser)]
#[command(name = "krpii-bench", about, long_about = None)]
struct Args {
    /// Seed for corpus generation; the same seed always yields the same corpus.
    #[arg(long, default_value_t = 42)]
    seed: u64,

    /// Number of samples to generate.
    #[arg(long, default_value_t = 10_000)]
    samples: usize,

    /// Print up to this many krpii false positives and false negatives per entity type.
    #[arg(long, default_value_t = 0, value_name = "N")]
    show_errors: usize,

    /// Write the corpus (without labels) as JSON Lines to this path and exit.
    #[arg(long, value_name = "PATH")]
    export_corpus: Option<PathBuf>,

    /// Score an external tool's predictions; may be repeated.
    #[arg(long, value_name = "NAME=PATH", value_parser = parse_named_path)]
    predictions: Vec<NamedPath>,
}

/// A display name paired with a file path, parsed from `NAME=PATH`.
#[derive(Debug, Clone)]
struct NamedPath {
    name: String,
    path: PathBuf,
}

fn parse_named_path(value: &str) -> Result<NamedPath, String> {
    match value.split_once('=') {
        Some((name, path)) if !name.is_empty() && !path.is_empty() => Ok(NamedPath {
            name: name.to_owned(),
            path: PathBuf::from(path),
        }),
        _ => Err(format!("expected NAME=PATH, got {value:?}")),
    }
}

fn main() -> Result<()> {
    let args = Args::parse();
    let samples = corpus::generate(args.seed, args.samples);

    if let Some(path) = &args.export_corpus {
        external::export_corpus(&samples, path)?;
        eprintln!("Wrote {} samples to {}", samples.len(), path.display());
        return Ok(());
    }

    let detector = Detector::new();
    let krpii = metrics::evaluate(|_, text| to_predictions(detector.detect(text)), &samples);
    let baseline = metrics::evaluate(|_, text| to_predictions(baseline::detect(text)), &samples);

    let mut externals = Vec::with_capacity(args.predictions.len());
    for NamedPath { name, path } in &args.predictions {
        let predictions = external::load_predictions(&samples, path)?;
        let report = metrics::evaluate(|index, _| predictions[index].clone(), &samples);
        externals.push((name.as_str(), report));
    }

    let mut systems: Vec<(&str, &Report)> = vec![("Regex only", &baseline)];
    systems.extend(externals.iter().map(|(name, report)| (*name, report)));
    systems.push(("krpii", &krpii));

    print!("{}", render_evaluation(&krpii, &args));
    print!("{}", render_comparison(&systems));
    if args.show_errors > 0 {
        print!("{}", render_errors(&krpii, args.show_errors));
    }
    Ok(())
}

fn to_predictions(entities: Vec<krpii_core::Entity>) -> Vec<Prediction> {
    entities.into_iter().map(Prediction::from).collect()
}

fn render_evaluation(report: &Report, args: &Args) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Seed {}, {} samples, exact span and type match.\n",
        args.seed, report.samples
    );

    out.push_str("| Entity | Support | Precision | Recall | F1 |\n");
    out.push_str("|---|--:|--:|--:|--:|\n");
    for (entity_type, counts) in &report.per_type {
        let _ = writeln!(
            out,
            "| `{entity_type}` | {} | {:.3} | {:.3} | {:.3} |",
            counts.support(),
            counts.precision(),
            counts.recall(),
            counts.f1()
        );
    }
    let overall = report.overall();
    let _ = writeln!(
        out,
        "| **Overall (micro)** | {} | {:.3} | {:.3} | {:.3} |\n",
        overall.support(),
        overall.precision(),
        overall.recall(),
        overall.f1()
    );

    out.push_str("| Decoy | Samples | Flagged | False-positive rate |\n");
    out.push_str("|---|--:|--:|--:|\n");
    for (kind, counts) in &report.decoys {
        let _ = writeln!(
            out,
            "| {} | {} | {} | {:.1}% |",
            kind.label(),
            counts.total,
            counts.flagged,
            counts.rate() * 100.0
        );
    }

    let _ = writeln!(
        out,
        "\nThroughput: {:.1} MiB/s ({} bytes in {:.1} ms).",
        report.throughput_mib_per_sec(),
        report.bytes,
        report.detection_time.as_secs_f64() * 1_000.0
    );
    if cfg!(debug_assertions) {
        out.push_str("Note: debug build; run with --release for meaningful throughput.\n");
    }
    out
}

fn render_comparison(systems: &[(&str, &Report)]) -> String {
    let mut out = String::from("\n## Comparison\n\n");
    // Credentials are excluded: Presidio has no credential recognizers, so
    // including them would penalize it for a task it does not attempt.
    out.push_str("Korean PII types only; credentials are reported in the krpii table above.\n\n");
    out.push_str("| System | Precision | Recall | F1 | Decoys flagged as PII |\n");
    out.push_str("|---|--:|--:|--:|--:|\n");
    for (name, report) in systems {
        let overall = report.overall_for(&EntityType::PII);
        let _ = writeln!(
            out,
            "| {name} | {:.3} | {:.3} | {:.3} | {:.1}% |",
            overall.precision(),
            overall.recall(),
            overall.f1(),
            report.decoys_overall().rate() * 100.0
        );
    }

    out.push_str("\nPer entity, precision / recall:\n\n| Entity |");
    for (name, _) in systems {
        let _ = write!(out, " {name} |");
    }
    out.push_str("\n|---|");
    out.push_str(&"--:|".repeat(systems.len()));
    out.push('\n');
    for entity_type in EntityType::PII {
        let _ = write!(out, "| `{entity_type}` |");
        for (_, report) in systems {
            let counts = report.per_type[&entity_type];
            let _ = write!(out, " {:.3} / {:.3} |", counts.precision(), counts.recall());
        }
        out.push('\n');
    }

    for (name, report) in systems {
        if report.other_false_positives > 0 {
            let _ = writeln!(
                out,
                "\n{name} also reported {} detections of types outside krpii's taxonomy, \
                 counted as false positives in its overall precision.",
                report.other_false_positives
            );
        }
    }
    out
}

fn render_errors(report: &Report, limit: usize) -> String {
    let mut out = String::from("\n## Errors\n");
    for entity_type in EntityType::ALL {
        for kind in [ErrorKind::FalseNegative, ErrorKind::FalsePositive] {
            let examples: Vec<_> = report
                .errors
                .iter()
                .filter(|e| e.entity_type == entity_type && e.kind == kind)
                .take(limit)
                .collect();
            if examples.is_empty() {
                continue;
            }
            let heading = match kind {
                ErrorKind::FalseNegative => "missed",
                ErrorKind::FalsePositive => "false positives",
            };
            let _ = writeln!(out, "\n### `{entity_type}` {heading}");
            for example in examples {
                let _ = writeln!(out, "- `{}` in \"{}\"", example.value, example.context);
            }
        }
    }
    out
}
