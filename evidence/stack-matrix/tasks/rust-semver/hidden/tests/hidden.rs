use semver_lite::{Range, Version};

fn matches(range: &str, version: &str) -> bool {
    range.parse::<Range>().unwrap().matches(&version.parse::<Version>().unwrap())
}

#[test]
fn hidden_numeric_prerelease_is_below_alphanumeric() {
    assert!("1.0.0-9".parse::<Version>().unwrap() < "1.0.0-a".parse::<Version>().unwrap());
    assert!("1.0.0-alpha.9".parse::<Version>().unwrap() < "1.0.0-alpha.10".parse::<Version>().unwrap());
}

#[test]
fn hidden_x_with_star_and_tilde_major_minor_zero() {
    assert!(matches("1.2.*", "1.2.0"));
    assert!(matches("x", "0.0.0"));
    assert!(matches("~0.2", "0.2.5"));
    assert!(!matches("~0.2", "0.3.0"));
}

#[test]
fn hidden_every_comparator_of_a_set_must_hold() {
    assert!(!matches(">=1.0.0 <1.5.0 >1.4.0", "1.3.0"));
    assert!(matches(">=1.0.0 <1.5.0 >1.4.0", "1.4.5"));
}

#[test]
fn hidden_prerelease_named_in_the_other_set_does_not_count() {
    assert!(!matches(">=1.0.0-alpha <1.0.0 || >=2.0.0", "2.0.0-beta"));
    assert!(matches(">=1.0.0-alpha <1.0.0 || >=2.0.0", "1.0.0-beta"));
}

#[test]
fn hidden_errors_are_std_errors_with_messages() {
    let error = "1.2".parse::<Version>().unwrap_err();
    let boxed: Box<dyn std::error::Error> = Box::new(error);
    assert!(!boxed.to_string().is_empty());
}

#[test]
fn hidden_large_numbers() {
    let v: Version = "18446744073709551615.0.0".parse().unwrap();
    assert_eq!(v.to_string(), "18446744073709551615.0.0");
    assert!("18446744073709551616.0.0".parse::<Version>().is_err());
}
