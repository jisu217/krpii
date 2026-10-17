//! End-to-end tests that execute the compiled `krpii` binary.

use std::io::{ErrorKind, Write};
use std::process::{Command, Output, Stdio};

const SAMPLE: &str = "홍길동 010-1234-5678, 주민번호 900101-1234567로 가입\n";

fn run(args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_krpii"))
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn krpii");
    let mut pipe = child.stdin.take().expect("stdin is piped");
    // On a usage error the binary exits without reading its input, so writing
    // may hit a closed pipe. That is expected; any other failure is not.
    if let Err(error) = pipe.write_all(stdin.as_bytes()) {
        assert_eq!(
            error.kind(),
            ErrorKind::BrokenPipe,
            "failed to write stdin: {error}"
        );
    }
    drop(pipe);
    child.wait_with_output().expect("failed to wait for krpii")
}

fn stdout(output: &Output) -> &str {
    std::str::from_utf8(&output.stdout).expect("stdout is UTF-8")
}

#[test]
fn mask_rewrites_input_and_preserves_trailing_newline() {
    let output = run(&["mask"], SAMPLE);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(
        stdout(&output),
        "홍길동 010-****-5678, 주민번호 900101-*******로 가입\n"
    );
}

#[test]
fn mask_supports_tag_style_and_type_filter() {
    let output = run(&["mask", "--style", "tag", "--types", "rrn"], SAMPLE);
    assert_eq!(
        stdout(&output),
        "홍길동 010-1234-5678, 주민번호 <RRN>로 가입\n"
    );
}

#[test]
fn scan_reports_entities_without_values_and_exits_one() {
    let output = run(&["scan"], SAMPLE);
    assert_eq!(output.status.code(), Some(1));
    let out = stdout(&output);
    assert_eq!(out.lines().count(), 2);
    assert!(out.contains(r#""type":"PHONE""#));
    assert!(
        !out.contains("1234"),
        "scan output must not contain PII values"
    );
}

#[test]
fn scan_exits_zero_when_no_pii_is_found() {
    let output = run(&["scan"], "안녕하세요\n");
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout(&output), "");
}

#[test]
fn unknown_type_is_a_usage_error() {
    let output = run(&["scan", "--types", "passport"], "");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn summary_is_not_written_when_stderr_is_not_a_terminal() {
    // Tests capture stderr through a pipe, as an editor hook or script would.
    let output = run(&["mask"], SAMPLE);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
}

#[test]
fn quiet_flag_is_accepted() {
    let output = run(&["scan", "--quiet"], SAMPLE);
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn invalid_min_score_is_a_usage_error() {
    for value in ["1.5", "-0.1", "NaN", "high"] {
        let output = run(&["scan", "--min-score", value], SAMPLE);
        assert_eq!(output.status.code(), Some(2), "value: {value}");
    }
}

#[test]
fn usage_errors_do_not_depend_on_reading_input() {
    // The binary rejects the argument before reading standard input. A large
    // input guarantees it exits while the input is still being written, which
    // reproduces on every platform the race that made this flaky on Linux.
    let large_input = "가".repeat(1 << 20);
    let output = run(&["scan", "--min-score", "1.5"], &large_input);
    assert_eq!(output.status.code(), Some(2));
}

/// Writes `contents` to a file under Cargo's per-test temporary directory.
fn temp_file(name: &str, contents: &str) -> std::path::PathBuf {
    let path = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    std::fs::write(&path, contents).expect("failed to write temp file");
    path
}

fn read_event(path: &std::path::Path) -> String {
    format!(
        r#"{{"hook_event_name":"PreToolUse","tool_name":"Read","tool_input":{{"file_path":{}}}}}"#,
        serde_json_string(&path.to_string_lossy())
    )
}

fn serde_json_string(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

#[test]
fn hook_blocks_a_prompt_containing_pii() {
    let event = r#"{"hook_event_name":"UserPromptSubmit","prompt":"고객 010-1234-5678 환불"}"#;
    let output = run(&["hook"], event);
    assert_eq!(output.status.code(), Some(0));
    let out = stdout(&output);
    assert!(out.contains(r#""decision":"block""#), "stdout: {out}");
    assert!(
        out.contains(r#""suppressOriginalPrompt":true"#),
        "stdout: {out}"
    );
    assert!(
        !out.contains("1234"),
        "the decision must not echo the value"
    );
}

#[test]
fn hook_denies_reading_a_file_with_pii() {
    let path = temp_file("hook-pii.log", "user=hong phone=010-1234-5678\n");
    let output = run(&["hook"], &read_event(&path));
    assert_eq!(output.status.code(), Some(0));
    let out = stdout(&output);
    assert!(
        out.contains(r#""permissionDecision":"deny""#),
        "stdout: {out}"
    );
    assert!(out.contains("krpii mask --style tag"), "stdout: {out}");
}

#[test]
fn hook_allows_reading_a_file_without_pii() {
    let path = temp_file("hook-clean.log", "build finished in 3.2s\n");
    let output = run(&["hook"], &read_event(&path));
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(stdout(&output), "");
}

#[test]
fn hook_errors_do_not_block() {
    // Exit code 2 would make Claude Code block the prompt or tool call.
    let output = run(&["hook"], "not json");
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn hook_denies_reading_an_env_file_with_a_credential() {
    // Assembled at runtime so no complete credential literal lives in the source.
    let token = format!("ghp_{}", "a1B2c3D4e5".repeat(4).get(..36).unwrap());
    let path = temp_file("hook-secret.env", &format!("GITHUB_TOKEN={token}\n"));
    let output = run(&["hook"], &read_event(&path));
    assert_eq!(output.status.code(), Some(0));
    let out = stdout(&output);
    assert!(
        out.contains(r#""permissionDecision":"deny""#),
        "stdout: {out}"
    );
    assert!(out.contains("1 SECRET"), "stdout: {out}");
}
