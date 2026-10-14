//! The declared profiles, checked as data rather than trusted as configuration.

use pwr_domain::{ModelProfile, ParameterSource};

fn declared() -> Vec<ModelProfile> {
    #[derive(serde::Deserialize)]
    struct File {
        profiles: Vec<ModelProfile>,
    }
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../strategies/models.json"
    ))
    .expect("strategies/models.json");
    serde_json::from_slice::<File>(&bytes).unwrap().profiles
}

#[test]
fn every_profile_is_coherent_and_explains_itself() {
    let profiles = declared();
    // The Gemma 4 packs: 12B, 26B-A4B at 4 bits, QAT 4 bits and 6 bits, 31B.
    assert_eq!(profiles.len(), 5);
    for profile in &profiles {
        assert!(
            profile.context.is_coherent(),
            "{} has contradictory context sizes",
            profile.model_selector
        );
        // A profile without a reason is configuration nobody can argue with.
        assert!(
            profile.provenance.len() > 60,
            "{} has no usable provenance",
            profile.model_selector
        );
        assert!(!profile.sampling.is_empty(), "{}", profile.model_selector);
    }
}

/// The product targets large repositories, so a deployment that cannot serve
/// the required context does not qualify however it scores. That is a
/// selection criterion, not a tuning knob, and it is visible in the data.
#[test]
fn every_qualifying_deployment_is_allocated_the_full_ceiling() {
    const REQUIRED: u32 = 262_144;
    let profiles = declared();
    for profile in &profiles {
        if profile.context.maximum >= REQUIRED {
            assert_eq!(
                profile.context.default, profile.context.maximum,
                "{} qualifies but is allocated less than it can serve",
                profile.model_selector
            );
            // The rule is the most the model and the host allow: the
            // default is the model's maximum, and the minimum is only the
            // floor a smaller host may fall back to, never a demand. A
            // minimum at the maximum would clamp a window the hardware
            // lowered back up past what it can hold.
            assert!(profile.context.minimum <= profile.context.default);
            assert_eq!(profile.context_source, ParameterSource::PwrOverride);
        } else {
            // Below the requirement, and the profile does not pretend
            // otherwise by raising a ceiling the tag cannot serve.
            assert!(profile.context.default <= profile.context.maximum);
        }
    }
    let qualifying = profiles
        .iter()
        .filter(|p| p.context.maximum >= REQUIRED)
        .count();
    assert_eq!(qualifying, 5);
}

/// Every Gemma pack samples as Google advises for all use cases, and says so.
#[test]
fn every_gemma_pack_samples_as_google_advises() {
    for profile in declared() {
        let options = profile.sampling_options();
        assert_eq!(
            options["temperature"],
            serde_json::json!(1.0),
            "{}",
            profile.model_selector
        );
        assert_eq!(
            options["top_p"],
            serde_json::json!(0.95),
            "{}",
            profile.model_selector
        );
        assert_eq!(
            options["top_k"],
            serde_json::json!(64),
            "{}",
            profile.model_selector
        );
        assert_eq!(
            profile.sampling["temperature"].source,
            ParameterSource::OfficialModelCard
        );
    }
}
