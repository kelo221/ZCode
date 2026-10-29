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
fn file_mentions_come_from_the_index_with_desktop_markdown() {
    let dir = std::env::temp_dir().join(format!("zcode_ac_{}", std::process::id()));
    std::fs::create_dir_all(dir.join("a")).unwrap();
    std::fs::write(dir.join("a").join("sample.rs"), "fn main() {}").unwrap();
    let builtins = builtin_slash_commands();
    let mut list = vec![];
    for _ in 0..200 {
        list = detect_autocomplete("@sample", &builtins, &[], Some(&dir)).unwrap_or_default();
        if list.iter().all(|s| !matches!(s, AutocompleteSuggestion::Mention { category, .. } if *category == INDEXING)) {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let file = list
        .iter()
        .find_map(|s| match s {
            AutocompleteSuggestion::Mention {
                category: "file",
                insert_text,
                ..
            } => Some(insert_text.clone()),
            _ => None,
        })
        .expect("file suggestion");
    assert_eq!(file, "[sample.rs](./a/sample.rs)");
    let _ = std::fs::remove_dir_all(&dir);
}
