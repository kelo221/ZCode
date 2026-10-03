//! Tests for `composer/chips.rs`: atomic chip edits.

use super::*;

const CHIP: &str = "[main.rs](./src/main.rs)";

/// "see " + CHIP + " " + "now" with the chip registered.
fn sample() -> (String, ChipTable, Range<usize>) {
    let mut content = "see @ma now".to_string();
    let mut chips = ChipTable::default();
    // Replace "@ma " (bytes 4..8) the way autocomplete does.
    let caret = insert_chip(&mut content, &mut chips, 4..8, CHIP);
    assert_eq!(content, format!("see {CHIP} now"));
    assert_eq!(caret, 4 + CHIP.len() + 1);
    let chip = 4..4 + CHIP.len();
    assert_eq!(chips.ranges(), std::slice::from_ref(&chip));
    (content, chips, chip)
}

#[test]
fn backspace_at_chip_end_removes_whole_chip() {
    let (mut content, mut chips, chip) = sample();
    // Backspace = delete the char before the caret at chip.end.
    let r = edit(&mut content, &mut chips, chip.end - 1..chip.end, "");
    assert_eq!(r, chip);
    assert_eq!(content, "see  now");
    assert!(chips.ranges().is_empty());
}

#[test]
fn forward_delete_at_chip_start_removes_whole_chip() {
    let (mut content, mut chips, chip) = sample();
    edit(&mut content, &mut chips, chip.start..chip.start + 1, "");
    assert_eq!(content, "see  now");
    assert!(chips.ranges().is_empty());
}

#[test]
fn insertion_inside_chip_moves_to_chip_end() {
    let (mut content, mut chips, chip) = sample();
    let r = edit(
        &mut content,
        &mut chips,
        chip.start + 3..chip.start + 3,
        "X",
    );
    assert_eq!(r, chip.end..chip.end);
    assert_eq!(content, format!("see {CHIP}X now"));
    assert_eq!(chips.ranges(), std::slice::from_ref(&chip), "chip intact");
}

#[test]
fn ime_composition_range_cannot_split_a_chip() {
    let (mut content, mut chips, chip) = sample();
    // An IME replaces a range straddling the chip's tail and the space.
    let r = edit(&mut content, &mut chips, chip.end - 2..chip.end + 1, "日本");
    assert_eq!(r, chip.start..chip.end + 1);
    assert_eq!(content, "see 日本now");
    assert!(chips.ranges().is_empty(), "chip removed whole, never half");
}

#[test]
fn edits_before_a_chip_shift_it_and_after_leave_it() {
    let (mut content, mut chips, chip) = sample();
    edit(&mut content, &mut chips, 0..3, "look at");
    let moved = chip.start + 4..chip.end + 4;
    assert_eq!(chips.ranges(), std::slice::from_ref(&moved));
    assert_eq!(&content[moved.clone()], CHIP);
    let len = content.len();
    edit(&mut content, &mut chips, len..len, "!");
    assert_eq!(chips.ranges(), std::slice::from_ref(&moved));
}

#[test]
fn caret_navigation_steps_over_chips() {
    let (_content, chips, chip) = sample();
    assert_eq!(chips.chip_ending_at(chip.end), Some(chip.start));
    assert_eq!(chips.chip_starting_at(chip.start), Some(chip.end));
    assert_eq!(chips.snap(chip.start + 5), chip.end);
    assert_eq!(chips.snap(chip.start), chip.start);
}

#[test]
fn multibyte_text_keeps_char_boundaries() {
    let mut content = "你好 @x".to_string();
    let mut chips = ChipTable::default();
    let at = content.find('@').unwrap();
    let len = content.len();
    insert_chip(&mut content, &mut chips, at..len, "$skill");
    assert_eq!(content, "你好 $skill ");
    assert_eq!(utf16_of(&content, at), 3);
    let chip = chips.ranges()[0].clone();
    edit(&mut content, &mut chips, chip.end - 1..chip.end, "");
    assert_eq!(content, "你好  ");
}
