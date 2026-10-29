use super::*;
use crate::composer::chips::{self, ChipTable};

fn composer_with_chip() -> (String, ChipTable) {
    let mut content = String::from("hi ");
    let mut table = ChipTable::default();
    let end = content.len();
    chips::insert_chip(
        &mut content,
        &mut table,
        end..end,
        "[$code-review](./s/SKILL.md)",
    );
    content.push_str("ok");
    (content, table)
}

#[test]
fn chip_shows_its_pill_text() {
    let (content, table) = composer_with_chip();
    let d = ChipDisplay::new(&content, &table);
    assert_eq!(d.text, "hi ⚡ Code Review ok");
    let pills: Vec<&str> = d.pills().map(|r| &d.text[r]).collect();
    assert_eq!(pills, ["⚡ Code Review"]);
}

#[test]
fn offsets_round_trip_at_chip_edges_and_after() {
    let (content, table) = composer_with_chip();
    let d = ChipDisplay::new(&content, &table);
    let chip = table.iter().next().unwrap().clone();
    let pill = d.pills().next().unwrap();
    assert_eq!(d.to_display(chip.start), pill.start);
    assert_eq!(d.to_display(chip.end), pill.end);
    assert_eq!(d.to_display(content.len()), d.text.len());
    assert_eq!(d.to_raw(d.text.len()), content.len());
    assert_eq!(d.to_raw(pill.end + 1), chip.end + 1);
    assert_eq!(d.to_raw(1), 1);
}

#[test]
fn a_click_inside_the_pill_snaps_to_the_nearer_edge() {
    let (content, table) = composer_with_chip();
    let d = ChipDisplay::new(&content, &table);
    let chip = table.iter().next().unwrap().clone();
    let pill = d.pills().next().unwrap();
    assert_eq!(d.to_raw(pill.start + 1), chip.start);
    assert_eq!(d.to_raw(pill.end - 1), chip.end);
}

#[test]
fn backspace_after_a_chip_removes_the_whole_chip() {
    let (mut content, mut table) = composer_with_chip();
    let chip = table.iter().next().unwrap().clone();
    chips::edit(&mut content, &mut table, chip.end - 1..chip.end, "");
    assert_eq!(content, "hi  ok");
    assert_eq!(ChipDisplay::new(&content, &table).text, "hi  ok");
}

#[test]
fn text_without_chips_is_shown_as_is() {
    let d = ChipDisplay::new("[$x](y) typed by hand", &ChipTable::default());
    assert_eq!(d.text, "[$x](y) typed by hand");
    assert_eq!(d.to_raw(4), 4);
}

#[test]
fn overlapping_highlights_merge_into_disjoint_pieces() {
    let a = gpui::HighlightStyle {
        font_weight: Some(gpui::FontWeight::BOLD),
        ..Default::default()
    };
    let b = gpui::HighlightStyle {
        fade_out: Some(0.5),
        ..Default::default()
    };
    let merged = merge_highlights(vec![(0..4, a), (2..6, b)]);
    let ranges: Vec<_> = merged.iter().map(|(r, _)| r.clone()).collect();
    assert_eq!(ranges, [0..2, 2..4, 4..6]);
    assert_eq!(merged[1].1.font_weight, Some(gpui::FontWeight::BOLD));
    assert_eq!(merged[1].1.fade_out, Some(0.5));
}
