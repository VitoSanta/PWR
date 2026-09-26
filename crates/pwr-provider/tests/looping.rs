//! Telling a reply that goes round in circles from one that is merely long.

use pwr_provider::looping;

/// The start of a real reply: Qwen3.6-35B-A3B, reasoning off, 2026-09-26,
/// restating why `-2^2` parsed wrong until the turn's time was gone.
const CAPTURED: &str = include_str!("fixtures/looping-reply-2026-09-26.txt");

#[test]
fn the_captured_loop_is_caught_before_its_end() {
    let caught = (2_500..CAPTURED.len())
        .step_by(256)
        .find(|end| CAPTURED.is_char_boundary(*end) && looping(&CAPTURED[..*end]).is_some())
        .expect("the loop was never caught");
    assert!(caught < 8_000, "caught only after {caught} bytes");
}

#[test]
fn a_reply_restating_the_same_paragraphs_is_a_loop() {
    let paragraph = "The fix: `_unary` should handle `-` by calling `_power` for the base, and `_power` should call `_unary` for the base.\n";
    let mut text = String::from("Looking at the parser first.\n");
    for round in 0..6 {
        text.push_str(paragraph);
        text.push_str(&format!(
            "I think the issue is, round {round}, the same thought said a little differently.\n"
        ));
    }
    while text.len() < 3_000 {
        text.push_str(paragraph);
    }
    let said = looping(&text).expect("a loop");
    assert!(said.contains("came back"), "{said}");
}

#[test]
fn a_loop_without_line_breaks_is_found_too() {
    let text = "and then the base is the unary node so power handles it ".repeat(80);
    assert!(looping(&text).is_some());
}

#[test]
fn ordinary_long_prose_and_short_answers_are_not_loops() {
    let prose: String = (0..120)
        .map(|n| {
            format!(
                "Step {n}: read file number {n}, which defines f{n} and calls g{}.\n",
                n * 7 % 13
            )
        })
        .collect();
    assert!(prose.len() > 6_000);
    assert!(looping(&prose).is_none(), "{:?}", looping(&prose));
    assert!(looping("Done. Done. Done. Done.").is_none());
    let summary =
        "All 35 tests pass.\n\n- `sheet/__init__.py`: the parser, evaluator and CSV\n".repeat(2);
    assert!(looping(&summary).is_none());
}
