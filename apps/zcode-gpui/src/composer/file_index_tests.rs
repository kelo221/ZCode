use super::*;
use std::fs;

struct Tree(PathBuf);

impl Tree {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("zcode_fi_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn file(&self, rel: &str) -> &Self {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x").unwrap();
        self
    }

    fn rels(&self) -> Vec<String> {
        build_now(&self.0).into_iter().map(|h| h.rel).collect()
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn deep_files_are_indexed() {
    let t = Tree::new("deep");
    t.file("Assets/Scripts/Game/UI/Views/Deep/PlayerView.cs");
    assert!(
        t.rels()
            .contains(&"Assets/Scripts/Game/UI/Views/Deep/PlayerView.cs".to_string())
    );
}

#[test]
fn unity_noise_and_meta_files_are_skipped() {
    let t = Tree::new("unity");
    t.file("Library/Artifacts/x.bin")
        .file("Temp/a.tmp")
        .file("Assets/Player.cs")
        .file("Assets/Player.cs.meta");
    let rels = t.rels();
    assert!(rels.contains(&"Assets/Player.cs".to_string()));
    assert!(
        rels.iter()
            .all(|r| !r.starts_with("Library") && !r.starts_with("Temp"))
    );
    assert!(rels.iter().all(|r| !r.ends_with(".meta")));
}

#[test]
fn gitignore_and_zcodeignore_are_honored() {
    let t = Tree::new("ignore");
    t.file("keep.rs").file("build/out.o").file("secret/key.txt");
    fs::write(t.0.join(".gitignore"), "build/\n").unwrap();
    fs::write(t.0.join(".zcodeignore"), "secret/\n").unwrap();
    let rels = t.rels();
    assert!(rels.contains(&"keep.rs".to_string()));
    assert!(
        rels.iter()
            .all(|r| !r.starts_with("build") && !r.starts_with("secret"))
    );
}

#[test]
fn folders_are_entries_with_a_trailing_slash() {
    let t = Tree::new("dirs");
    t.file("src/main.rs");
    assert!(t.rels().contains(&"src/".to_string()));
}

#[test]
fn ranking_prefers_file_names_and_word_starts() {
    let t = Tree::new("rank");
    t.file("Assets/Scripts/PlayerMovement.cs")
        .file("Assets/player/README.md")
        .file("Assets/Art/upmost.png");
    let entries = walk(&t.0);
    let first = |q: &str| rank(&entries, q, 1)[0].name.clone();
    assert_eq!(first("pm"), "PlayerMovement.cs", "camel humps win");
    assert_eq!(first("playermov"), "PlayerMovement.cs");
    assert_eq!(
        first("player/read"),
        "README.md",
        "queries can span folders"
    );
}

#[test]
fn scoring_matches_desktop_order() {
    let t = Tree::new("score");
    t.file("PlayerManager.cs")
        .file("lib/manager.rs")
        .file("zz/pm_gr.txt");
    let entries = walk(&t.0);
    let names: Vec<String> = rank(&entries, "pmgr", 10)
        .into_iter()
        .map(|h| h.name)
        .collect();
    assert!(names.contains(&"PlayerManager.cs".to_string()), "{names:?}");
    let first = rank(&entries, "manager", 1);
    assert_eq!(first[0].name, "manager.rs", "prefix beats substring");
    assert!(rank(&entries, "qqq", 10).is_empty());
}

#[test]
fn search_builds_in_the_background_then_returns_hits() {
    let t = Tree::new("search");
    t.file("a/b/c/d/e/f/target_file.txt");
    let mut found = Search::default();
    for _ in 0..300 {
        found = search(&t.0, "target_file", 5);
        if !found.indexing {
            break;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!found.indexing);
    assert_eq!(found.hits[0].rel, "a/b/c/d/e/f/target_file.txt");
}

#[test]
fn large_indexes_rank_in_the_background_and_keep_old_hits_meanwhile() {
    let entries: Vec<Entry> = (0..SYNC_RANK_MAX + 1)
        .map(|i| {
            let rel = format!("dir{}/file{i}.txt", i % 50);
            Entry {
                lower_rel: rel.clone(),
                hit: FileHit {
                    name: format!("file{i}.txt"),
                    rel,
                    is_dir: false,
                },
            }
        })
        .collect();
    let index = Index {
        root: PathBuf::from("synthetic"),
        shared: Arc::new(Mutex::new(Shared {
            entries: Some(Arc::new(entries)),
            built_at: Some(Instant::now()),
            ..Default::default()
        })),
        building: Arc::new(AtomicBool::new(false)),
        used_at: Instant::now(),
        _watcher: None,
    };
    let settle = |q: &str| {
        for _ in 0..500 {
            let s = index.search(q, 3);
            if !s.searching {
                return s;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        panic!("ranking never finished");
    };
    let first = index.search("file17.txt", 3);
    assert!(first.searching && first.hits.is_empty());
    assert!(index.pending());
    assert_eq!(settle("file17.txt").hits[0].name, "file17.txt");
    assert!(!index.pending());
    let next = index.search("file42.txt", 3);
    assert!(next.searching);
    assert_eq!(
        next.hits[0].name, "file17.txt",
        "previous hits stay visible"
    );
    assert_eq!(settle("file42.txt").hits[0].name, "file42.txt");
}

#[test]
fn a_missed_query_rebuilds_once_and_finds_the_new_file() {
    let t = Tree::new("miss");
    t.file("old.txt");
    let wait = |q: &str| {
        for _ in 0..300 {
            let s = search(&t.0, q, 5);
            if !s.indexing && !s.hits.is_empty() {
                return s.hits;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        vec![]
    };
    assert!(!wait("old").is_empty());
    t.file("brand_new.txt");
    assert_eq!(wait("brand_new")[0].name, "brand_new.txt");
}
