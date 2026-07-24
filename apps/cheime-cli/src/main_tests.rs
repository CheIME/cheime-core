use super::{CliOptions, load_dict_directory, open_user_store};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

fn args(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

#[test]
fn options_require_dictionary_directory() {
    let error = CliOptions::parse_from(args(&[]), Path::new("data")).unwrap_err();

    assert!(error.contains("--dict"));
}

#[test]
fn options_use_default_log_under_data_directory() {
    let options = CliOptions::parse_from(args(&["--dict", "dicts"]), Path::new("state")).unwrap();

    assert_eq!(options.dictionary_dir, PathBuf::from("dicts"));
    assert_eq!(
        options.log,
        PathBuf::from("state").join("logs").join("cheime-cli.log")
    );
}

#[test]
fn options_accept_explicit_log_path() {
    let options = CliOptions::parse_from(
        args(&["--dict", "dicts", "--log", "diagnostics/demo.log"]),
        Path::new("state"),
    )
    .unwrap();

    assert_eq!(options.log, PathBuf::from("diagnostics/demo.log"));
}

#[test]
fn dictionaries_are_merged_from_supplied_directory() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(
        directory.path().join("base.dict.yaml"),
        "---\nname: base\n...\n你\tni\t100\n",
    )
    .unwrap();
    fs::write(
        directory.path().join("ext.dict.yaml"),
        "---\nname: ext\n...\n泥\tni\t20\n好\thao\t80\n",
    )
    .unwrap();
    fs::write(directory.path().join("README.md"), "ignored").unwrap();

    let index = load_dict_directory(directory.path()).unwrap();

    assert_eq!(index.total_entries(), 3);
    assert_eq!(index.query("ni")[0].text, "你");
    assert_eq!(index.query("hao")[0].text, "好");
}

#[test]
fn missing_dictionary_directory_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("missing");

    let error = load_dict_directory(&path).unwrap_err();

    assert!(error.contains(&path.display().to_string()));
}

#[test]
fn directory_without_dictionary_files_is_an_error() {
    let directory = tempfile::tempdir().unwrap();
    fs::write(directory.path().join("README.md"), "not a dictionary").unwrap();

    let error = load_dict_directory(directory.path()).unwrap_err();

    assert!(error.contains(".dict.yaml"));
}

#[test]
fn dictionary_with_windows_line_endings_is_loaded_from_directory() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("windows.dict.yaml");
    fs::write(&path, "---\r\nname: demo\r\n...\r\n你\tni\t100\r\n").unwrap();

    let index = load_dict_directory(directory.path()).unwrap();

    assert_eq!(index.total_entries(), 1);
    assert_eq!(index.query("ni")[0].text, "你");
}

#[test]
fn opening_user_store_creates_fresh_data_directory_and_persists() {
    let directory = tempfile::tempdir().unwrap();
    let data_dir = directory.path().join("fresh");

    {
        let mut store = open_user_store(&data_dir).unwrap();
        store.commit_pending("你", "ni", "quanpin");
        store.confirm_all_pending();
    }

    let store = open_user_store(&data_dir).unwrap();
    assert!(data_dir.join("cheime_cli_user.db").is_file());
    assert_eq!(store.query("ni")[0].text, "你");
}
