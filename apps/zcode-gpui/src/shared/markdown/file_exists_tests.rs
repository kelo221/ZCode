use super::{file_exists, set_root};
use crate::shared::markdown::inline::{Style, render_inlines};
use pulldown_cmark::{Parser, TagEnd};

fn workspace() -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("zcode-file-exists-{}", uuid::Uuid::now_v7()));
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/real.rs"), "").unwrap();
    root
}

#[test]
fn paths_are_checked_against_the_workspace() {
    let root = workspace();
    set_root(Some(root.clone()));
    assert_eq!(file_exists("src/real.rs"), Some(true));
    assert_eq!(file_exists("src/missing.rs"), Some(false));
    std::fs::create_dir_all(root.join("src/dir")).unwrap();
    assert_eq!(
        file_exists("src/dir"),
        Some(false),
        "a folder is not a file"
    );
    set_root(None);
    assert_eq!(file_exists("src/missing.rs"), None, "no workspace: unknown");
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn bare_names_wait_for_the_index_then_need_one_match() {
    let root = workspace();
    set_root(Some(root.clone()));
    let started = std::time::Instant::now();
    let mut answer = file_exists("ExplosionUtils.Explode");
    while answer.is_none() && started.elapsed() < std::time::Duration::from_secs(10) {
        std::thread::sleep(std::time::Duration::from_millis(20));
        answer = file_exists("ExplosionUtils.Explode");
    }
    assert_eq!(answer, Some(false));
    assert_eq!(file_exists("real.rs"), Some(true));
    set_root(None);
    std::fs::remove_dir_all(&root).ok();
}

#[test]
fn missing_files_are_not_rendered_as_links() {
    let root = workspace();
    set_root(Some(root.clone()));
    let events: Vec<_> = Parser::new("see `src/real.rs` and `src/gone.rs`").collect();
    let mut pos = 1;
    let inline = render_inlines(
        &events,
        &mut pos,
        Some(&TagEnd::Paragraph),
        &Style::default(),
    );
    let urls: Vec<_> = inline.links.iter().map(|(_, u)| u.as_str()).collect();
    assert_eq!(urls, ["zcode-file:src/real.rs"]);
    set_root(None);
    std::fs::remove_dir_all(&root).ok();
}
