//! Retrieval ranking and its limits.

use pwr_repo::{Excerpt, index, retrieve};
use std::fs;
use std::path::Path;

fn workspace(files: &[(&str, &str)]) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for (path, contents) in files {
        let full = root.path().join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, contents).unwrap();
    }
    root
}

fn ranked(root: &Path, query: &str) -> Vec<Excerpt> {
    let index = index(root).unwrap();
    retrieve(root, &index, query, 10, 100_000).unwrap()
}

#[test]
fn a_file_defining_the_named_symbol_outranks_one_merely_mentioning_it() {
    let root = workspace(&[
        (
            "src/checksum.rs",
            "pub fn checksum_of(b: &[u8]) -> u32 { 0 }\n",
        ),
        (
            "src/notes.rs",
            "// checksum_of is described here but not defined\npub fn other() {}\n",
        ),
    ]);
    let hits = ranked(root.path(), "fix the checksum_of function");
    assert_eq!(hits[0].path, "src/checksum.rs");
    assert!(hits[0].rationale.contains("defines checksum_of"));
}

/// The rationale is what makes a wrong retrieval diagnosable instead of
/// mysterious.
#[test]
fn every_excerpt_says_why_it_was_chosen_and_what_it_cost() {
    let root = workspace(&[("src/parser.rs", "pub fn parse_port(t: &str) {}\n")]);
    let hits = ranked(root.path(), "parse_port returns the wrong value");
    let hit = &hits[0];
    assert!(hit.rationale.starts_with("score "));
    assert!(hit.estimated_tokens > 0);
    assert!(hit.first_line >= 1 && hit.last_line >= hit.first_line);
    assert!(!hit.content_hash.is_empty());
}

/// An edit is guarded against the whole file, so a retrieved fragment must
/// carry the whole file's hash rather than the fragment's.
#[test]
fn the_excerpt_hash_is_the_whole_file() {
    let body = "pub fn target() -> i32 { 1 }\n";
    let root = workspace(&[("src/a.rs", body)]);
    let hits = ranked(root.path(), "target");
    assert_eq!(hits[0].content_hash, pwr_domain::hash_bytes(body));
}

/// A window centred on the top of the file would miss the evidence in a long
/// one.
#[test]
fn an_excerpt_is_centred_on_the_matching_line() {
    let mut body: String = (1..=200).map(|i| format!("// filler {i}\n")).collect();
    body.push_str("pub fn needle_function() -> i32 { 7 }\n");
    let root = workspace(&[("src/long.rs", &body)]);
    let hits = ranked(root.path(), "needle_function is wrong");
    let hit = &hits[0];
    assert!(hit.content.contains("needle_function"));
    assert!(hit.first_line > 150, "window started at {}", hit.first_line);
    assert!(hit.last_line - hit.first_line < 40);
}

#[test]
fn a_token_budget_stops_retrieval_rather_than_being_exceeded() {
    let big: String = (1..=400)
        .map(|i| format!("pub fn target{i}() {{}}\n"))
        .collect();
    let root = workspace(&[("src/a.rs", &big), ("src/b.rs", &big)]);
    let index = index(root.path()).unwrap();
    let hits = retrieve(root.path(), &index, "target", 10, 40).unwrap();
    assert!(hits.iter().map(|h| h.estimated_tokens).sum::<usize>() <= 40);
}

#[test]
fn a_query_of_only_common_words_retrieves_nothing() {
    let root = workspace(&[("src/a.rs", "pub fn thing() {}\n")]);
    assert!(ranked(root.path(), "what should the and for this").is_empty());
}

#[test]
fn a_file_matching_nothing_is_not_returned() {
    let root = workspace(&[
        ("src/relevant.rs", "pub fn parse_port() {}\n"),
        ("src/unrelated.rs", "pub fn quite_different() {}\n"),
    ]);
    let hits = ranked(root.path(), "parse_port");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "src/relevant.rs");
}

/// Ignored files are not in the index, so they cannot be retrieved either --
/// the secret-leak case, one layer further out.
#[test]
fn an_ignored_file_is_never_retrieved() {
    let root = workspace(&[
        (".gitignore", ".env\n"),
        (".env", "API_KEY=live-secret-token\n"),
        ("src/a.rs", "pub fn api_key_loader() {}\n"),
    ]);
    let hits = ranked(root.path(), "api_key");
    assert!(hits.iter().all(|h| h.path != ".env"));
    assert!(!hits.iter().any(|h| h.content.contains("live-secret-token")));
}

