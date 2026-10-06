use std::path::Path;

fn check_directory(path: &Path, violations: &mut Vec<String>) {
    for entry in std::fs::read_dir(path).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            check_directory(&path, violations);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            let lines = std::fs::read_to_string(&path).unwrap().lines().count();
            if lines > 400 {
                violations.push(format!("{}: {lines} lines", path.display()));
            }
        }
    }
}

#[test]
fn rust_sources_stay_within_400_lines() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut violations = vec![];
    for directory in ["src", "tests"] {
        check_directory(&root.join(directory), &mut violations);
    }
    assert!(
        violations.is_empty(),
        "Rust source cap exceeded:\n{}",
        violations.join("\n")
    );
}
