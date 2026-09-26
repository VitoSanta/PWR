use semver_lite::Version;

fn v(text: &str) -> Version {
    text.parse().unwrap_or_else(|e| panic!("{text}: {e}"))
}

#[test]
fn parses_and_displays() {
    assert_eq!(v("1.2.3").to_string(), "1.2.3");
    assert_eq!(v("v1.2.3").to_string(), "1.2.3");
    assert_eq!(v("1.0.0-alpha.1+build.5").to_string(), "1.0.0-alpha.1+build.5");
    assert_eq!(v("0.0.0").to_string(), "0.0.0");
}

#[test]
fn rejects_what_is_not_a_version() {
    for bad in ["", "1", "1.2", "1.2.3.4", "01.2.3", "1.02.3", "1.2.3-", "1.2.3-01", "1.2.3+", "a.b.c", "1.2.3-al pha", "-1.2.3"] {
        assert!(bad.parse::<Version>().is_err(), "{bad:?} parsed");
    }
}

#[test]
fn orders_by_semver_precedence() {
    let ordered = [
        "1.0.0-alpha", "1.0.0-alpha.1", "1.0.0-alpha.beta", "1.0.0-beta", "1.0.0-beta.2",
        "1.0.0-beta.11", "1.0.0-rc.1", "1.0.0", "1.0.1", "1.1.0", "2.0.0", "10.0.0",
    ];
    for pair in ordered.windows(2) {
        assert!(v(pair[0]) < v(pair[1]), "{} < {}", pair[0], pair[1]);
    }
}

#[test]
fn build_metadata_is_ignored_for_equality() {
    assert_eq!(v("1.2.3+a"), v("1.2.3+b"));
    assert_eq!(v("1.2.3+a").cmp(&v("1.2.3")), std::cmp::Ordering::Equal);
}
