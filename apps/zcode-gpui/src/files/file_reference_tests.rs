use super::locate;

fn workspace() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("zcode-file-ref-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(root.join("tmp/plan")).unwrap();
    std::fs::create_dir_all(root.join("Assets/Scripts")).unwrap();
    std::fs::create_dir_all(root.join("a")).unwrap();
    std::fs::create_dir_all(root.join("b")).unwrap();
    std::fs::write(root.join("tmp/plan/PLAN.md"), "plan").unwrap();
    std::fs::write(root.join("Assets/Scripts/TommyBarks.cs"), "class").unwrap();
    std::fs::write(root.join("a/dup.rs"), "").unwrap();
    std::fs::write(root.join("b/dup.rs"), "").unwrap();
    root
}

#[test]
fn workspace_relative_paths_and_unique_bare_names_are_found() {
    let root = workspace();
    assert_eq!(
        locate(Some(&root), None, "tmp/plan/PLAN.md"),
        Some(root.join("tmp/plan/PLAN.md"))
    );
    assert_eq!(
        locate(Some(&root), None, "TommyBarks.cs"),
        Some(root.join("Assets/Scripts/TommyBarks.cs"))
    );
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn missing_ambiguous_and_escaping_references_are_not_found() {
    let root = workspace();
    assert_eq!(locate(Some(&root), None, "nope/PLAN.md"), None);
    assert_eq!(
        locate(Some(&root), None, "dup.rs"),
        None,
        "two files share the name"
    );
    assert_eq!(locate(Some(&root), None, "Missing.cs"), None);
    assert_eq!(locate(Some(&root), None, "../outside.md"), None);
    assert_eq!(
        locate(Some(&root), None, "tmp/plan"),
        None,
        "a folder is not a file"
    );
    std::fs::remove_dir_all(&root).ok();
}
