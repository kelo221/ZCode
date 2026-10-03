use super::*;

#[test]
fn test_filter_slash_commands() {
    let catalog = builtin_slash_commands();

    // Bare slash matches all
    let matches = filter_slash_commands(&catalog, "/").unwrap();
    assert_eq!(matches.len(), catalog.len());

    // Prefix matches
    let matches = filter_slash_commands(&catalog, "/comp").unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "compact");

    // Case insensitive
    let matches = filter_slash_commands(&catalog, "/G").unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].name, "goal");

    // Space closes autocomplete
    assert!(filter_slash_commands(&catalog, "/goal ").is_none());
    assert!(filter_slash_commands(&catalog, "not a slash").is_none());
}

#[test]
fn test_classify_slash_command() {
    assert_eq!(classify_slash_command("/compact"), SlashAction::Compact);
    assert_eq!(
        classify_slash_command("  /compress  "),
        SlashAction::Compact
    );
    assert_eq!(
        classify_slash_command("/goal Finish task A"),
        SlashAction::Goal("Finish task A".into())
    );
    assert_eq!(
        classify_slash_command("/target Reach 100%"),
        SlashAction::Goal("Reach 100%".into())
    );
    assert_eq!(
        classify_slash_command("hello world"),
        SlashAction::Plain("hello world".into())
    );
    assert_eq!(
        classify_slash_command("/custom arg"),
        SlashAction::Plain("/custom arg".into())
    );
}
