//! Unit tests for autocomplete detection and suggestion filtering.

use super::*;
use crate::composer::slash::builtin_slash_commands;

#[test]
fn test_detect_slash_autocomplete() {
    let builtins = builtin_slash_commands();
    let sessions = vec![];

    // Starts with slash
    let res = detect_autocomplete("/co", &builtins, &sessions, None);
    assert!(res.is_some());
    let list = res.unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].label(), "compact");

    // Has spaces -> no slash autocomplete
    let res2 = detect_autocomplete("/compact hello", &builtins, &sessions, None);
    assert!(res2.is_none());
}

#[test]
fn test_detect_mention_autocomplete() {
    let builtins = builtin_slash_commands();
    let sessions = vec![("sess_123".into(), "Refactor parser".into())];

    let res = detect_autocomplete("Check this @ref", &builtins, &sessions, None);
    assert!(res.is_some());
    let list = res.unwrap();
    assert!(list.iter().any(|s| s.label() == "Refactor parser"));

    assert!(detect_autocomplete("Use $agent", &builtins, &sessions, None).is_none());
    assert!(detect_autocomplete("Try @browser", &builtins, &sessions, None).is_none());
}

#[test]
fn test_match_workspace_files() {
    let temp_dir = std::env::temp_dir().join(format!("zcode_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let test_file = temp_dir.join("sample.rs");
    let _ = std::fs::write(&test_file, "fn main() {}");

    let matches = match_workspace_files(&temp_dir, "sample", 5);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].1, "sample.rs");

    let _ = std::fs::remove_dir_all(&temp_dir);
}
