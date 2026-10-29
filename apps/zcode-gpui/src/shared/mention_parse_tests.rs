use super::*;

fn kinds(text: &str) -> Vec<(MentionKind, String)> {
    parse_mentions(text)
        .into_iter()
        .map(|m| (m.kind, m.label))
        .collect()
}

#[test]
fn links_map_to_desktop_categories() {
    let text = concat!(
        r"see [#Spawn agents](#sess_3783a0b5-fa) and [$architecture-governance](C:\\Users\\SKILL.md) ",
        "[main.rs](./src/main.rs) [src](./src/) [@Docs](plugin://docs@official)"
    );
    assert_eq!(
        kinds(text),
        vec![
            (MentionKind::Session, "Spawn agents".into()),
            (MentionKind::Skill, "architecture-governance".into()),
            (MentionKind::File, "main.rs".into()),
            (MentionKind::Directory, "src".into()),
            (MentionKind::Plugin, "Docs".into()),
        ]
    );
}

#[test]
fn raw_ranges_cover_the_whole_link() {
    let text = "a [#T](#sess_x) b";
    let m = &parse_mentions(text)[0];
    assert_eq!(&text[m.raw.clone()], "[#T](#sess_x)");
}

#[test]
fn escaped_label_and_angle_destination() {
    let m = &parse_mentions(r"[a\]b](<./x y.md>)")[0];
    assert_eq!((m.kind, m.label.as_str()), (MentionKind::File, "a]b"));
}

#[test]
fn bare_tokens_need_whitespace_boundaries() {
    assert_eq!(
        kinds("$review then /compact @explore #sess_abc"),
        vec![
            (MentionKind::Skill, "review".into()),
            (MentionKind::Command, "compact".into()),
            (MentionKind::Subagent, "explore".into()),
            (MentionKind::Session, "sess_abc".into()),
        ]
    );
    assert!(kinds("mail a@b.com, path /home/x/y and #notsess").is_empty());
}

#[test]
fn plain_brackets_are_not_mentions() {
    assert!(kinds("[not a link] (nor this)").is_empty());
}

#[test]
fn whole_mention_only_for_exact_chip_text() {
    assert!(whole_mention("[#T](#sess_x)").is_some());
    assert!(whole_mention("[#T](#sess_x) tail").is_none());
}

#[test]
fn skill_labels_are_title_cased_like_desktop() {
    assert_eq!(skill_display_label("code-review"), "Code Review");
    assert_eq!(skill_display_label("my_skill"), "My Skill");
    assert_eq!(skill_display_label("Already Nice"), "Already Nice");
    let m = whole_mention("[$architecture-governance](C:\\\\a\\\\SKILL.md)").unwrap();
    assert_eq!(m.display(), "⚡ Architecture Governance");
}

#[test]
fn display_replaces_links_and_reports_pill_ranges() {
    let (shown, pills) = display_with_pills("use [$code-review](./s/SKILL.md) on [#Fix](#sess_1)!");
    assert_eq!(shown, "use ⚡ Code Review on # Fix!");
    let texts: Vec<&str> = pills.iter().map(|r| &shown[r.clone()]).collect();
    assert_eq!(texts, ["⚡ Code Review", "# Fix"]);
}
