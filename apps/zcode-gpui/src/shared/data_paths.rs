use std::path::PathBuf;
use std::sync::OnceLock;

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DataPaths {
    pub settings_home: PathBuf,
    pub data_base: PathBuf,
}

impl DataPaths {
    pub fn resolve(env: impl Fn(&str) -> Option<String>, persisted_base: Option<&str>) -> Self {
        let read = |key| {
            env(key)
                .filter(|s| !s.trim().is_empty())
                .map(|s| s.trim().to_owned())
        };
        let home = read("HOME")
            .or_else(|| read("USERPROFILE"))
            .unwrap_or_else(|| ".".into());
        let settings_home = read("ZCODE_DESKTOP_HOME_DIR").unwrap_or_else(|| home.clone());
        let data_base = persisted_base
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .or_else(|| read("ZCODE_DATA_BASE_DIR"))
            .unwrap_or(home);
        Self {
            settings_home: settings_home.into(),
            data_base: data_base.into(),
        }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.settings_home
            .join(".zcode")
            .join("v2")
            .join("setting.json")
    }

    pub fn data_root(&self) -> PathBuf {
        // 桌面 dataBaseDir 是 .zcode 的父目录，不能把环境变量直接当作数据根。
        self.data_base.join(".zcode")
    }
}

static PATHS: OnceLock<DataPaths> = OnceLock::new();

pub(crate) fn initialize(persisted_base: Option<&str>) {
    let value = if let Some(isolated) = crate::shared::isolation::active() {
        DataPaths {
            settings_home: isolated.home(),
            data_base: isolated.data_base(),
        }
    } else {
        DataPaths::resolve(|k| std::env::var(k).ok(), persisted_base)
    };
    let _ = PATHS.set(value);
}

pub(crate) fn paths() -> &'static DataPaths {
    PATHS.get_or_init(|| DataPaths::resolve(|k| std::env::var(k).ok(), None))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_home_and_data_base_have_distinct_precedence() {
        let env = |key: &str| match key {
            "ZCODE_DESKTOP_HOME_DIR" => Some("bootstrap".into()),
            "HOME" => Some("home".into()),
            "USERPROFILE" => Some("profile".into()),
            "ZCODE_DATA_BASE_DIR" => Some("environment".into()),
            _ => None,
        };
        let p = DataPaths::resolve(env, Some(" persisted "));
        assert_eq!(
            p.settings_file(),
            PathBuf::from("bootstrap/.zcode/v2/setting.json")
        );
        assert_eq!(p.data_root(), PathBuf::from("persisted/.zcode"));
        let p = DataPaths::resolve(env, None);
        assert_eq!(p.data_root(), PathBuf::from("environment/.zcode"));
    }

    #[test]
    fn defaults_and_blank_overrides_match_desktop() {
        let p = DataPaths::resolve(
            |k| (k == "USERPROFILE").then(|| "profile".into()),
            Some(" "),
        );
        assert_eq!(p.data_root(), PathBuf::from("profile/.zcode"));
        assert_eq!(
            p.settings_file(),
            PathBuf::from("profile/.zcode/v2/setting.json")
        );
        let p = DataPaths::resolve(|_| None, None);
        assert_eq!(p.data_base, PathBuf::from("."));
    }
}
