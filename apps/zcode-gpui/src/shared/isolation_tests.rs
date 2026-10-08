use super::*;

#[test]
fn disposable_mode_creates_only_fresh_owned_roots() {
    let first = IsolatedSettings::create().unwrap();
    let second = IsolatedSettings::create().unwrap();
    assert_ne!(first.root(), second.root());
    for path in [
        first.home(),
        first.data_base(),
        first.workspace(),
        first.temp(),
    ] {
        assert!(path.is_dir());
        assert!(path.starts_with(first.root()));
    }
    assert!(first.allows_workspace(&first.workspace()));
    assert!(first.contains_known_path(&first.workspace()));
    let plain = first
        .workspace()
        .to_string_lossy()
        .trim_start_matches(r"\\?\")
        .to_owned();
    assert!(first.contains_known_path(Path::new(&plain)));
    assert!(!first.allows_workspace(&std::env::temp_dir()));
    assert!(!first.allows_workspace(&first.workspace().join("../..")));
    assert!(!first.contains_known_path(&first.workspace().join("../..")));
}

#[test]
fn child_environment_overrides_storage_and_drops_inherited_configuration() {
    let isolated = IsolatedSettings::create().unwrap();
    let mut env = vec![
        ("HTTP_PROXY".into(), "private-proxy".into()),
        ("ZCODE_HOME".into(), "real-home".into()),
        (
            "ZCODE_BUILTIN_PROVIDER_CONFIG_FILE".into(),
            "real-provider".into(),
        ),
        (
            "ZCODE_GPUI_ACCEPTANCE_TOKEN".into(),
            "not-propagated".into(),
        ),
        (
            "ZCODE_GPUI_ACCEPTANCE_TICKET".into(),
            "not-propagated".into(),
        ),
        ("ZCODE_GPUI_ACCEPTANCE_THEME".into(), "zai-light".into()),
        ("HOME".into(), "real-home".into()),
        ("PATH".into(), "system-tools".into()),
    ];
    isolated.apply_child_environment(&mut env);
    let get = |key| env.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    assert_eq!(get("PATH"), Some("system-tools"));
    assert_eq!(get("HOME"), isolated.home().to_str());
    assert_eq!(get("USERPROFILE"), isolated.home().to_str());
    assert_eq!(get("ZCODE_DATA_BASE_DIR"), isolated.data_base().to_str());
    for key in [
        "ZCODE_HOME",
        "ZCODE_BUILTIN_PROVIDER_CONFIG_FILE",
        "HTTP_PROXY",
        "ZCODE_GPUI_ACCEPTANCE_TOKEN",
        "ZCODE_GPUI_ACCEPTANCE_TICKET",
        "ZCODE_GPUI_ACCEPTANCE_THEME",
    ] {
        assert_eq!(get(key), None);
    }
    assert_eq!(get("TEMP"), isolated.temp().to_str());
}

#[cfg(debug_assertions)]
fn released_ticket() -> (PathBuf, String, PathBuf) {
    let isolated = IsolatedSettings::create().unwrap();
    let ticket = isolated.ticket_path();
    let value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&ticket).unwrap()).unwrap();
    let token = value["token"].as_str().unwrap().to_owned();
    let root = isolated.root().to_owned();
    drop(isolated);
    (ticket, token, root)
}

