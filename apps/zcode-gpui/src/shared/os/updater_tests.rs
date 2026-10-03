use super::*;
use serde_json::json;

#[test]
fn test_channel_mapping() {
    assert_eq!(ReleaseChannel::Stable.as_str(), "stable");
    assert_eq!(ReleaseChannel::Stable.api_value(), "1");
    assert_eq!(ReleaseChannel::Preview.as_str(), "preview");
    assert_eq!(ReleaseChannel::Preview.api_value(), "3");

    assert_eq!(
        ReleaseChannel::from_name("preview"),
        ReleaseChannel::Preview
    );
    assert_eq!(
        ReleaseChannel::from_name("PREVIEW"),
        ReleaseChannel::Preview
    );
    assert_eq!(ReleaseChannel::from_name("stable"), ReleaseChannel::Stable);
}

#[test]
fn test_platform_arch_mapping() {
    assert_eq!(map_electron_platform("windows"), "windows");
    assert_eq!(map_electron_platform("macos"), "darwin");
    assert_eq!(map_electron_platform("linux"), "linux");

    assert_eq!(map_electron_arch("x86_64"), "x86_64");
    assert_eq!(map_electron_arch("arm64"), "aarch64");
    assert_eq!(map_electron_arch("ia32"), "x86");
}

#[test]
fn test_build_manifest_url() {
    let url = build_manifest_url(
        "https://example.com/api/",
        "windows-x86_64",
        ReleaseChannel::Stable,
        Some("device-123"),
    );
    assert_eq!(
        url,
        "https://example.com/api/api/v1/releases/electron/manifest?platform=windows-x86_64&channel=1&device_mid=device-123"
    );

    let url_no_mid = build_manifest_url(
        "https://example.com",
        "darwin-aarch64",
        ReleaseChannel::Preview,
        None,
    );
    assert_eq!(
        url_no_mid,
        "https://example.com/api/v1/releases/electron/manifest?platform=darwin-aarch64&channel=3"
    );
}

#[test]
fn test_semver_comparisons() {
    assert!(is_newer_semver("3.14.2", "3.14.3"));
    assert!(is_newer_semver("3.14.3", "3.15.0"));
    assert!(is_newer_semver("3.14.3", "4.0.0"));
    assert!(is_newer_semver("v3.14.3", "v3.14.4"));

    assert!(!is_newer_semver("3.14.3", "3.14.3"));
    assert!(!is_newer_semver("3.14.3", "3.14.2"));
    assert!(!is_newer_semver("4.0.0", "3.15.9"));

    // Prerelease comparison
    assert!(is_newer_semver("3.15.0-alpha.1", "3.15.0"));
}

#[test]
fn test_manifest_parsing_and_evaluation() {
    let v = json!({
        "version": "3.15.0",
        "releaseDate": "2026-10-03T12:00:00Z",
        "releaseNotes": "## What's new in 3.15.0\n- Native GPUI frontend enhancements",
        "files": [
            {
                "url": "https://example.com/downloads/zcode-3.15.0-win.zip",
                "sha2": "abcd1234ef",
                "size": 52428800
            }
        ]
    });

    let manifest = ManifestUpdateInfo::from_value(&v).expect("should parse");
    assert_eq!(manifest.version, "3.15.0");
    assert_eq!(manifest.files.len(), 1);
    assert_eq!(
        manifest.files[0].url,
        "https://example.com/downloads/zcode-3.15.0-win.zip"
    );

    let res = evaluate_manifest("3.14.3", ReleaseChannel::Stable, &manifest);
    match res {
        UpdateCheckResult::Available {
            version,
            download_url,
            ..
        } => {
            assert_eq!(version, "3.15.0");
            assert_eq!(
                download_url.as_deref(),
                Some("https://example.com/downloads/zcode-3.15.0-win.zip")
            );
        }
        _ => panic!("expected Available update"),
    }

    let up_to_date = evaluate_manifest("3.15.0", ReleaseChannel::Stable, &manifest);
    assert_eq!(
        up_to_date,
        UpdateCheckResult::UpToDate {
            current_version: "3.15.0".to_string()
        }
    );
}