/// A document is retrieved by section, and the section is the one whose
/// heading and body answer the request -- not a window of lines around the
/// densest match somewhere in a long file.
///
/// Measured 2026-09-22 on twelve documentation requests (backlog C.22): the
/// file ranking found the answering section for one of the twelve, because
/// one long document outranked the short one that answered.
#[test]
fn a_document_is_retrieved_by_the_section_that_answers() {
    let long = format!(
        "# Log\n\n## An early campaign\n\n{}\n\n## Another campaign\n\n{}\n",
        "The sandbox was mentioned here in passing while measuring runs. ".repeat(40),
        "More prose about runs, measurements and the sandbox again. ".repeat(40),
    );
    let root = workspace(&[
        ("docs/log.md", &long),
        (
            "docs/security.md",
            "# Security\n\n## What the boundary is\n\nThe sandbox confines every command to the \
             workspace, and network access is refused unless it was granted.\n\n## Reporting\n\n\
             Mail the maintainer.\n",
        ),
        ("src/lib.rs", "pub fn sandbox() {}\n"),
    ]);
    let hits = ranked(root.path(), "what does the sandbox confine commands to?");
    let best = hits.first().expect("a passage");
    assert_eq!(best.path, "docs/security.md", "{:?}", hits);
    assert!(
        best.rationale.contains("What the boundary is"),
        "{}",
        best.rationale
    );
    assert!(
        best.content.starts_with("## What the boundary is"),
        "{}",
        best.content
    );
    // Both halves of one document at most, so one file cannot fill the answer.
    let from_log = hits.iter().filter(|hit| hit.path == "docs/log.md").count();
    assert!(from_log <= 2, "{from_log}");
    // Code still competes: the symbol match is offered too.
    assert!(hits.iter().any(|hit| hit.path == "src/lib.rs"), "{hits:?}");
}

/// A semantic ranker reaches the section that answers a request in other
/// words, which the lexical ranking cannot: with a ranker every document is a
/// candidate, not only those that share a term with the request.
struct Prefers(&'static str);

impl pwr_repo::SectionRanker for Prefers {
    fn similarities(
        &mut self,
        _query: &str,
        sections: &[pwr_repo::SectionText<'_>],
    ) -> Option<Vec<f32>> {
        Some(
            sections
                .iter()
                .map(|section| {
                    if section.heading.contains(self.0) {
                        1.0
                    } else {
                        0.0
                    }
                })
                .collect(),
        )
    }
}

/// A ranker whose model is missing: retrieval must be exactly the lexical one.
struct Absent;

impl pwr_repo::SectionRanker for Absent {
    fn similarities(&mut self, _: &str, _: &[pwr_repo::SectionText<'_>]) -> Option<Vec<f32>> {
        None
    }
}

#[test]
fn a_semantic_ranker_reaches_a_section_in_other_words_and_its_absence_changes_nothing() {
    let root = workspace(&[
        (
            "docs/spec.md",
            "# Spec\n\n## Definition and initial user\n\nA harness for engineers on one \
             machine.\n\n## Glossary\n\nTerms.\n",
        ),
        (
            "docs/notes.md",
            "# Notes\n\n## Purpose of the project\n\nSee elsewhere.\n",
        ),
    ]);
    let index = pwr_repo::index(root.path()).unwrap();
    let question = "why does this project exist and who is it for";

    let lexical = retrieve(root.path(), &index, question, 5, 100_000).unwrap();
    assert!(
        !lexical
            .iter()
            .any(|hit| hit.rationale.contains("Definition and initial user")),
        "the lexical ranking should not reach it: {lexical:?}"
    );

    let mut ranker = Prefers("Definition");
    let fused =
        pwr_repo::retrieve_with(root.path(), &index, question, 5, 100_000, Some(&mut ranker))
            .unwrap();
    assert!(
        fused
            .iter()
            .any(|hit| hit.rationale.contains("Definition and initial user")),
        "{fused:?}"
    );
    assert!(
        fused.iter().any(|hit| hit.rationale.contains("meaning #1")),
        "{fused:?}"
    );

    let mut absent = Absent;
    let fallback =
        pwr_repo::retrieve_with(root.path(), &index, question, 5, 100_000, Some(&mut absent))
            .unwrap();
    let paths = |hits: &[pwr_repo::Excerpt]| {
        hits.iter()
            .map(|hit| (hit.path.clone(), hit.first_line))
            .collect::<Vec<_>>()
    };
    // Absent means lexical -- though every document was a candidate, only the
    // ones sharing a term can score, so the answer is the same.
    assert_eq!(paths(&fallback), paths(&lexical));
}
