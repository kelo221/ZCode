use super::*;

fn s(text: &str, query: &str) -> Option<i32> {
    let t: Vec<char> = text.chars().collect();
    let q: Vec<char> = query.to_lowercase().chars().collect();
    score(&t, &q)
}

#[test]
fn needs_every_character_in_order_case_insensitively() {
    assert!(s("PlayerManager.cs", "pmgr").is_some());
    assert!(s("PlayerManager.cs", "PLAYER").is_some());
    assert!(s("PlayerManager.cs", "rgmp").is_none());
    assert!(s("ab", "abc").is_none());
    assert_eq!(s("anything", ""), Some(0));
}

#[test]
fn camel_humps_beat_letters_inside_words() {
    assert!(s("PlayerManager.cs", "pm") > s("upmost.txt", "pm"));
}

#[test]
fn word_starts_after_separators_score_high() {
    assert!(s("player_manager.cs", "pm") > s("chipmunk.cs", "pm"));
    assert!(s("src/main.rs", "main") > s("domain.rs", "main"));
}

#[test]
fn a_consecutive_run_beats_scattered_boundary_hits() {
    assert!(s("main.rs", "main") > s("m_a_i_n.txt", "main"));
}

#[test]
fn the_best_alignment_is_found_not_the_first() {
    // Greedy left-to-right would take the `p`/`m` in "promo"; the camel humps score higher.
    assert!(s("promo/PlayerManager.cs", "pm") > s("promo/xyz.cs", "pm"));
}

#[test]
fn subsequence_precheck_matches_score() {
    assert!(is_subsequence("assets/scripts/playermanager.cs", "pmgr"));
    assert!(!is_subsequence("assets/scripts/playermanager.cs", "zzz"));
}
