//! The ladder measured allocation, and said so in its own documents.
//!
//! "A ladder of `num_ctx` values with a fixed short prompt measures
//! allocation, not occupancy: it establishes that a tier can be served, not
//! what a full context costs."

use pwr_orchestrator::occupancy_prompt;

#[test]
fn the_prompt_actually_fills_the_tier_it_measures() {
    for tier in [2_048u32, 8_192, 32_768] {
        let prompt = occupancy_prompt(tier, 1);
        let estimated = prompt.len() / 4;
        let target = tier as usize * 3 / 4;
        // Within a tenth of the intended share: the estimate is characters
        // over four and does not need to be better than that.
        assert!(
            estimated * 10 >= target * 9,
            "tier {tier}: filled {estimated} of a target {target}"
        );
        // And never past the tier, or the sample measures a refusal rather
        // than a cost.
        assert!(estimated < tier as usize, "tier {tier} would overflow");
    }
}

/// A tier where the needle comes back held the context. One where it does not
/// was allocated and then not used -- which on a deployment that truncates
/// silently is every tier, and is exactly what a short prompt cannot see.
#[test]
fn the_prompt_carries_a_needle_at_the_start_and_asks_for_it_at_the_end() {
    let prompt = occupancy_prompt(8_192, 1);
    let needle = "PLUM-7391";
    let first = prompt.find(needle).expect("no needle");
    assert!(
        first < prompt.len() / 10,
        "the needle is not near the start, so recalling it proves less"
    );
    assert!(
        prompt.trim_end().ends_with("reply NONE."),
        "the prompt does not ask for the needle back"
    );
}

/// A run of identical tokens compresses in ways a real prompt does not, and
/// would understate the cost this is trying to measure.
#[test]
fn the_filler_is_not_one_word_repeated() {
    let prompt = occupancy_prompt(4_096, 1);
    let lines: Vec<&str> = prompt.lines().skip(1).take(20).collect();
    let distinct: std::collections::BTreeSet<&&str> = lines.iter().collect();
    assert!(distinct.len() > 1, "the filler is a single repeated line");
}

#[test]
fn a_tiny_tier_still_produces_a_usable_prompt() {
    let prompt = occupancy_prompt(64, 1);
    assert!(prompt.contains("PLUM-7391"));
    assert!(prompt.contains("NONE"));
}

/// One prompt sent four times measures a cache, not a tier.
///
/// Measured under harness v5 on `glm-4.7-flash:q8_0`: the warm-up evaluated
/// 13,174 prompt tokens in 79 seconds and the three counted repetitions, sent
/// the identical prompt, in 0.07 seconds each -- so the tier was admitted on a
/// median first token of 98 ms that no cold prompt of that size gets near. A
/// prefix cache reuses everything up to the first differing token, so what is
/// held here is how early two samples part, not merely that they differ.
#[test]
fn no_two_samples_share_a_prefix_a_cache_could_reuse() {
    let samples = [
        occupancy_prompt(8_192, 0),
        occupancy_prompt(8_192, 1),
        occupancy_prompt(8_192, 2),
        occupancy_prompt(8_192, 3),
        occupancy_prompt(16_384, 1),
    ];
    for (i, a) in samples.iter().enumerate() {
        for b in &samples[i + 1..] {
            let shared = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
            assert!(
                shared < 32,
                "two samples share their first {shared} characters: {:?}",
                &a[..shared.min(80)]
            );
        }
    }
}
