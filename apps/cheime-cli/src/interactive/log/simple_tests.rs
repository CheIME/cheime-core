use super::{RunLog, parent_to_create};
use std::fs;
use std::path::Path;

#[test]
fn run_log_appends_plain_text_lines_to_selected_path() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("nested").join("demo.log");

    {
        let mut log = RunLog::open(&path).unwrap();
        log.append("session started").unwrap();
        log.append("engine candidate_snapshot").unwrap();
    }

    assert_eq!(
        fs::read_to_string(path).unwrap(),
        "session started\nengine candidate_snapshot\n"
    );
}

#[test]
fn bare_log_file_does_not_try_to_create_an_empty_parent() {
    assert_eq!(parent_to_create(Path::new("demo.log")), None);
}

#[test]
fn reopening_log_appends_to_existing_content() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("demo.log");

    RunLog::open(&path).unwrap().append("first run").unwrap();
    RunLog::open(&path).unwrap().append("second run").unwrap();

    assert_eq!(fs::read_to_string(path).unwrap(), "first run\nsecond run\n");
}
