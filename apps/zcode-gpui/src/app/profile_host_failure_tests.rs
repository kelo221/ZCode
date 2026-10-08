//! Opt-in characterization of unchanged Services; owned scratch storage only, no UI evidence.
use super::profile_host_support::{ScratchHost, named};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

#[test]
#[ignore = "requires unchanged Services bundle and installed compatible runtime"]
fn real_unchanged_host_native_and_unmodeled_settings_startup_list_characterization() {
    let mut host = ScratchHost::new();
    let defaults = settings_get(&host);
    let path = settings_file(&host);
    for initialized in [true, false] {
        host.stop();
        let mut fixture = defaults.clone();
        fixture["themePreference"] = json!("dark");
        fixture["uiFontSize"] = json!(19.5);
        fixture["nativeUnmodeledSetting"] = json!({"sentinel": "synthetic-scratch-only"});
        for flag in [
            "closeToTrayOnWindowsMigrationInitialized",
            "messageStreamShowReasoningMigrationInitialized",
        ] {
            if initialized {
                fixture[flag] = json!(true);
            } else {
                fixture.as_object_mut().unwrap().remove(flag);
            }
        }
        let bytes = serde_json::to_vec_pretty(&fixture).unwrap();
        host.write_fixture(&path, &bytes);
        host.start();
        let startup = read_json(&host, &path);
        let startup_unchanged = std::fs::read(&path).unwrap() == bytes;
        let inventory = host.user_list();
        assert!(inventory.agents.iter().any(|a| a.config.name == "Explore"));
        let after_list = read_json(&host, &path);
        let projection = settings_get(&host);
        let after_get = read_json(&host, &path);
        // 服务 schema 不含 native 字段；是否落盘取决于实际迁移条件，不能猜测保真保存。
        eprintln!(
            "settings characterization {}",
            json!({
                "initialized": initialized, "startupBytesUnchanged": startup_unchanged,
                "startup": setting_fields(&startup), "afterList": setting_fields(&after_list),
                "getProjection": setting_fields(&projection), "afterGet": setting_fields(&after_get),
                "listChangedSettings": startup != after_list, "getChangedSettings": after_list != after_get,
                "mutationRequests": 0,
            })
        );
        host.stop();
        let stopped = read_json(&host, &path);
        host.start();
        host.user_list();
        let restarted = read_json(&host, &path);
        eprintln!(
            "settings restart characterization {}",
            json!({"initialized": initialized, "afterStop": setting_fields(&stopped),
                "afterRestart": setting_fields(&restarted),
                "restartChangedSettings": stopped != restarted, "mutationRequests": 0})
        );
    }
}

