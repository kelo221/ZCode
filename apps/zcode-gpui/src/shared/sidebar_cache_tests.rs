use super::*;

fn entry(id: &str, at: i64, phase: &str) -> SessionEntry {
    SessionEntry {
        session_id: id.into(),
        title: format!("T {id}"),
        phase: phase.into(),
        last_activity_at: Some(at),
        sort_activity_at: Some(at),
        preview: "p".into(),
        has_background_work: true,
    }
}

#[test]
fn rows_and_open_folders_round_trip_through_the_file() {
    let dir = std::env::temp_dir().join(format!("zcode_sidebar_cache_{}", std::process::id()));
    let file = dir.join("gpui_sidebar_cache.json");
    let mut cache = SidebarCache::load(file.clone());
    assert!(cache.rows("ws").is_empty());
    assert!(cache.take_write().is_none());

    cache.record("ws", &[entry("b", 2, "running"), entry("a", 1, "idle")]);
    cache.set_open_folders(&HashSet::from(["ws".to_string()]));
    let (path, text) = cache.take_write().expect("dirty");
    crate::shared::project_activity::write(&path, &text).unwrap();
    cache.record("ws", &[entry("b", 2, "running"), entry("a", 1, "idle")]);
    assert!(cache.take_write().is_none(), "same rows are not a change");

    let loaded = SidebarCache::load(file);
    let rows = loaded.rows("ws");
    assert_eq!(
        rows.iter()
            .map(|r| r.session_id.as_str())
            .collect::<Vec<_>>(),
        ["b", "a"]
    );
    assert_eq!(rows[0].phase, "idle", "a cached row never shows as running");
    assert!(!rows[0].has_background_work);
    assert_eq!(rows[0].sort_activity_at, Some(2));
    assert!(loaded.open_folders().contains("ws"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn keeps_only_the_newest_rows_and_forgets_removed_workspaces() {
    let mut cache = SidebarCache::default();
    let many: Vec<_> = (0..40)
        .rev()
        .map(|i| entry(&i.to_string(), i, "idle"))
        .collect();
    cache.record("ws", &many);
    assert_eq!(cache.rows("ws").len(), CACHED_ROWS);
    assert_eq!(cache.rows("ws")[0].session_id, "39");
    cache.forget("ws");
    assert!(cache.rows("ws").is_empty());
    assert!(
        cache.take_write().is_none(),
        "memory-only cache never writes"
    );
}

#[test]
fn corrupt_file_starts_empty() {
    let dir = std::env::temp_dir().join(format!("zcode_sidebar_bad_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("gpui_sidebar_cache.json");
    std::fs::write(&file, "not json").unwrap();
    assert!(SidebarCache::load(file).rows("ws").is_empty());
    let _ = std::fs::remove_dir_all(dir);
}
