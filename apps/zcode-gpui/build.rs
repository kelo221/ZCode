//! Build script for zcode-gpui
//! Extracts i18n translations from TypeScript source files or cache into OUT_DIR/i18n_data.rs.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

fn generate_table_from_json(
    map: &serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> String {
    let mut keys: Vec<&String> = map.keys().collect();
    keys.sort();
    let mut lines = Vec::new();
    lines.push(format!("pub static {name}: &[(&str, &str)] = &["));
    for k in keys {
        let v = map.get(k).and_then(|v| v.as_str()).unwrap_or_default();
        let escaped_k = serde_json::to_string(k).unwrap();
        let escaped_v = serde_json::to_string(v).unwrap();
        lines.push(format!("    ({escaped_k}, {escaped_v}),"));
    }
    lines.push("];".to_string());
    lines.join("\n")
}

fn fallback_from_cache(cache_path: &Path, dest_path: &Path) {
    let content = fs::read_to_string(cache_path).expect("failed to read i18n_cache.json");
    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("invalid i18n_cache.json");
    let en = parsed
        .get("en")
        .and_then(|v| v.as_object())
        .expect("missing en in cache");
    let zh = parsed
        .get("zh")
        .and_then(|v| v.as_object())
        .expect("missing zh in cache");

    let en_table = generate_table_from_json(en, "EN_US_TRANSLATIONS");
    let zh_table = generate_table_from_json(zh, "ZH_CN_TRANSLATIONS");

    let output = format!("// Generated from i18n_cache.json\n\n{en_table}\n\n{zh_table}\n");
    fs::write(dest_path, output).expect("failed to write i18n_data.rs");
}

fn main() {
    println!("cargo:rerun-if-changed=../../packages/ui/src/i18n/locales/en-US.ts");
    println!("cargo:rerun-if-changed=../../packages/ui/src/i18n/locales/zh-CN.ts");
    println!("cargo:rerun-if-changed=../../package.json");
    println!("cargo:rerun-if-changed=scripts/extract_i18n.ts");
    println!("cargo:rerun-if-changed=i18n_cache.json");

    let repo_pkg = Path::new("../../package.json");
    let version = if repo_pkg.exists() {
        if let Ok(content) = fs::read_to_string(repo_pkg) {
            serde_json::from_str::<serde_json::Value>(&content)
                .ok()
                .and_then(|v| {
                    v.get("version")
                        .and_then(|s| s.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| "3.14.3".to_string())
        } else {
            "3.14.3".to_string()
        }
    } else {
        "3.14.3".to_string()
    };
    println!("cargo:rustc-env=ZCODE_APP_VERSION={version}");

    let git_hash = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string());
    println!("cargo:rustc-env=ZCODE_GIT_COMMIT={git_hash}");

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let dest_path = out_dir.join("i18n_data.rs");
    let cache_path = PathBuf::from("i18n_cache.json");

    // Try deno first, then bun, then fallback to cache
    let deno_res = Command::new("deno")
        .args([
            "run",
            "--allow-read",
            "--allow-write",
            "scripts/extract_i18n.ts",
            dest_path.to_str().unwrap(),
        ])
        .status();

    let success = match deno_res {
        Ok(s) => s.success(),
        Err(_) => {
            let bun_res = Command::new("bun")
                .args(["scripts/extract_i18n.ts", dest_path.to_str().unwrap()])
                .status();
            match bun_res {
                Ok(s) => s.success(),
                Err(_) => false,
            }
        }
    };

    if !success {
        if cache_path.exists() {
            println!("cargo:warning=deno/bun not found or failed, falling back to i18n_cache.json");
            fallback_from_cache(&cache_path, &dest_path);
        } else {
            panic!("Could not extract i18n and no i18n_cache.json found");
        }
    }
}
