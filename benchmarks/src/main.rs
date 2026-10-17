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
//! ```console
//! $ cargo run --release -p krpii-bench
//! $ cargo run --release -p krpii-bench -- --samples 20000 --show-errors 5
//! ```

mod baseline;
mod corpus;
mod generators;
mod metrics;
mod rng;

use std::fmt::Write as _;

use clap::Parser;
use krpii_core::{Detector, EntityType};

use crate::metrics::{Counts, ErrorKind, Report};

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
}

fn main() {
    let args = Args::parse();
    let samples = corpus::generate(args.seed, args.samples);
    let detector = Detector::new();
    let krpii = metrics::evaluate(|text| detector.detect(text), &samples);
    let baseline = metrics::evaluate(baseline::detect, &samples);

    print!("{}", render_evaluation(&krpii, &args));
    print!("{}", render_comparison(&krpii, &baseline));
    if args.show_errors > 0 {
        print!("{}", render_errors(&krpii, args.show_errors));
    }
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

fn render_comparison(krpii: &Report, baseline: &Report) -> String {
    let mut out = String::from("\n## Comparison with a regex-only baseline\n\n");
    out.push_str(
        "| Entity | Precision (regex) | Precision (krpii) | Recall (regex) | Recall (krpii) |\n",
    );
    out.push_str("|---|--:|--:|--:|--:|\n");
    let rows = krpii
        .per_type
        .iter()
        .map(|(t, k)| (format!("`{t}`"), baseline.per_type[t], *k))
        .chain(std::iter::once((
            "**Overall (micro)**".to_owned(),
            baseline.overall(),
            krpii.overall(),
        )));
    for (label, regex, ours) in rows {
        let _ = writeln!(out, "| {label} | {} |", comparison_cells(regex, ours));
    }

    let (regex_decoys, krpii_decoys) = (baseline.decoys_overall(), krpii.decoys_overall());
    let _ = writeln!(
        out,
        "\nDecoys flagged as PII: regex {} of {} ({:.1}%), krpii {} of {} ({:.1}%).",
        regex_decoys.flagged,
        regex_decoys.total,
        regex_decoys.rate() * 100.0,
        krpii_decoys.flagged,
        krpii_decoys.total,
        krpii_decoys.rate() * 100.0
    );
    out
}

fn comparison_cells(regex: Counts, krpii: Counts) -> String {
    format!(
        "{:.3} | {:.3} | {:.3} | {:.3}",
        regex.precision(),
        krpii.precision(),
        regex.recall(),
        krpii.recall()
    )
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
