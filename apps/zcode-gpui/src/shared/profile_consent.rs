//! GPUI-owned record that the user opted into local Subagents management, so
//! Settings › Subagents can start the Services Host without re-confirming.
//! Management is on by default; only an explicit revocation turns it off.
//! Stored in its own file, never in the preference file Desktop also writes.
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

const KEY: &str = "localManagement";

pub(crate) fn path() -> PathBuf {
    crate::shared::data_paths::paths()
        .settings_home
        .join(".zcode")
        .join("v2")
        .join("gpui_profile_consent.json")
}

/// 原因：默认值为 false，首次启用若因 Desktop 运行等原因失败就从未写入，
/// 用户每次都被要求“启用本地管理”。修复：只有显式撤销（写入 false）才关闭自动管理，
/// 缺失或损坏的记录视为已同意。
pub(crate) fn load(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .and_then(|value| value.get(KEY).and_then(Value::as_bool))
        .unwrap_or(true)
}

pub(crate) fn store(path: &Path, enabled: bool) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp, json!({ KEY: enabled }).to_string())?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consent_roundtrip_and_defaults() {
        let dir = std::env::temp_dir().join(format!("zcode_consent_{}", uuid::Uuid::now_v7()));
        let file = dir.join("gpui_profile_consent.json");
        assert!(load(&file));
        store(&file, false).unwrap();
        assert!(!load(&file));
        store(&file, true).unwrap();
        assert!(load(&file));
        std::fs::write(&file, "not json").unwrap();
        assert!(load(&file));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
