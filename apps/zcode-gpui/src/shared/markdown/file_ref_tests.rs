use super::*;

fn path_of(code: &str) -> Option<String> {
    code_file_ref(code).map(|r| r.path)
}

#[test]
fn path_like_inline_code_is_a_file_reference() {
    for (code, path, line) in [
        (
            "tmp/tommy-hookshot-yank/PLAN.md",
            "tmp/tommy-hookshot-yank/PLAN.md",
            None,
        ),
        ("TommyBarks.cs", "TommyBarks.cs", None),
        ("file.rs:42", "file.rs", Some(42)),
        ("src/a.rs:12:3", "src/a.rs", Some(12)),
        ("src/a.rs:12-20", "src/a.rs", Some(12)),
        ("docs/x.md#L7", "docs/x.md", Some(7)),
        (r"C:\x\y.ts", r"C:\x\y.ts", None),
        ("~/notes.md", "~/notes.md", None),
        ("./scripts/run", "./scripts/run", None),
        (
            "apps/zcode-gpui/PARITY.md",
            "apps/zcode-gpui/PARITY.md",
            None,
        ),
    ] {
        assert_eq!(
            code_file_ref(code),
            Some(FileRef {
                path: path.into(),
                line
            }),
            "{code}"
        );
    }
}

#[test]
fn ordinary_inline_code_is_not_a_file_reference() {
    for code in [
        "cargo test",
        "bun run typecheck",
        "--release",
        "*.rs",
        "https://example.com/a.md",
        "std::fs::read",
        "1.2.3",
        "v0.4",
        "HookYank",
        "PoolCount",
        "Option<T>",
        "foo()",
        "src/components/",
        "and/or",
        "a = b.c",
        "",
    ] {
        assert_eq!(path_of(code), None, "{code}");
    }
}

#[test]
fn link_targets_without_a_scheme_are_file_targets() {
    assert_eq!(
        link_file_target("tmp/PLAN.md").as_deref(),
        Some("tmp/PLAN.md")
    );
    assert_eq!(link_file_target("./a.rs:12").as_deref(), Some("./a.rs:12"));
    assert_eq!(link_file_target("a.rs:12").as_deref(), Some("a.rs:12"));
    assert_eq!(
        link_file_target("/C:/x/y.md").as_deref(),
        Some("/C:/x/y.md")
    );
    assert_eq!(link_file_target("C:/x/y.md").as_deref(), Some("C:/x/y.md"));
    assert_eq!(
        link_file_target("file:///C:/x/y.md").as_deref(),
        Some("C:/x/y.md")
    );
    assert_eq!(
        link_file_target("file:///home/x.md").as_deref(),
        Some("/home/x.md")
    );
    assert_eq!(link_file_target("https://example.com"), None);
    assert_eq!(link_file_target("mailto:a@b.c"), None);
    assert_eq!(link_file_target("#section"), None);
}

#[test]
fn relative_paths_resolve_inside_the_workspace_only() {
    let root = std::env::temp_dir().join("ws");
    let home = std::env::temp_dir().join("home");
    let resolve = |p: &str| resolve_path(Some(&root), Some(&home), p);
    assert_eq!(
        resolve("tmp/PLAN.md"),
        Some(root.join("tmp").join("PLAN.md"))
    );
    assert_eq!(resolve("./a/../b.rs"), Some(root.join("b.rs")));
    assert_eq!(resolve("../escape.rs"), None);
    assert_eq!(resolve("a/../../escape.rs"), None);
    assert_eq!(resolve("~/notes.md"), Some(home.join("notes.md")));
    let absolute = std::env::temp_dir().join("abs.md");
    assert_eq!(resolve(absolute.to_str().unwrap()), Some(absolute.clone()));
    assert_eq!(resolve_path(None, None, "tmp/PLAN.md"), None);
}

#[test]
fn bare_names_have_no_directory_part() {
    assert!(is_bare_name("TommyBarks.cs"));
    assert!(!is_bare_name("src/TommyBarks.cs"));
    assert!(!is_bare_name("~/x.md"));
}
