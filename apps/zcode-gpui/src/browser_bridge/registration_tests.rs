use super::registration::{
    HostRegistry, REGISTRY_PARENTS, Registered, manifest_file, manifest_json, register, status,
    unregister,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Default)]
struct FakeRegistry {
    keys: RefCell<HashMap<String, String>>,
}

impl HostRegistry for FakeRegistry {
    fn supported(&self) -> bool {
        true
    }
    fn read(&self, parent: &str) -> Option<String> {
        self.keys.borrow().get(parent).cloned()
    }
    fn write(&self, parent: &str, manifest: &Path) -> Result<(), String> {
        self.keys
            .borrow_mut()
            .insert(parent.into(), manifest.to_string_lossy().into());
        Ok(())
    }
    fn delete(&self, parent: &str) -> Result<(), String> {
        self.keys.borrow_mut().remove(parent);
        Ok(())
    }
}

fn scratch(name: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!("zcode-gpui-reg-{name}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn manifest_names_the_host_and_only_our_extension() {
    let value: serde_json::Value = serde_json::from_str(&manifest_json(Path::new(
        r"C:\Apps\ZCode\zcode-browser-host.exe",
    )))
    .unwrap();
    assert_eq!(value["name"], "dev.zcode.gpui.browser");
    assert_eq!(value["type"], "stdio");
    assert_eq!(value["path"], r"C:\Apps\ZCode\zcode-browser-host.exe");
    assert_eq!(
        value["allowed_origins"],
        serde_json::json!(["chrome-extension://ogdkmkfjcfnbhcankdfaeddjjkohdojh/"])
    );
}

#[test]
fn register_writes_manifest_and_every_key_then_unregister_removes_them() {
    let dir = scratch("roundtrip");
    let host = dir.join("zcode-browser-host.exe");
    std::fs::write(&host, b"").unwrap();
    let registry = FakeRegistry::default();
    assert_eq!(status(&registry, &host).registered, Registered::No);

    register(&registry, &dir.join("bridge"), &host).unwrap();
    let manifest = manifest_file(&dir.join("bridge"));
    assert!(manifest.is_file());
    for parent in REGISTRY_PARENTS {
        assert_eq!(
            registry.read(parent).as_deref(),
            Some(manifest.to_string_lossy().as_ref())
        );
    }
    let current = status(&registry, &host);
    assert_eq!(current.registered, Registered::Ours);
    assert!(current.host_exists);

    unregister(&registry, &dir.join("bridge")).unwrap();
    assert!(!manifest.exists());
    assert!(registry.keys.borrow().is_empty());
    // Unregistering twice is harmless.
    unregister(&registry, &dir.join("bridge")).unwrap();
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn another_gpui_copy_is_reported_and_missing_host_refuses() {
    let dir = scratch("other");
    let ours = dir.join("release").join("zcode-browser-host.exe");
    let theirs = dir.join("debug").join("zcode-browser-host.exe");
    for host in [&ours, &theirs] {
        std::fs::create_dir_all(host.parent().unwrap()).unwrap();
        std::fs::write(host, b"").unwrap();
    }
    let registry = FakeRegistry::default();
    register(&registry, &dir, &theirs).unwrap();
    assert_eq!(
        status(&registry, &ours).registered,
        Registered::Other(theirs.to_string_lossy().into())
    );
    // Registering again takes the name over.
    register(&registry, &dir, &ours).unwrap();
    assert_eq!(status(&registry, &ours).registered, Registered::Ours);

    let missing = dir.join("nowhere").join("zcode-browser-host.exe");
    assert!(register(&registry, &dir, &missing).is_err());
    assert!(!status(&registry, &missing).host_exists);
    let _ = std::fs::remove_dir_all(dir);
}

#[cfg(windows)]
#[test]
fn windows_registry_round_trips_a_default_value() {
    use super::registration::windows_registry::{delete, read_default, write_default};
    let key = format!(r"Software\ZCodeGpuiTest\reg-{}", std::process::id());
    write_default(&key, r"C:\Temp\manifest.json").unwrap();
    assert_eq!(
        read_default(&key).as_deref(),
        Some(r"C:\Temp\manifest.json")
    );
    delete(&key).unwrap();
    assert_eq!(read_default(&key), None);
    delete(&key).unwrap();
    let _ = delete(r"Software\ZCodeGpuiTest");
}
