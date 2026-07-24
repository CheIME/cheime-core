//! Integration tests exercising the `cheime` binary via JSON I/O.
//!
//! These tests spawn the binary in `--json` mode, feed it key events,
//! and verify the structured output matches expected candidates.
//!
//! CheIME advantage: full-black-box testing via the JSON I/O contract.

use cheime_protocol::EngineMessage;
use std::io::Write;
use std::process::{Command, Stdio};

/// Run `cheime --json` with the given key event JSON lines, return stdout lines.
fn run_json(keys: &[&str]) -> Vec<String> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cheime"))
        .arg("--json")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn cheime");

    {
        let mut stdin = child.stdin.take().unwrap();
        for k in keys {
            writeln!(stdin, "{k}").unwrap();
        }
    }

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().map(|l| l.to_owned()).collect()
}

/// Run `cheime --json`, capturing both stdout and stderr.
/// Returns (stdout_lines, stderr_lines, exit_success).
fn run_json_full(keys: &[&str]) -> (Vec<String>, Vec<String>, bool) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_cheime"))
        .arg("--json")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn cheime");

    {
        let mut stdin = child.stdin.take().unwrap();
        for k in keys {
            writeln!(stdin, "{k}").unwrap();
        }
    }

    let output = child.wait_with_output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    (
        stdout.lines().map(|l| l.to_owned()).collect(),
        stderr.lines().map(|l| l.to_owned()).collect(),
        output.status.success(),
    )
}

fn key_char(c: char) -> String {
    format!(
        r#"{{"key":{{"Character":"{c}"}},"state":{{"shift":false,"control":false,"alt":false}}}}"#
    )
}

#[test]
fn nihao_produces_ni_hao_candidates() {
    let keys: Vec<String> = "nihao".chars().map(key_char).collect();
    let keys_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let lines = run_json(&keys_refs);

    // Find the last CandidateSnapshot
    let last_snapshot = lines
        .iter()
        .rfind(|l| l.contains("CandidateSnapshot"))
        .expect("no CandidateSnapshot found");

    // It must contain 你好 and 拟好
    assert!(
        last_snapshot.contains("你好"),
        "snapshot missing 你好: {last_snapshot}"
    );
    assert!(
        last_snapshot.contains("拟好"),
        "snapshot missing 拟好: {last_snapshot}"
    );
}

#[test]
fn enter_commits_highlighted() {
    let mut keys: Vec<String> = "nihao".chars().map(key_char).collect();
    keys.push(r#"{"key":"Enter","state":{"shift":false,"control":false,"alt":false}}"#.to_owned());
    let keys_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let lines = run_json(&keys_refs);

    let has_commit = lines.iter().any(|l| l.contains(r#""Commit""#));
    assert!(has_commit, "no Commit action found in output");
}

#[test]
fn backspace_removes_last_char() {
    let keys = [
        key_char('n'),
        key_char('i'),
        key_char('h'),
        r#"{"key":"Backspace","state":{"shift":false,"control":false,"alt":false}}"#.to_owned(),
    ];
    let keys_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let lines = run_json(&keys_refs);

    // The last PlatformAction should have preedit "ni"
    let last_set_preedit = lines
        .iter()
        .rfind(|l| l.contains("SetPreedit"))
        .expect("no SetPreedit found");
    assert!(
        last_set_preedit.contains(r#""text":"ni""#),
        "expected text 'ni': {last_set_preedit}"
    );
}

#[test]
fn json_mode_emits_only_compact_engine_messages_on_stdout() {
    let keys: Vec<String> = vec![key_char('n')];
    let keys_refs: Vec<&str> = keys.iter().map(|s| s.as_str()).collect();
    let (stdout_lines, stderr_lines, success) = run_json_full(&keys_refs);

    assert!(success, "process should exit successfully");
    assert!(
        !stderr_lines.is_empty(),
        "stderr should contain startup banners"
    );
    assert!(
        stderr_lines
            .iter()
            .any(|l| l.contains("[cheime] JSON mode")),
        "stderr should contain startup banner marker"
    );
    assert!(
        !stderr_lines.iter().any(|l| l.contains('\x1b')),
        "stderr must not contain terminal escapes"
    );

    let mut snapshot_count = 0;
    for line in &stdout_lines {
        assert!(
            !line.contains('\x1b'),
            "stdout must not contain terminal escapes: {line:?}"
        );
        let msg: EngineMessage = serde_json::from_str(line).unwrap_or_else(|e| {
            panic!("stdout line must be valid EngineMessage JSON: {e} in {line:?}")
        });
        let canonical =
            serde_json::to_string(&msg).expect("canonical serialization should not fail");
        assert_eq!(
            *line, canonical,
            "stdout line must be canonical compact JSON, no surrounding whitespace"
        );
        if matches!(msg, EngineMessage::CandidateSnapshot { .. }) {
            snapshot_count += 1;
        }
    }
    assert!(
        snapshot_count > 0,
        "must produce at least one CandidateSnapshot"
    );
}
