use super::*;

struct Fixture {
    root: PathBuf,
    settings: PathBuf,
}

impl Fixture {
    fn new(projects: &[&str]) -> Self {
        let root = std::env::temp_dir().join(format!("gpui-ws-paths-{}", uuid::Uuid::now_v7()));
        for p in projects {
            std::fs::create_dir_all(root.join(p)).unwrap();
        }
        let settings = root.join("setting.json");
        Self { root, settings }
    }

    fn dir(&self, name: &str) -> String {
        self.root.join(name).to_string_lossy().into_owned()
    }

    fn write(&self, value: serde_json::Value) {
        std::fs::write(&self.settings, value.to_string()).unwrap();
    }

    fn names(&self, explicit: Option<&Path>) -> Vec<String> {
        discover_workspaces_from(&self.settings, explicit)
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn local(path: &str) -> serde_json::Value {
    serde_json::json!({"kind": "local", "workspacePath": path, "workspacePurpose": "project"})
}

#[test]
fn every_persisted_project_is_listed_without_a_cap() {
    let names: Vec<String> = (0..14).map(|i| format!("p{i:02}")).collect();
    let refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let f = Fixture::new(&refs);
    let session: Vec<_> = names[..12].iter().map(|n| local(&f.dir(n))).collect();
    let recents: Vec<_> = names[12..].iter().map(|n| f.dir(n)).collect();
    f.write(serde_json::json!({"lastWorkspaceSession": session, "recentProjects": recents}));
    assert_eq!(f.names(None), names);
}

#[test]
fn launch_folder_is_a_project_only_when_named_explicitly() {
    let f = Fixture::new(&["a", "b", "launch"]);
    f.write(serde_json::json!({"lastWorkspaceSession": [local(&f.dir("a"))], "recentProjects": [f.dir("b")]}));
    assert_eq!(f.names(None), ["a", "b"]);
    let launch = f.root.join("launch");
    assert_eq!(f.names(Some(&launch)), ["launch", "a", "b"]);
}

#[test]
fn last_active_project_leads_and_duplicates_collapse() {
    let f = Fixture::new(&["a", "b", "c"]);
    f.write(serde_json::json!({
        "lastActiveWorkspace": local(&f.dir("c")),
        "lastWorkspaceSession": [local(&f.dir("a")), local(&f.dir("b")), local(&f.dir("c"))],
        "recentProjects": [f.dir("b"), f.dir("a")],
    }));
    assert_eq!(f.names(None), ["c", "a", "b"]);
}

#[test]
fn remote_conversation_and_missing_entries_are_skipped() {
    let f = Fixture::new(&["a"]);
    f.write(serde_json::json!({
        "lastActiveWorkspace": {"kind": "remote", "workspacePath": f.dir("a")},
        "lastWorkspaceSession": [
            {"kind": "remote", "workspacePath": "/srv/remote", "workspacePurpose": "project"},
            {"kind": "local", "workspacePath": f.dir("a"), "workspacePurpose": "conversation"},
            local(&f.dir("gone")),
        ],
        "recentProjects": [f.dir("a")],
    }));
    assert_eq!(f.names(None), ["a"]);
}

#[test]
fn unreadable_settings_yield_only_the_explicit_folder() {
    let f = Fixture::new(&["launch"]);
    assert!(f.names(None).is_empty());
    std::fs::write(&f.settings, "not json").unwrap();
    assert_eq!(f.names(Some(&f.root.join("launch"))), ["launch"]);
}
