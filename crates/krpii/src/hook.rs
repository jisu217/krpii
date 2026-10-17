//! Claude Code hook adapter.
//!
//! `krpii hook` reads one hook event as JSON on standard input and, when the
//! event carries PII, prints a decision as JSON on standard output:
//!
//! - `UserPromptSubmit`: blocks the prompt. The original text is suppressed
//!   from the block message so the PII is not echoed back into the transcript.
//! - `PreToolUse` for the `Read` tool: denies the read and tells Claude how to
//!   read a masked copy instead, so the task can continue without the values.
//!
//! Events without PII, and events this adapter does not handle, produce no
//! output, which leaves the decision to Claude Code's normal flow.
//!
//! Reference: <https://code.claude.com/docs/en/hooks>

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use krpii_core::{Detector, Entity};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::describe;

/// Files larger than this are not scanned, so a hook never stalls a session on
/// a huge file. At the measured throughput this bounds a scan to about a second.
const MAX_SCANNED_BYTES: u64 = 16 * 1024 * 1024;

/// How much of a file is inspected for NUL bytes to recognize binary content.
const BINARY_SNIFF_BYTES: usize = 8 * 1024;

/// The subset of the hook input this adapter reads. Unknown fields are ignored,
/// so new fields added by Claude Code do not break the hook.
#[derive(Debug, Deserialize)]
struct HookInput {
    hook_event_name: String,
    #[serde(default)]
    prompt: Option<String>,
    #[serde(default)]
    tool_name: Option<String>,
    #[serde(default)]
    tool_input: Option<ToolInput>,
}

#[derive(Debug, Deserialize)]
struct ToolInput {
    #[serde(default)]
    file_path: Option<PathBuf>,
}

/// Returns the decision to print for the hook event in `input`, or `None` to
/// leave the event to Claude Code's normal flow.
pub fn decide(input: &str) -> Result<Option<Value>> {
    let event: HookInput = serde_json::from_str(input).context("invalid hook input")?;
    let detector = Detector::new();

    let decision = match event.hook_event_name.as_str() {
        "UserPromptSubmit" => event
            .prompt
            .map(|prompt| detector.detect(&prompt))
            .filter(|entities| !entities.is_empty())
            .map(|entities| block_prompt(&entities)),
        "PreToolUse" if event.tool_name.as_deref() == Some("Read") => event
            .tool_input
            .and_then(|tool_input| tool_input.file_path)
            .and_then(|path| scan_file(&detector, &path).map(|entities| (path, entities)))
            .filter(|(_, entities)| !entities.is_empty())
            .map(|(path, entities)| deny_read(&path, &entities)),
        _ => None,
    };
    Ok(decision)
}

fn block_prompt(entities: &[Entity]) -> Value {
    json!({
        "decision": "block",
        "reason": format!(
            "krpii: this prompt contains {}. Remove it, or mask it with \
             `krpii mask --style tag` and resend.",
            describe(entities)
        ),
        "suppressOriginalPrompt": true,
    })
}

fn deny_read(path: &Path, entities: &[Entity]) -> Value {
    json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "permissionDecision": "deny",
            "permissionDecisionReason": format!(
                "krpii: {} contains {}. Read a masked copy instead: krpii mask --style tag {}",
                path.display(),
                describe(entities),
                shell_quote(&path.to_string_lossy())
            ),
        }
    })
}

/// Scans the file at `path`, or returns `None` when it should not be scanned:
/// it is missing or unreadable (the read will fail on its own), too large, or
/// binary.
fn scan_file(detector: &Detector, path: &Path) -> Option<Vec<Entity>> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() > MAX_SCANNED_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    if bytes.iter().take(BINARY_SNIFF_BYTES).any(|&b| b == 0) {
        return None;
    }
    // Logs often contain a stray invalid byte; decode lossily rather than skip.
    Some(detector.detect(&String::from_utf8_lossy(&bytes)))
}

/// Quotes `value` for a POSIX shell when it contains anything beyond a
/// conservative set of safe characters.
fn shell_quote(value: &str) -> String {
    let safe = |c: char| c.is_ascii_alphanumeric() || "/._-+:@%".contains(c);
    if !value.is_empty() && value.chars().all(safe) {
        value.to_owned()
    } else {
        format!("'{}'", value.replace('\'', r"'\''"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prompt_event(prompt: &str) -> String {
        json!({ "hook_event_name": "UserPromptSubmit", "prompt": prompt }).to_string()
    }

    #[test]
    fn prompt_with_pii_is_blocked_without_echoing_it() {
        let decision = decide(&prompt_event("고객 010-1234-5678 환불"))
            .unwrap()
            .unwrap();
        assert_eq!(decision["decision"], "block");
        assert_eq!(decision["suppressOriginalPrompt"], true);
        let reason = decision["reason"].as_str().unwrap();
        assert!(reason.contains("1 value (1 PHONE)"));
        assert!(
            !reason.contains("1234"),
            "the reason must not echo the value"
        );
    }

    #[test]
    fn prompt_without_pii_passes() {
        assert_eq!(decide(&prompt_event("README를 요약해 줘")).unwrap(), None);
    }

    #[test]
    fn other_tools_and_events_are_ignored() {
        for input in [
            json!({ "hook_event_name": "PreToolUse", "tool_name": "Bash",
                    "tool_input": { "command": "echo 010-1234-5678" } }),
            json!({ "hook_event_name": "SessionStart" }),
        ] {
            assert_eq!(decide(&input.to_string()).unwrap(), None);
        }
    }

    #[test]
    fn read_of_a_missing_file_is_left_to_claude_code() {
        let input = json!({ "hook_event_name": "PreToolUse", "tool_name": "Read",
                            "tool_input": { "file_path": "/nonexistent/krpii-test.log" } });
        assert_eq!(decide(&input.to_string()).unwrap(), None);
    }

    #[test]
    fn malformed_input_is_an_error() {
        assert!(decide("not json").is_err());
    }

    #[test]
    fn shell_quote_leaves_safe_paths_and_quotes_the_rest() {
        assert_eq!(shell_quote("/tmp/app.log"), "/tmp/app.log");
        assert_eq!(shell_quote("/tmp/my logs/a.log"), "'/tmp/my logs/a.log'");
        assert_eq!(shell_quote("/tmp/it's.log"), r"'/tmp/it'\''s.log'");
    }
}