#[cfg(debug_assertions)]
#[test]
fn harness_reattaches_same_root_preserving_fixture_after_lock_release() {
    let (ticket, token, root) = released_ticket();
    let sentinel = root.join("home/synthetic.txt");
    std::fs::write(&sentinel, "synthetic restart sentinel").unwrap();
    let attached = IsolatedSettings::reattach(&ticket, &token).unwrap();
    assert_eq!(attached.root(), root);
    assert_eq!(
        std::fs::read_to_string(sentinel).unwrap(),
        "synthetic restart sentinel"
    );
    assert!(IsolatedSettings::reattach(&ticket, &token).is_err());
    drop(attached);
    assert!(IsolatedSettings::reattach(&ticket, &token).is_ok());
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(debug_assertions)]
#[test]
fn harness_rejects_missing_wrong_token_and_marker_identity() {
    let (ticket, token, root) = released_ticket();
    assert!(IsolatedSettings::reattach(&ticket, "").is_err());
    assert!(IsolatedSettings::reattach(&ticket, "wrong-token").is_err());
    assert!(IsolatedSettings::reattach(&root.join("other.json"), &token).is_err());
    let marker = root.join(MARKER_NAME);
    let original = std::fs::read(&marker).unwrap();
    let mut value: serde_json::Value = serde_json::from_slice(&original).unwrap();
    value["identity"] = serde_json::json!(uuid::Uuid::now_v7().to_string());
    std::fs::write(&marker, serde_json::to_vec(&value).unwrap()).unwrap();
    assert!(IsolatedSettings::reattach(&ticket, &token).is_err());
    std::fs::write(&marker, original).unwrap();
    std::fs::remove_dir_all(root.join("data")).unwrap();
    assert!(IsolatedSettings::reattach(&ticket, &token).is_err());
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(debug_assertions)]
#[test]
fn harness_rejects_other_parent_name_and_copied_marker_roots() {
    let (_ticket, token, root) = released_ticket();
    let nested = root.join(format!("{ROOT_PREFIX}{}", uuid::Uuid::now_v7()));
    std::fs::create_dir(&nested).unwrap();
    for name in [TICKET_NAME, MARKER_NAME, LOCK_NAME] {
        std::fs::copy(root.join(name), nested.join(name)).unwrap();
    }
    assert!(IsolatedSettings::reattach(&nested.join(TICKET_NAME), &token).is_err());
    assert!(IsolatedSettings::reattach(&std::env::temp_dir().join(TICKET_NAME), &token).is_err());
    let renamed = root.with_file_name(format!("unowned-{}", uuid::Uuid::now_v7()));
    std::fs::rename(&root, &renamed).unwrap();
    assert!(IsolatedSettings::reattach(&renamed.join(TICKET_NAME), &token).is_err());
    std::fs::rename(&renamed, &root).unwrap();
    let copied = root.with_file_name(format!("{ROOT_PREFIX}{}", uuid::Uuid::now_v7()));
    std::fs::rename(&root, &copied).unwrap();
    assert!(IsolatedSettings::reattach(&copied.join(TICKET_NAME), &token).is_err());
    std::fs::remove_dir_all(copied).unwrap();
}

#[cfg(all(unix, debug_assertions))]
#[test]
fn harness_rejects_any_symlink_even_inside_owned_root() {
    let (ticket, token, root) = released_ticket();
    let link = root.join("home/linked");
    std::os::unix::fs::symlink(root.join("workspace"), &link).unwrap();
    assert!(IsolatedSettings::reattach(&ticket, &token).is_err());
    std::fs::remove_file(link).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(all(windows, debug_assertions))]
#[test]
fn harness_rejects_windows_junction_escape() {
    let (ticket, token, root) = released_ticket();
    let link = root.join("home/linked");
    let status = std::process::Command::new("cmd.exe")
        .args(["/D", "/C", "mklink", "/J"])
        .arg(&link)
        .arg(std::env::temp_dir())
        .output()
        .unwrap();
    assert!(status.status.success(), "junction fixture creation failed");
    assert!(IsolatedSettings::reattach(&ticket, &token).is_err());
    std::fs::remove_dir(link).unwrap();
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn symlink_workspace_cannot_escape_owned_root() {
    let isolated = IsolatedSettings::create().unwrap();
    let link = isolated.root().join("outside");
    std::os::unix::fs::symlink(std::env::temp_dir(), &link).unwrap();
    assert!(!isolated.allows_workspace(&link));
}