#[test]
#[ignore = "requires unchanged Services bundle and installed compatible runtime"]
fn real_unchanged_host_corrupt_settings_quarantine_characterization() {
    let mut host = ScratchHost::new();
    host.stop();
    let path = settings_file(&host);
    let corrupt = b"{\"synthetic-corrupt-setting\":";
    host.write_fixture(&path, corrupt);
    host.start();
    let projection = settings_get(&host);
    host.user_list();
    // 连续 JSON 解析失败后现有服务隔离原文；此处不伪称 native rebase 或无损修复。
    let backups: Vec<_> = std::fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|file| {
            file.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("setting.json.corrupt-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    host.owned_path(&backups[0]);
    assert_eq!(std::fs::read(&backups[0]).unwrap(), corrupt);
    assert!(projection.is_object());
    eprintln!(
        "corrupt settings characterization {}",
        json!({"backupCount": backups.len(), "backupRetainsCorruptBytes": true,
            "canonicalExists": path.exists(), "getProjection": setting_fields(&projection),
            "mutationRequests": 0})
    );
    host.restart();
    host.user_list();
    assert_eq!(std::fs::read(&backups[0]).unwrap(), corrupt);
}

#[test]
#[ignore = "requires unchanged Services bundle and installed compatible runtime"]
fn real_unchanged_host_corrupt_agent_state_lists_defaults_without_repair() {
    let mut host = ScratchHost::new();
    let profile = host.create(
        "user",
        &host.isolated.workspace(),
        config("corrupt-state-agent"),
    );
    host.undefined(
        "setEnabled",
        json!({"agentId": profile.id, "enabled": false}),
    );
    assert!(!named(&host.user_list().agents, "corrupt-state-agent").enabled);
    host.stop();
    let path = host.state_file();
    let corrupt = b"{\"synthetic-corrupt-agent-state\":";
    host.write_fixture(&path, corrupt);
    host.start();
    for restarted in [false, true] {
        if restarted {
            host.restart();
        }
        let inventory = host.user_list();
        let listed = named(&inventory.agents, "corrupt-state-agent");
        // 既有 reader 对损坏 state 返回空覆盖，因此禁用信息不在 list 中；不写盘修复。
        assert!(listed.enabled);
        assert_eq!(std::fs::read(&path).unwrap(), corrupt);
        eprintln!(
            "corrupt agents-state characterization {}",
            json!({"restarted": restarted, "profileEnabled": listed.enabled,
                "corruptBytesUnchanged": true, "mutationRequestsAfterCorruption": 0})
        );
    }
}

#[cfg(windows)]
#[test]
#[ignore = "requires unchanged Services bundle and installed compatible runtime"]
fn real_unchanged_host_held_state_refuses_replace_without_mutation_replay() {
    let mut host = ScratchHost::new();
    let profile = host.create(
        "user",
        &host.isolated.workspace(),
        config("held-state-agent"),
    );
    host.undefined(
        "setEnabled",
        json!({"agentId": profile.id, "enabled": true}),
    );
    host.stop();
    let path = host.state_file();
    let before = std::fs::read(&path).unwrap();
    let held = hold_without_delete_sharing(&host, &path);
    host.start();
    // Windows DELETE sharing 被拒绝，atomic rename 的既有内部重试耗尽后必须返回真实失败。
    let error = host
        .try_call(
            "subagents",
            "setEnabled",
            vec![json!({"agentId": profile.id, "enabled": false})],
        )
        .expect_err("held target must reject atomic replacement");
    let inventory = host.user_list();
    assert!(named(&inventory.agents, "held-state-agent").enabled);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    eprintln!(
        "held state characterization {}",
        json!({
            "rpcError": error, "mutationRequests": 1, "freshEnabled": true,
            "stateBytesUnchanged": true, "backendInternalRenameRetries": "unchanged",
        })
    );
    host.stop();
    drop(held);
    host.start();
    assert!(named(&host.user_list().agents, "held-state-agent").enabled);
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[cfg(windows)]
#[test]
#[ignore = "requires unchanged Services bundle and installed compatible runtime"]
fn real_unchanged_host_partial_rename_retains_both_profiles_without_replay() {
    let mut host = ScratchHost::new();
    let profile = host.create(
        "user",
        &host.isolated.workspace(),
        config("partial-old-agent"),
    );
    host.undefined(
        "setEnabled",
        json!({"agentId": profile.id, "enabled": false}),
    );
    let old = named(&host.user_list().agents, "partial-old-agent").clone();
    assert!(!old.enabled);
    host.stop();
    let old_path = Path::new(&old.path);
    let old_bytes = std::fs::read(old_path).unwrap();
    let held = hold_without_delete_sharing(&host, old_path);
    host.start();
    let mut next = serde_json::to_value(&old.config).unwrap();
    next["name"] = json!("partial-new-agent");
    next["description"] = json!("Synthetic renamed description");
    let params = json!({
        "provider": "glm", "scope": "user", "agentId": old.id, "oldFilePath": old.path,
        "workspacePath": host.isolated.workspace(), "config": next,
    });
    // 顺序是新 Markdown → disabled ID 迁移 → 删除旧文件；锁旧文件只阻断最后一步。
    let error = host
        .try_call("subagents", "updateAgent", vec![params.clone()])
        .expect_err("held old profile must reject the final unlink");
    let mutation = super::profile_state::ProfileMutation {
        origin: Default::default(),
        receipt: "synthetic-unacknowledged-rename".into(),
        scope: "user".into(),
        method: "updateAgent".into(),
        params,
        baseline: Some(old.clone()),
    };
    assert_partial_rename(&host, &mutation, old_path, &old_bytes, &next);
    eprintln!(
        "partial rename characterization {}",
        json!({
            "rpcError": error, "mutationRequests": 1, "oldExists": true, "newExists": true,
            "oldEnabled": true, "newEnabled": false, "classifierObserved": false,
        })
    );
    host.stop();
    drop(held);
    host.start();
    // 释放 handle 后仅 fresh list；不能补发 update、删除旧文件或凭目标存在冒称成功。
    assert_partial_rename(&host, &mutation, old_path, &old_bytes, &next);
}

#[cfg(windows)]
fn assert_partial_rename(
    host: &ScratchHost,
    mutation: &super::profile_state::ProfileMutation,
    old_path: &Path,
    old_bytes: &[u8],
    next: &Value,
) {
    let inventory = host.user_list();
    let old = named(&inventory.agents, "partial-old-agent");
    let new = named(&inventory.agents, "partial-new-agent");
    assert_eq!(inventory.user_agents.len(), 2);
    assert!(old.enabled);
    assert!(!new.enabled);
    assert_eq!(std::fs::read(old_path).unwrap(), old_bytes);
    host.owned_path(Path::new(&new.path));
    let intended: crate::app::subagent_profiles::SubAgentConfig =
        serde_json::from_value(next.clone()).unwrap();
    assert_eq!(new.config, intended);
    assert!(!super::profile_recovery::observed(mutation, &inventory));
}

#[cfg(windows)]
fn hold_without_delete_sharing(host: &ScratchHost, path: &Path) -> std::fs::File {
    use std::os::windows::fs::OpenOptionsExt;
    host.owned_path(path);
    // 不改 ACL/只读属性，不竞争业务写入；仅 owned 测试 handle 禁止重命名/删除。
    std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0x1 | 0x2) // FILE_SHARE_READ | FILE_SHARE_WRITE，故意无 DELETE。
        .open(path)
        .unwrap()
}

fn config(name: &str) -> Value {
    json!({"name": name, "description": "Synthetic failure fixture", "systemPrompt": "Scratch only"})
}

fn settings_file(host: &ScratchHost) -> PathBuf {
    host.isolated.home().join(".zcode/v2/setting.json")
}

fn settings_get(host: &ScratchHost) -> Value {
    let projection = host.call("setting", "get", vec![]).into_json().unwrap();
    assert!(projection.is_object());
    projection
}

fn read_json(host: &ScratchHost, path: &Path) -> Value {
    host.owned_path(path);
    serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap()
}

fn setting_fields(value: &Value) -> Value {
    json!({
        "themePreference": value.get("themePreference"), "uiFontSize": value.get("uiFontSize"),
        "nativeUnmodeledSetting": value.get("nativeUnmodeledSetting"),
        "closeToTrayMigration": value.get("closeToTrayOnWindowsMigrationInitialized"),
        "reasoningMigration": value.get("messageStreamShowReasoningMigrationInitialized"),
    })
}
