use semver_lite::{max_satisfying, Range, Version};

fn matches(range: &str, version: &str) -> bool {
    let r: Range = range.parse().unwrap_or_else(|e| panic!("{range}: {e}"));
    r.matches(&version.parse::<Version>().unwrap())
}

#[test]
fn operators() {
    assert!(matches(">=1.2.3", "1.2.3"));
    assert!(!matches(">1.2.3", "1.2.3"));
    assert!(matches("<2.0.0", "1.9.9"));
    assert!(matches("<=2.0.0", "2.0.0"));
    assert!(matches("=1.2.3", "1.2.3+build"));
    assert!(matches("1.2.3", "1.2.3"));
    assert!(!matches("1.2.3", "1.2.4"));
    assert!(matches(">= 1.0.0  < 2.0.0", "1.5.0"));
}

#[test]
fn caret_and_tilde() {
    assert!(matches("^1.2.3", "1.9.0"));
    assert!(!matches("^1.2.3", "2.0.0"));
    assert!(!matches("^1.2.3", "1.2.2"));
    assert!(matches("^0.2.3", "0.2.9"));
    assert!(!matches("^0.2.3", "0.3.0"));
    assert!(matches("^0.0.3", "0.0.3"));
    assert!(!matches("^0.0.3", "0.0.4"));
    assert!(matches("~1.2.3", "1.2.9"));
    assert!(!matches("~1.2.3", "1.3.0"));
    assert!(matches("~1.2", "1.2.0"));
    assert!(matches("~1", "1.9.9"));
    assert!(!matches("~1", "2.0.0"));
}

#[test]
fn x_ranges_and_hyphens() {
    assert!(matches("1.2.x", "1.2.7"));
    assert!(!matches("1.2.x", "1.3.0"));
    assert!(matches("1.x", "1.99.0"));
    assert!(matches("1", "1.0.0"));
    assert!(matches("*", "3.4.5"));
    assert!(matches("", "0.0.1"));
    assert!(matches("1.2.3 - 2.3.4", "2.3.4"));
    assert!(!matches("1.2.3 - 2.3.4", "2.3.5"));
}

#[test]
fn alternatives() {
    assert!(matches("^1.0.0 || ^3.0.0", "3.1.0"));
    assert!(!matches("^1.0.0 || ^3.0.0", "2.0.0"));
    assert!(matches(">=1.0.0 <1.1.0 || >=2.0.0", "2.5.0"));
}

#[test]
fn prereleases_only_where_named() {
    assert!(matches(">=1.2.3-alpha", "1.2.3-beta"));
    assert!(!matches(">=1.2.3-alpha", "1.2.4-beta"));
    assert!(matches(">=1.2.3-alpha", "1.2.4"));
    assert!(!matches("*", "1.0.0-rc.1"));
    assert!(!matches("^1.0.0", "1.1.0-rc.1"));
    assert!(matches("^1.1.0-rc.1", "1.1.0-rc.2"));
}

#[test]
fn bad_ranges_are_errors() {
    for bad in [">=", "^1.2", ">1.x", "1.2.3 -", "||", "1.2.3 - 2", "~a", "=>1.0.0"] {
        assert!(bad.parse::<Range>().is_err(), "{bad:?} parsed");
    }
}

#[test]
fn highest_satisfying() {
    let versions: Vec<Version> = ["1.0.0", "1.4.2", "1.10.0", "2.0.0", "1.11.0-beta"]
        .iter()
        .map(|t| t.parse().unwrap())
        .collect();
    let best = max_satisfying(&versions, &"^1.0.0".parse().unwrap());
    assert_eq!(best.map(ToString::to_string), Some("1.10.0".to_string()));
    assert!(max_satisfying(&versions, &">3.0.0".parse().unwrap()).is_none());
}
