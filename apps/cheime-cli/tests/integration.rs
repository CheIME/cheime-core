//! Integration tests exercising the `cheime` binary via JSON I/O.
//!
//! These tests spawn the binary in `--json` mode, feed it key events,
//! and verify the structured output matches expected candidates.
//!
//! CheIME advantage: full-black-box testing via the JSON I/O contract.

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
        .filter(|l| l.contains("CandidateSnapshot"))
        .last()
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
        .filter(|l| l.contains("SetPreedit"))
        .last()
        .expect("no SetPreedit found");
    assert!(
        last_set_preedit.contains(r#""text":"ni""#),
        "expected text 'ni': {last_set_preedit}"
    );
}
