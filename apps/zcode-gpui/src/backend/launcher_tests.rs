//! Tests for `backend/launcher.rs`.

use super::*;

fn scratch(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("zcode_gpui_launcher_{tag}_{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn touch(root: &Path, rel: &str) {
    let p = root.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, "").unwrap();
}

const READY_FILES: [&str; 5] = [
    "node_modules/@zcode/shared/package.json",
    "apps/zcode-cli/packages/cli/src/main.ts",
    "apps/zcode-cli/packages/core/dist/index.js",
    "apps/zcode-cli/packages/bootstrap/dist/index.js",
    "apps/zcode-cli/packages/adapters/dist/index.js",
];

/// A source checkout that was never installed or built must not become the
/// first candidate: it dies before startup and costs a fallback round-trip.
#[test]
fn bun_source_requires_install_and_built_packages() {
    for missing in READY_FILES {
        let root = scratch("partial");
        for f in READY_FILES.iter().filter(|f| **f != missing) {
            touch(&root, f);
        }
        assert!(!bun_source_ready(&root), "ready without {missing}");
        let _ = std::fs::remove_dir_all(&root);
    }
    let root = scratch("ready");
    for f in READY_FILES {
        touch(&root, f);
    }
    assert!(bun_source_ready(&root));
    let _ = std::fs::remove_dir_all(&root);
}

/// `ZCODE_GPUI_AGENT_PROGRAM` always wins; the variables the desktop app
/// exports to its children must not, or a GPUI launched from a ZCode terminal
/// would silently skip bun. One test so the env mutations cannot race.
#[test]
fn only_the_gpui_variable_overrides() {
    let is_override = |c: &BackendLaunch| c.describe == "env override";
    // SAFETY: these variables are only read by this test.
    unsafe {
        std::env::set_var("GLM_BINARY_PATH", "desktop-agent");
        std::env::set_var("ZCODE_AGENT_SERVER_COMMAND", "desktop-agent");
    }
    let inherited = resolve_candidates(Path::new("."));
    unsafe { std::env::set_var("ZCODE_GPUI_AGENT_PROGRAM", "custom-agent") };
    let explicit = resolve_candidates(Path::new("."));
    unsafe {
        std::env::remove_var("ZCODE_GPUI_AGENT_PROGRAM");
        std::env::remove_var("GLM_BINARY_PATH");
        std::env::remove_var("ZCODE_AGENT_SERVER_COMMAND");
    }
    assert!(!inherited.iter().any(is_override));
    let first = explicit.first().expect("override candidate");
    assert!(is_override(first));
    assert_eq!(first.program, "custom-agent");
}
