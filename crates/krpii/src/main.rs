//! `krpii` command-line interface.
//!
//! Exit codes follow the `grep` convention so the binary composes with shell
//! scripts, CI jobs, and editor hooks:
//!
//! | Code | Meaning                                        |
//! |------|------------------------------------------------|
//! | 0    | Success (`scan`: no PII found)                 |
//! | 1    | `scan` only: at least one entity was found     |
//! | 2    | Usage or I/O error                             |
//!
//! When PII is found and standard error is a terminal, a one-line summary such
//! as `krpii: masked 2 values (1 PHONE, 1 RRN)` is written to standard error.
//! It never reaches standard output, so redirected or piped results stay clean,
//! and it is omitted entirely when the caller is another program.

use std::fs;
use std::io::{self, BufWriter, IsTerminal, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use krpii_core::{Detector, Entity, EntityType, MaskStyle, apply_mask};
use serde::Serialize;

/// Detect and mask Korean personally identifiable information.
#[derive(Debug, Parser)]
#[command(name = "krpii", version, about, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Report detected entities as JSON Lines without printing their values.
    Scan(ScanArgs),
    /// Print the input with every detected entity masked.
    Mask(MaskArgs),
}

/// Options shared by every subcommand that runs detection.
#[derive(Debug, Args)]
struct DetectionArgs {
    /// File to read; reads standard input when omitted or set to "-".
    input: Option<PathBuf>,

    /// Comma-separated entity types to detect (default: all).
    #[arg(long, short, value_delimiter = ',', value_name = "TYPES")]
    types: Vec<EntityType>,

    /// Ignore entities whose confidence is below this threshold, from 0 to 1.
    #[arg(long, default_value_t = 0.0, value_name = "SCORE", value_parser = parse_score)]
    min_score: f64,

    /// Do not print the summary line in an interactive terminal.
    #[arg(long, short)]
    quiet: bool,
}

#[derive(Debug, Args)]
struct ScanArgs {
    #[command(flatten)]
    detection: DetectionArgs,
}

#[derive(Debug, Args)]
struct MaskArgs {
    #[command(flatten)]
    detection: DetectionArgs,

    /// Masking style: "partial" keeps the format, "tag" substitutes a type label.
    #[arg(long, short, default_value_t = MaskStyle::Partial, value_name = "STYLE")]
    style: MaskStyle,
}

/// Serialized form of an [`Entity`]. The matched value is intentionally omitted.
#[derive(Debug, Serialize)]
struct EntityRecord {
    #[serde(rename = "type")]
    entity_type: &'static str,
    /// UTF-8 byte offset, inclusive.
    start: usize,
    /// UTF-8 byte offset, exclusive.
    end: usize,
    score: f64,
}

impl From<&Entity> for EntityRecord {
    fn from(entity: &Entity) -> Self {
        Self {
            entity_type: entity.entity_type.as_str(),
            start: entity.start,
            end: entity.end,
            score: entity.score,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command) {
        Ok(code) => code,
        Err(error) => {
            eprintln!("krpii: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn run(command: Command) -> Result<ExitCode> {
    match command {
        Command::Scan(args) => scan(&args),
        Command::Mask(args) => mask(&args),
    }
}

fn scan(args: &ScanArgs) -> Result<ExitCode> {
    let text = read_input(args.detection.input.as_ref())?;
    let entities = build_detector(&args.detection).detect(&text);

    let mut out = BufWriter::new(io::stdout().lock());
    for entity in &entities {
        serde_json::to_writer(&mut out, &EntityRecord::from(entity))?;
        out.write_all(b"\n")?;
    }
    out.flush()?;
    report_summary(&args.detection, "found", &entities);

    Ok(if entities.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    })
}

fn mask(args: &MaskArgs) -> Result<ExitCode> {
    let text = read_input(args.detection.input.as_ref())?;
    let entities = build_detector(&args.detection).detect(&text);
    let masked = apply_mask(&text, &entities, args.style);

    let mut out = io::stdout().lock();
    out.write_all(masked.as_bytes())?;
    out.flush()?;
    report_summary(&args.detection, "masked", &entities);
    Ok(ExitCode::SUCCESS)
}

/// Writes the summary line to standard error when a person is watching.
fn report_summary(args: &DetectionArgs, verb: &str, entities: &[Entity]) {
    let stderr = io::stderr();
    if args.quiet || !stderr.is_terminal() {
        return;
    }
    let Some(line) = summary(verb, entities) else {
        return;
    };
    // Dimmed so the summary reads as secondary to the output itself; the
    // NO_COLOR convention (https://no-color.org) disables the styling.
    let colored = std::env::var_os("NO_COLOR").is_none_or(|value| value.is_empty());
    let mut stderr = stderr.lock();
    let _ = if colored {
        writeln!(stderr, "\x1b[2m{line}\x1b[0m")
    } else {
        writeln!(stderr, "{line}")
    };
}

/// Formats a summary such as `krpii: masked 2 values (1 PHONE, 1 RRN)`, or
/// returns `None` when nothing was found.
fn summary(verb: &str, entities: &[Entity]) -> Option<String> {
    if entities.is_empty() {
        return None;
    }
    let breakdown: Vec<String> = EntityType::ALL
        .into_iter()
        .filter_map(|entity_type| {
            let count = entities
                .iter()
                .filter(|e| e.entity_type == entity_type)
                .count();
            (count > 0).then(|| format!("{count} {entity_type}"))
        })
        .collect();
    let noun = if entities.len() == 1 {
        "value"
    } else {
        "values"
    };
    Some(format!(
        "krpii: {verb} {} {noun} ({})",
        entities.len(),
        breakdown.join(", ")
    ))
}

/// Parses a confidence threshold, rejecting values that no score can satisfy
/// or compare against, such as `1.5` or `NaN`.
fn parse_score(value: &str) -> Result<f64, String> {
    let score: f64 = value
        .parse()
        .map_err(|_| format!("{value:?} is not a number"))?;
    if (0.0..=1.0).contains(&score) {
        Ok(score)
    } else {
        Err(format!("{value} is outside the range 0 to 1"))
    }
}

fn build_detector(args: &DetectionArgs) -> Detector {
    let detector = Detector::new().with_min_score(args.min_score);
    if args.types.is_empty() {
        detector
    } else {
        detector.with_types(args.types.iter().copied())
    }
}

fn read_input(path: Option<&PathBuf>) -> Result<String> {
    match path {
        Some(path) if path.as_os_str() != "-" => {
            fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
        }
        _ => {
            let mut text = String::new();
            io::stdin()
                .read_to_string(&mut text)
                .context("failed to read standard input")?;
            Ok(text)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entity(entity_type: EntityType) -> Entity {
        Entity {
            entity_type,
            start: 0,
            end: 1,
            score: 1.0,
        }
    }

    #[test]
    fn summary_is_absent_when_nothing_was_found() {
        assert_eq!(summary("masked", &[]), None);
    }

    #[test]
    fn summary_uses_singular_for_one_value() {
        assert_eq!(
            summary("found", &[entity(EntityType::Phone)]).as_deref(),
            Some("krpii: found 1 value (1 PHONE)")
        );
    }

    #[test]
    fn summary_breaks_down_types_in_canonical_order() {
        let entities = [
            entity(EntityType::Rrn),
            entity(EntityType::Phone),
            entity(EntityType::Phone),
        ];
        assert_eq!(
            summary("masked", &entities).as_deref(),
            Some("krpii: masked 3 values (2 PHONE, 1 RRN)")
        );
    }
}
