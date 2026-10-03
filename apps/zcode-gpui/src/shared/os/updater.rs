//! Desktop manifest update provider client and update status model.
//!
//! Follows packages/desktop/src/main/manifestUpdateProvider.ts and
//! packages/shared/src/update.ts.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const APP_VERSION: &str = match option_env!("ZCODE_APP_VERSION") {
    Some(v) => v,
    None => "3.14.3",
};

pub const GIT_COMMIT: &str = match option_env!("ZCODE_GIT_COMMIT") {
    Some(v) => v,
    None => "unknown",
};

pub const DEFAULT_ZCODE_ENDPOINT: &str = "https://zcode.example.com";
pub const MANIFEST_API_PATH: &str = "/api/v1/releases/electron/manifest";

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReleaseChannel {
    #[default]
    Stable,
    Preview,
}

impl ReleaseChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Stable => "stable",
            Self::Preview => "preview",
        }
    }

    pub fn api_value(&self) -> &'static str {
        match self {
            Self::Stable => "1",
            Self::Preview => "3",
        }
    }

    pub fn from_name(name: &str) -> Self {
        if name.eq_ignore_ascii_case("preview") {
            Self::Preview
        } else {
            Self::Stable
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ManifestFileInfo {
    pub url: String,
    pub sha2: Option<String>,
    pub sha512: Option<String>,
    pub size: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ManifestUpdateInfo {
    pub version: String,
    pub release_date: Option<String>,
    pub release_notes: Option<String>,
    pub files: Vec<ManifestFileInfo>,
}

impl ManifestUpdateInfo {
    pub fn from_value(v: &Value) -> Option<Self> {
        let version = v.get("version").and_then(Value::as_str)?.to_string();
        let release_date = v
            .get("releaseDate")
            .and_then(Value::as_str)
            .map(str::to_string);
        let release_notes = v
            .get("releaseNotes")
            .and_then(Value::as_str)
            .map(str::to_string);

        let mut files = Vec::new();
        if let Some(arr) = v.get("files").and_then(Value::as_array) {
            for item in arr {
                if let Some(url) = item.get("url").and_then(Value::as_str) {
                    files.push(ManifestFileInfo {
                        url: url.to_string(),
                        sha2: item.get("sha2").and_then(Value::as_str).map(str::to_string),
                        sha512: item
                            .get("sha512")
                            .and_then(Value::as_str)
                            .map(str::to_string),
                        size: item.get("size").and_then(Value::as_u64),
                    });
                }
            }
        } else if let Some(path) = v.get("path").and_then(Value::as_str) {
            files.push(ManifestFileInfo {
                url: path.to_string(),
                sha2: v.get("sha2").and_then(Value::as_str).map(str::to_string),
                sha512: v.get("sha512").and_then(Value::as_str).map(str::to_string),
                size: None,
            });
        }

        Some(Self {
            version,
            release_date,
            release_notes,
            files,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum UpdateCheckResult {
    UpToDate {
        current_version: String,
    },
    Available {
        version: String,
        channel: ReleaseChannel,
        release_notes: Option<String>,
        download_url: Option<String>,
    },
    Downloading {
        version: String,
    },
    AlreadyDownloading {
        version: String,
        progress: String,
    },
    Ready {
        version: String,
    },
    DevSkipped,
    Error {
        message: String,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum UpdateState {
    Idle {
        enabled: bool,
    },
    Checking {
        enabled: bool,
    },
    UpdateAvailable {
        enabled: bool,
        version: String,
        channel: Option<ReleaseChannel>,
        release_notes: Option<String>,
    },
    DownloadProgress {
        enabled: bool,
        progress: String,
        transferred_bytes: Option<u64>,
        total_bytes: Option<u64>,
        version: Option<String>,
        channel: Option<ReleaseChannel>,
    },
    UpdateDownloaded {
        enabled: bool,
        version: String,
        channel: Option<ReleaseChannel>,
        release_notes: Option<String>,
    },
}

pub fn map_electron_platform(os: &str) -> &'static str {
    match os {
        "windows" => "windows",
        "macos" | "darwin" => "darwin",
        "linux" => "linux",
        _ => "windows",
    }
}

pub fn map_electron_arch(arch: &str) -> &'static str {
    match arch {
        "arm64" | "aarch64" => "aarch64",
        "x64" | "x86_64" => "x86_64",
        "ia32" | "x86" => "x86",
        _ => "x86_64",
    }
}

pub fn current_platform_target() -> String {
    let os = if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else {
        "linux"
    };

    let arch = if cfg!(target_arch = "x86_64") {
        "x86_64"
    } else if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86"
    };

    format!("{os}-{arch}")
}

pub fn build_manifest_url(
    endpoint_origin: &str,
    platform: &str,
    channel: ReleaseChannel,
    device_mid: Option<&str>,
) -> String {
    let base = endpoint_origin.trim_end_matches('/');
    let mut url = format!(
        "{base}{MANIFEST_API_PATH}?platform={platform}&channel={}",
        channel.api_value()
    );
    if let Some(mid) = device_mid
        && !mid.trim().is_empty()
    {
        url.push_str("&device_mid=");
        url.push_str(mid.trim());
    }
    url
}

fn parse_semver(s: &str) -> (Vec<u64>, Option<&str>) {
    let clean = s.trim().trim_start_matches('v');
    let parts: Vec<&str> = clean.splitn(2, '-').collect();
    let nums: Vec<u64> = parts[0]
        .split('.')
        .map(|n| n.parse::<u64>().unwrap_or(0))
        .collect();
    let prerelease = parts.get(1).copied();
    (nums, prerelease)
}

/// Simple semver comparator. Returns true if candidate is strictly newer than current.
pub fn is_newer_semver(current: &str, candidate: &str) -> bool {
    let (c_nums, c_pre) = parse_semver(current);
    let (t_nums, t_pre) = parse_semver(candidate);

    let max_len = c_nums.len().max(t_nums.len());
    for i in 0..max_len {
        let c_val = c_nums.get(i).copied().unwrap_or(0);
        let t_val = t_nums.get(i).copied().unwrap_or(0);
        if t_val > c_val {
            return true;
        }
        if t_val < c_val {
            return false;
        }
    }

    match (c_pre, t_pre) {
        (Some(_), None) => true, // release > prerelease
        (None, Some(_)) => false,
        (Some(cp), Some(tp)) => tp > cp,
        (None, None) => false,
    }
}

pub fn evaluate_manifest(
    current_version: &str,
    channel: ReleaseChannel,
    manifest: &ManifestUpdateInfo,
) -> UpdateCheckResult {
    if is_newer_semver(current_version, &manifest.version) {
        UpdateCheckResult::Available {
            version: manifest.version.clone(),
            channel,
            release_notes: manifest.release_notes.clone(),
            download_url: manifest.files.first().map(|f| f.url.clone()),
        }
    } else {
        UpdateCheckResult::UpToDate {
            current_version: current_version.to_string(),
        }
    }
}

#[cfg(test)]
#[path = "updater_tests.rs"]
mod tests;
