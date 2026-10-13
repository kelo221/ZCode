use super::*;

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "zcode-gpui-compiled-{name}-{}",
        uuid::Uuid::now_v7()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn complete_layout_launches_app_server_directly() {
    let dir = scratch("complete");
    std::fs::write(dir.join(BACKEND_EXE), b"").unwrap();
    std::fs::create_dir_all(dir.join("provider")).unwrap();
    std::fs::write(dir.join("provider").join("zcode-builtin.json"), b"{}").unwrap();

    let launch = launch_in(&dir).expect("complete layout");
    assert_eq!(PathBuf::from(&launch.program), dir.join(BACKEND_EXE));
    assert_eq!(
        launch.args,
        ["app-server", "--stdio", "--surface", "desktop"]
    );
    assert!(
        launch
            .envs
            .iter()
            .any(|(k, v)| k == "ZCODE_RUNTIME_ENV" && v == "desktop")
    );
    assert!(!launch.envs.iter().any(|(k, _)| k == "ELECTRON_RUN_AS_NODE"));
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn executable_without_provider_release_is_skipped() {
    let dir = scratch("partial");
    std::fs::write(dir.join(BACKEND_EXE), b"").unwrap();
    assert!(launch_in(&dir).is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}
