//! Property tests for the invariants every persisted artifact depends on.
//!
//! These exercise the crate through its public contract, the way the store,
//! adapters and CLI see it.

use proptest::prelude::*;
use pwr_domain::*;
use std::collections::BTreeMap;

fn any_provenance() -> impl Strategy<Value = Provenance> {
    ("[a-z:/]{1,20}", "[a-f0-9]{8}").prop_map(|(source, hash)| Provenance {
        source,
        observed_at: now(),
        content_hash: hash,
    })
}

fn any_observation() -> impl Strategy<Value = Observation> {
    prop_oneof![
        any::<bool>().prop_map(|b| Observation::Observed(serde_json::json!(b))),
        "[a-z ]{0,40}".prop_map(|reason| Observation::Unknown { reason }),
    ]
}

fn any_deployment() -> impl Strategy<Value = DeploymentDescriptor> {
    (
        "[a-z]{1,10}",
        "https?://[a-z]{1,10}/",
        "[a-z0-9.:_-]{1,20}",
        proptest::collection::btree_map("[a-z]{1,5}", "[a-z]{1,5}", 0..3),
        proptest::option::of("[a-z]{1,8}"),
    )
        .prop_map(
            |(provider, endpoint, model_ref, backend_options, auth_ref)| DeploymentDescriptor {
                schema_version: SCHEMA_VERSION,
                id: new_id(),
                provider,
                endpoint,
                model_ref,
                backend_options,
                auth_ref,
            },
        )
}

/// Points that meet the default thresholds, so a profile built from them is
/// valid. Points that fail thresholds are exercised by their own properties.
fn any_stable_point() -> impl Strategy<Value = StablePoint> {
    (1u32..131_072, 3u32..10, 0.0f64..=1.0).prop_map(|(context_tokens, samples, _)| StablePoint {
        context_tokens,
        samples,
        success_rate: 1.0,
        median_first_token_ms: 1.0,
        generation_tokens_per_second: 1.0,
        variance: 0.0,
        memory_pressure_observed: false,
    })
}

fn calibration_with(points: Vec<StablePoint>, key: &str) -> CalibrationProfile {
    CalibrationProfile {
        schema_version: SCHEMA_VERSION,
        id: new_id(),
        compatibility_key: key.into(),
        model_digest: "digest".into(),
        deployment_fingerprint: "fingerprint".into(),
        harness_rev: "harness".into(),
        thresholds: CalibrationThresholds::default(),
        stable_points: points,
        raw_artifact_hashes: vec![],
        created_at: now(),
    }
}

fn execution_for(
    calibration: Option<&CalibrationProfile>,
    context_tokens: u32,
    reserve_tokens: u32,
    evidence: EvidenceLabel,
) -> ExecutionProfile {
    ExecutionProfile {
        schema_version: SCHEMA_VERSION,
        id: new_id(),
        strategy_id: new_id(),
        calibration_id: calibration.map(|c| c.id),
        context_tokens,
        reserve_tokens,
        concurrency: 1,
        budgets: serde_json::json!({
            "max_actions": 8,
            "edit_verify_cycles": 3,
            "context_retries": 1,
        }),
        rationale: "property test".into(),
        evidence,
        compatibility_key: calibration
            .map(|c| c.compatibility_key.clone())
            .unwrap_or_default(),
    }
}

proptest! {
    /// An `Observation` must never round-trip into the other variant. A collapse
    /// here would silently turn "we did not observe this" into "observed".
    #[test]
    fn observation_round_trips_without_changing_variant(observation in any_observation()) {
        let encoded = serde_json::to_string(&observation).unwrap();
        let decoded: Observation = serde_json::from_str(&encoded).unwrap();
        prop_assert_eq!(&observation, &decoded);
        match observation {
            Observation::Observed(_) => prop_assert!(encoded.contains("\"observed\"")),
            Observation::Unknown { .. } => prop_assert!(encoded.contains("\"unknown\"")),
        }
    }

    /// `skip_serializing_if` must not drop a payload that was present.
    #[test]
    fn model_chunk_round_trips_every_channel(
        content in "[a-z ]{0,30}",
        thinking in proptest::option::of("[a-z ]{1,30}"),
        calls in proptest::collection::vec("[a-z_]{1,12}", 0..4),
        generated_tokens in proptest::option::of(any::<u64>()),
        done in any::<bool>(),
    ) {
        let chunk = ModelChunk {
            content,
            thinking,
            metrics: generated_tokens.map(|generated_tokens| GenerationMetrics {
                generated_tokens: Some(generated_tokens),
                generation_duration_ns: Some(1_000_000_000),
                ..Default::default()
            }),
            tool_calls: calls
                .into_iter()
                .map(|name| ToolCall {
                    name,
                    arguments: serde_json::json!({"value": "ok"}),
                    id: None,
                })
                .collect(),
            done,
        };
        let decoded: ModelChunk =
            serde_json::from_str(&serde_json::to_string(&chunk).unwrap()).unwrap();
        prop_assert_eq!(&chunk, &decoded);
        prop_assert_eq!(chunk.tool_calls.len(), decoded.tool_calls.len());
        // Backend-reported counts must survive the round trip, or a calibration
        // artifact loses the numbers its rate was computed from.
        prop_assert_eq!(
            chunk.metrics.as_ref().and_then(|m| m.generated_tokens),
            decoded.metrics.as_ref().and_then(|m| m.generated_tokens)
        );
    }

    /// A rate is reported only when the backend gave enough to compute one.
    #[test]
    fn a_reported_rate_requires_both_a_count_and_a_duration(
        tokens in proptest::option::of(1u64..10_000),
        nanos in proptest::option::of(0u64..10_000_000_000),
    ) {
        let metrics = GenerationMetrics {
            generated_tokens: tokens,
            generation_duration_ns: nanos,
            ..Default::default()
        };
        let computable = tokens.is_some() && nanos.is_some_and(|n| n > 0);
        prop_assert_eq!(metrics.tokens_per_second().is_some(), computable);
    }

    #[test]
    fn model_definition_round_trips(
        digest in "[a-f0-9:]{4,20}",
        capabilities in proptest::collection::btree_map("[a-z_]{1,12}", any_observation(), 0..5),
        provenance in any_provenance(),
    ) {
        let definition = ModelDefinition {
            schema_version: SCHEMA_VERSION,
            id: new_id(),
            digest,
            family: None,
            quantization: None,
            capabilities,
            metadata: serde_json::json!({"model_info": {}}),
            provenance,
        };
        let decoded: ModelDefinition =
            serde_json::from_str(&serde_json::to_string(&definition).unwrap()).unwrap();
        prop_assert_eq!(definition, decoded);
    }

    /// The fingerprint is an invalidation key: it must depend on every field
    /// that changes what is being served, and on nothing else.
    #[test]
    fn fingerprint_ignores_identity_and_credentials(deployment in any_deployment()) {
        let mut other = deployment.clone();
        other.id = new_id();
        other.auth_ref = Some("rotated-credential-ref".into());
        prop_assert_eq!(deployment.fingerprint(), other.fingerprint());
    }

    #[test]
    fn fingerprint_changes_with_the_served_model(deployment in any_deployment()) {
        let mut other = deployment.clone();
        other.model_ref = format!("{}-different", deployment.model_ref);
        prop_assert_ne!(deployment.fingerprint(), other.fingerprint());
    }

    #[test]
    fn fingerprint_changes_with_backend_options(deployment in any_deployment()) {
        let mut other = deployment.clone();
        other
            .backend_options
            .insert("num_ctx".into(), "8192".into());
        prop_assert_ne!(deployment.fingerprint(), other.fingerprint());
    }

    #[test]
    fn hashing_is_deterministic_and_collision_sensitive(
        left in ".{0,64}",
        right in ".{0,64}",
    ) {
        prop_assert_eq!(hash_bytes(&left), hash_bytes(&left));
        prop_assert_eq!(left == right, hash_bytes(&left) == hash_bytes(&right));
    }

    #[test]
    fn identifiers_are_time_ordered_and_unique(count in 2usize..32) {
        let ids: Vec<Id> = (0..count).map(|_| new_id()).collect();
        for id in &ids {
            prop_assert_eq!(id.get_version_num(), 7);
        }
        let mut sorted = ids.clone();
        sorted.sort();
        // UUIDv7 is time-ordered, so generation order must be sort order.
        prop_assert_eq!(&ids, &sorted);
        sorted.dedup();
        prop_assert_eq!(sorted.len(), count);
    }

    /// Calibration requires repeated measurement. Fewer than three samples is
    /// not a stable point regardless of how good the numbers look.
    #[test]
    fn calibration_rejects_under_sampled_points(
        context_tokens in 1u32..131_072,
        samples in 0u32..3,
    ) {
        let profile = calibration_with(
            vec![StablePoint {
                context_tokens,
                samples,
                success_rate: 1.0,
                median_first_token_ms: 1.0,
                generation_tokens_per_second: 1.0,
                variance: 0.0,
                memory_pressure_observed: false,
            }],
            "key",
        );
        prop_assert!(profile.validate().is_err());
    }

    #[test]
    fn calibration_rejects_success_rates_outside_the_unit_interval(
        success_rate in prop_oneof![-100.0f64..-0.001, 1.001f64..100.0],
    ) {
        let profile = calibration_with(
            vec![StablePoint {
                context_tokens: 4096,
                samples: 3,
                success_rate,
                median_first_token_ms: 1.0,
                generation_tokens_per_second: 1.0,
                variance: 0.0,
                memory_pressure_observed: false,
            }],
            "key",
        );
        prop_assert!(profile.validate().is_err());
    }

    #[test]
    fn calibration_accepts_measured_points(points in proptest::collection::vec(any_stable_point(), 1..5)) {
        prop_assert!(calibration_with(points, "key").validate().is_ok());
    }

    /// Capacity must come from a measurement that succeeded, not merely one
    /// attempted at that size. A tier where every sample failed is a record of
    /// failure; authorising execution from it is inventing capacity.
    #[test]
    fn a_tier_that_failed_its_thresholds_authorises_nothing(
        context_tokens in 1u32..131_072,
        success_rate in 0.0f64..1.0,
    ) {
        let failed = StablePoint {
            context_tokens,
            samples: 3,
            success_rate,
            median_first_token_ms: 1.0,
            generation_tokens_per_second: 0.0,
            variance: 0.0,
            memory_pressure_observed: false,
        };
        let calibration = calibration_with(vec![failed], "key");
        // The profile itself must refuse to hold a point below its thresholds.
        prop_assert!(calibration.validate().is_err());
        let profile = execution_for(
            Some(&calibration),
            context_tokens,
            0,
            EvidenceLabel::Measured,
        );
        prop_assert!(profile.validate_against(Some(&calibration)).is_err());
    }

    /// Memory pressure during measurement disqualifies the point unless the
    /// profile declares that pressure is acceptable.
    #[test]
    fn a_tier_measured_under_memory_pressure_authorises_nothing(
        context_tokens in 1u32..131_072,
    ) {
        let pressured = StablePoint {
            context_tokens,
            samples: 3,
            success_rate: 1.0,
            median_first_token_ms: 1.0,
            generation_tokens_per_second: 1.0,
            variance: 0.0,
            memory_pressure_observed: true,
        };
        let calibration = calibration_with(vec![pressured], "key");
        prop_assert!(calibration.validate().is_err());
        let profile = execution_for(
            Some(&calibration),
            context_tokens,
            0,
            EvidenceLabel::Measured,
        );
        prop_assert!(profile.validate_against(Some(&calibration)).is_err());
    }

    /// MASTER_SPEC rule 4: capacity comes from evidence, never extrapolation.
    /// A measured profile is accepted only when a measured point covers the
    /// requested context.
    #[test]
    fn measured_context_never_exceeds_a_measured_stable_point(
        points in proptest::collection::vec(any_stable_point(), 1..5),
        context_tokens in 1u32..131_072,
    ) {
        let calibration = calibration_with(points, "key");
        let profile = execution_for(
            Some(&calibration),
            context_tokens,
            0,
            EvidenceLabel::Measured,
        );
        let covered = calibration
            .stable_points
            .iter()
            .any(|p| p.context_tokens >= context_tokens);
        prop_assert_eq!(profile.validate_against(Some(&calibration)).is_ok(), covered);
    }

    /// A bootstrap profile is the uncalibrated fallback. It must never be able
    /// to borrow authority from a calibration it is not bound to.
    #[test]
    fn bootstrap_evidence_is_rejected_when_calibration_is_supplied(
        points in proptest::collection::vec(any_stable_point(), 1..5),
    ) {
        let calibration = calibration_with(points, "key");
        let profile = execution_for(
            Some(&calibration),
            1024,
            0,
            EvidenceLabel::ConservativeBootstrap,
        );
        prop_assert!(profile.validate_against(Some(&calibration)).is_err());
    }

    /// Claiming measured evidence without a calibration to back it is always
    /// invalid, whatever the numbers say.
    #[test]
    fn measured_evidence_requires_a_calibration(context_tokens in 1u32..131_072) {
        let profile = execution_for(None, context_tokens, 0, EvidenceLabel::Measured);
        prop_assert!(profile.validate_against(None).is_err());
    }

    /// The safety reserve must leave room for output. Reserve at or above the
    /// budget leaves none.
    #[test]
    fn reserve_never_consumes_the_whole_context(
        context_tokens in 1u32..65_536,
        excess in 0u32..1024,
    ) {
        let points = vec![StablePoint {
            context_tokens,
            samples: 3,
            success_rate: 1.0,
            median_first_token_ms: 1.0,
            generation_tokens_per_second: 1.0,
            variance: 0.0,
            memory_pressure_observed: false,
        }];
        let calibration = calibration_with(points, "key");
        let profile = execution_for(
            Some(&calibration),
            context_tokens,
            context_tokens + excess,
            EvidenceLabel::Measured,
        );
        prop_assert!(profile.validate_against(Some(&calibration)).is_err());
    }

    /// A calibration measured on different hardware or a different backend must
    /// not authorise this profile.
    #[test]
    fn incompatible_calibration_is_rejected(
        points in proptest::collection::vec(any_stable_point(), 1..5),
        key in "[a-z]{1,8}",
    ) {
        let calibration = calibration_with(points, &key);
        let mut profile = execution_for(Some(&calibration), 1, 0, EvidenceLabel::Measured);
        profile.compatibility_key = format!("{key}-other-machine");
        prop_assert!(profile.validate_against(Some(&calibration)).is_err());
    }

    #[test]
    fn deployment_validation_requires_an_http_endpoint(
        endpoint in "[a-z][a-z0-9+.-]{0,8}://[a-z]{1,8}/",
    ) {
        let deployment = DeploymentDescriptor {
            schema_version: SCHEMA_VERSION,
            id: new_id(),
            provider: "ollama".into(),
            endpoint: endpoint.clone(),
            model_ref: "model".into(),
            backend_options: BTreeMap::new(),
            auth_ref: None,
        };
        let http = endpoint.starts_with("http://") || endpoint.starts_with("https://");
        prop_assert_eq!(deployment.validate().is_ok(), http);
    }

    /// Provenance is what makes an artifact auditable; an evaluation without it
    /// is not a result.
    #[test]
    fn evaluation_requires_full_provenance(
        corpus_rev in "[a-z0-9]{0,6}",
        harness_rev in "[a-z0-9]{0,6}",
        model_digest in "[a-z0-9]{0,6}",
        outcome_hash in "[a-z0-9]{0,6}",
    ) {
        let run = EvaluationRun {
            schema_version: SCHEMA_VERSION,
            id: new_id(),
            corpus_rev: corpus_rev.clone(),
            task_set: "suite".into(),
            execution_profile_id: new_id(),
            model_digest: model_digest.clone(),
            deployment_fingerprint: "fingerprint".into(),
            hardware_compatibility_key: "key".into(),
            harness_rev: harness_rev.clone(),
            seeds: vec![1],
            outcome_hash: outcome_hash.clone(),
            artifact_hashes: vec![],
            created_at: now(),
        };
        let complete = ![&corpus_rev, &harness_rev, &model_digest, &outcome_hash]
            .iter()
            .any(|field| field.is_empty());
        prop_assert_eq!(run.validate().is_ok(), complete);
    }
}

// -------------------------------------------------------------- strategies

#[test]
fn a_strategy_applies_only_to_the_deployment_it_names() {
    let strategy = |selector: &str| ModelStrategy {
        schema_version: SCHEMA_VERSION,
        id: new_id(),
        model_selector: selector.into(),
        role: "control".into(),
        prompt_suffix: " extra".into(),
        max_actions: Some(12),
        retrieval_excerpts: Some(8),
        plan_first: false,
        rationale: "measured".into(),
    };
    let declared = vec![strategy("muse-glimmer:30b-mlx"), strategy("ornith-1.5:35b")];
    assert_eq!(
        ModelStrategy::select(&declared, "ornith-1.5:35b").map(|s| s.model_selector.as_str()),
        Some("ornith-1.5:35b")
    );
    // Selection is exact: a near miss gets the shared default, not someone
    // else's policy.
    assert!(ModelStrategy::select(&declared, "ornith-1.5:35b-mlx").is_none());
    assert!(ModelStrategy::select(&declared, "qwen3.8:27b-mlx").is_none());
    assert!(ModelStrategy::select(&[], "ornith-1.5:35b").is_none());
}

#[test]
fn a_strategy_round_trips_and_keeps_its_rationale() {
    let strategy = ModelStrategy {
        schema_version: SCHEMA_VERSION,
        id: new_id(),
        model_selector: "m".into(),
        role: "r".into(),
        prompt_suffix: " suffix".into(),
        max_actions: None,
        retrieval_excerpts: None,
        plan_first: false,
        rationale: "why this exists".into(),
    };
    let decoded: ModelStrategy =
        serde_json::from_str(&serde_json::to_string(&strategy).unwrap()).unwrap();
    assert_eq!(strategy, decoded);
    // A strategy without its reason is an opinion with a schema.
    assert!(!decoded.rationale.is_empty());
}

// ---------------------------------------------------------- model profiles

fn profile(selector: &str, maximum: u32) -> ModelProfile {
    ModelProfile {
        schema_version: SCHEMA_VERSION,
        model_selector: selector.into(),
        selectors: Vec::new(),
        context: ContextPolicy {
            minimum: 65_536,
            default: 131_072,
            maximum,
        },
        sampling: BTreeMap::from([(
            "temperature".to_string(),
            ResolvedParameter {
                value: serde_json::json!(0.6),
                source: ParameterSource::OfficialModelCard,
            },
        )]),
        reasoning: None,
        reasoning_budgets: None,
        context_source: ParameterSource::OfficialModelCard,
        provenance: "vendor card".into(),
    }
}

/// A request for more context than the tag declares would either be refused or
/// silently ignored, and both make the recorded number a fiction.
#[test]
fn context_is_clamped_to_what_the_tag_declares() {
    let p = profile("m", 131_072);
    assert_eq!(p.context_for(Some(1_000_000)), 131_072);
    assert_eq!(p.context_for(Some(1024)), 65_536);
    assert_eq!(p.context_for(None), 131_072);
    // A tag with a larger ceiling allows more.
    assert_eq!(profile("m", 262_144).context_for(Some(262_144)), 262_144);
}

/// A value without its origin cannot be compared with another run's: a
/// temperature the vendor recommends and one nobody chose look identical.
#[test]
fn every_sampling_value_carries_where_it_came_from() {
    let p = profile("m", 131_072);
    assert_eq!(
        p.sampling["temperature"].source,
        ParameterSource::OfficialModelCard
    );
    // What the backend receives is the value alone.
    assert_eq!(p.sampling_options()["temperature"], serde_json::json!(0.6));
}

#[test]
fn a_profile_applies_only_to_the_tag_it_names() {
    let declared = vec![profile("ornith-1.5:35b", 262_144)];
    assert!(ModelProfile::select(&declared, "ornith-1.5:35b").is_some());
    // Per tag, not per family: the same model under another tag can declare a
    // different limit.
    assert!(ModelProfile::select(&declared, "ornith-1.5:35b-mlx").is_none());
}

/// Sizes that contradict each other would clamp to something nobody chose.
#[test]
fn a_context_policy_must_be_ordered() {
    let policy = |min, def, max| ContextPolicy {
        minimum: min,
        default: def,
        maximum: max,
    };
    assert!(policy(65_536, 131_072, 262_144).is_coherent());
    assert!(policy(131_072, 131_072, 131_072).is_coherent());
    // A default below the minimum, or above the ceiling.
    assert!(!policy(131_072, 65_536, 262_144).is_coherent());
    assert!(!policy(65_536, 262_144, 131_072).is_coherent());
}

/// Every persisted contract carries a schema version and nothing compared it
/// against the version in force, so an artifact from another build was read
/// whenever its shape happened to fit -- which is exactly when the fields that
/// changed would be read wrongly.
#[test]
fn an_artifact_from_another_schema_is_refused_rather_than_read() {
    assert!(pwr_domain::check_schema_version(pwr_domain::SCHEMA_VERSION, "profile").is_ok());
    let error = pwr_domain::check_schema_version(pwr_domain::SCHEMA_VERSION + 1, "profile")
        .unwrap_err()
        .to_string();
    assert!(error.contains("schema version"), "{error}");
    // Older is refused too: there are no migrations, and reading an old
    // artifact as though it were current is the failure this prevents.
    assert!(pwr_domain::check_schema_version(0, "profile").is_err());
}

/// A typed event has to survive the store and come back as itself, or the
/// reducer that resumes a run is reading something else.
#[test]
fn every_run_event_round_trips_through_its_stored_shape() {
    use pwr_domain::{RunEvent, ToolActionStatus};
    let cases = vec![
        RunEvent::RunStarted(serde_json::json!({"task": "fix it"})),
        RunEvent::SessionOpened {
            session: "s".into(),
        },
        RunEvent::TaskPlan {
            steps: vec!["one".into(), "two".into()],
            note: None,
        },
        RunEvent::PlanReconciled {
            steps_total: 2,
            steps_recorded_done: 1,
            steps_outstanding: vec!["two".into()],
        },
        RunEvent::TurnGenerated {
            step: 3,
            turn: 4,
            metrics: None,
            tokens_per_second: Some(12.5),
            thinking_chars: 10,
            content_chars: 20,
            prompt_delivery: None,
            normalizations: vec![],
        },
        RunEvent::ToolAction {
            action: serde_json::json!({"capability": "read_file"}),
            status: ToolActionStatus::Denied,
            outcome_class: "policy_denial".into(),
            outcome: None,
            denial: Some("refused".into()),
            failure: None,
            failure_category: None,
        },
        RunEvent::VerifierAdopted {
            step: 1,
            executable: "pytest".into(),
            args: vec!["-q".into()],
            source: "approved".into(),
        },
        RunEvent::VerificationResult {
            verified: true,
            verifiable: true,
            suite_green: false,
            still_failing_from_before: vec!["cargo test".into()],
            after: serde_json::json!({}),
            comparison: serde_json::json!({}),
            failing_before_the_run: vec![],
        },
        RunEvent::ContextTierChanged {
            previous_context_tokens: 8192,
            context_tokens: 2048,
            evidence: "measured".into(),
            provider_error: "context".into(),
            attempt: 1,
        },
        RunEvent::NoProgressDetected {
            step: 6,
            window: 6,
            actions: vec!["read_file:a".into()],
        },
        RunEvent::TaskComplete {
            step: 2,
            verified: true,
        },
        RunEvent::TaskFailed {
            reason: "no verifier".into(),
            class: pwr_domain::TerminalClass::NoVerifier,
            detail: None,
        },
    ];
    for event in cases {
        let stored = RunEvent::from_stored(event.event_type(), &event.payload());
        assert_eq!(
            stored.as_ref(),
            Some(&event),
            "{} did not round trip",
            event.event_type()
        );
    }
}

/// An event written by another build is skipped rather than guessed at. A
/// reducer that invents a variant for an unknown record resumes a run into a
/// state nothing ever recorded.
#[test]
fn an_unknown_event_type_is_not_guessed_at() {
    assert!(pwr_domain::RunEvent::from_stored("something.new", &serde_json::json!({})).is_none());
}

#[test]
fn canonical_tool_catalog_rejects_ambiguous_or_invalid_definitions() {
    let schema = serde_json::json!({"type":"object","properties":{}});
    let tool = |name: &str| ToolDefinition {
        name: name.into(),
        description: "fixture tool".into(),
        input_schema: schema.clone(),
    };
    let catalog = ToolCatalog::new(vec![tool("read_file")]).unwrap();
    assert!(catalog.get("read_file").is_some());
    assert!(ToolCatalog::new(vec![tool("read_file"), tool("read_file")]).is_err());
    assert!(
        ToolCatalog::new(vec![ToolDefinition {
            name: "bad".into(),
            description: "bad schema".into(),
            input_schema: serde_json::json!(false),
        }])
        .is_err()
    );
}

#[test]
fn artifact_registry_pins_hub_revisions_and_platforms() {
    let artifact = ModelArtifact {
        schema_version: SCHEMA_VERSION,
        id: "qwen-coder-mlx".into(),
        family: "qwen".into(),
        variant: "coder".into(),
        source: ArtifactSource::HuggingFace {
            repository: "mlx-community/Qwen-Coder".into(),
            revision: "0123456789abcdef0123456789abcdef01234567".into(),
            files: vec!["model.safetensors".into()],
        },
        format: ArtifactFormat::Mlx,
        quantization: Some("4bit".into()),
        platform: ArtifactPlatform::MacosAppleSilicon,
        provenance: Provenance {
            source: "fixture".into(),
            observed_at: now(),
            content_hash: "fixture".into(),
        },
    };
    assert!(artifact.validate().is_ok());
    assert!(artifact.supports(ArtifactPlatform::MacosAppleSilicon));
    assert!(!artifact.supports(ArtifactPlatform::WindowsX86_64));
    let plan = artifact
        .download_plan(std::path::Path::new("/models"))
        .unwrap()
        .unwrap();
    assert_eq!(plan.artifact_id, "qwen-coder-mlx");
    assert_eq!(
        plan.files[0].url,
        "https://huggingface.co/mlx-community/Qwen-Coder/resolve/0123456789abcdef0123456789abcdef01234567/model.safetensors"
    );
    assert_eq!(
        plan.files[0].destination,
        "/models/mlx-community/Qwen-Coder/model.safetensors"
    );
    let mut moving = artifact;
    moving.source = ArtifactSource::HuggingFace {
        repository: "mlx-community/Qwen-Coder".into(),
        revision: "main".into(),
        files: vec!["model.safetensors".into()],
    };
    assert!(moving.validate().is_err());
}

#[test]
fn artifact_download_plan_carries_file_verification_data() {
    let artifact = ModelArtifact {
        schema_version: SCHEMA_VERSION,
        id: "qwen-coder-mlx".into(),
        family: "qwen".into(),
        variant: "coder".into(),
        source: ArtifactSource::HuggingFace {
            repository: "mlx-community/Qwen-Coder".into(),
            revision: "0123456789abcdef0123456789abcdef01234567".into(),
            files: vec![ArtifactFile::Described {
                path: "model.safetensors".into(),
                bytes: Some(42),
                blake3: Some(
                    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
                ),
                sha256: Some(
                    "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
                ),
            }],
        },
        format: ArtifactFormat::Mlx,
        quantization: Some("4bit".into()),
        platform: ArtifactPlatform::MacosAppleSilicon,
        provenance: Provenance {
            source: "fixture".into(),
            observed_at: now(),
            content_hash: "fixture".into(),
        },
    };

    let plan = artifact
        .download_plan(std::path::Path::new("/models"))
        .unwrap()
        .unwrap();
    assert_eq!(plan.files[0].expected_bytes, Some(42));
    assert_eq!(
        plan.files[0].blake3.as_deref(),
        Some("0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef")
    );
    assert_eq!(
        plan.files[0].sha256.as_deref(),
        Some("abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789")
    );
}

#[test]
fn huggingface_artifacts_refuse_unsafe_file_paths() {
    let artifact = ModelArtifact {
        schema_version: SCHEMA_VERSION,
        id: "qwen-coder-mlx".into(),
        family: "qwen".into(),
        variant: "coder".into(),
        source: ArtifactSource::HuggingFace {
            repository: "mlx-community/Qwen-Coder".into(),
            revision: "0123456789abcdef0123456789abcdef01234567".into(),
            files: vec!["../model.safetensors".into()],
        },
        format: ArtifactFormat::Mlx,
        quantization: Some("4bit".into()),
        platform: ArtifactPlatform::MacosAppleSilicon,
        provenance: Provenance {
            source: "fixture".into(),
            observed_at: now(),
            content_hash: "fixture".into(),
        },
    };

    assert!(artifact.validate().is_err());
}

#[test]
fn huggingface_artifacts_refuse_invalid_file_hashes() {
    let artifact = ModelArtifact {
        schema_version: SCHEMA_VERSION,
        id: "qwen-coder-mlx".into(),
        family: "qwen".into(),
        variant: "coder".into(),
        source: ArtifactSource::HuggingFace {
            repository: "mlx-community/Qwen-Coder".into(),
            revision: "0123456789abcdef0123456789abcdef01234567".into(),
            files: vec![ArtifactFile::Described {
                path: "model.safetensors".into(),
                bytes: Some(42),
                blake3: Some("not-a-hash".into()),
                sha256: None,
            }],
        },
        format: ArtifactFormat::Mlx,
        quantization: Some("4bit".into()),
        platform: ArtifactPlatform::MacosAppleSilicon,
        provenance: Provenance {
            source: "fixture".into(),
            observed_at: now(),
            content_hash: "fixture".into(),
        },
    };

    assert!(artifact.validate().is_err());
}

#[test]
fn profile_selection_prefers_immutable_identity_and_refuses_ties() {
    let profile = |selector: ModelProfileSelector| ModelProfile {
        schema_version: SCHEMA_VERSION,
        model_selector: "legacy-tag".into(),
        selectors: vec![selector],
        context: ContextPolicy {
            minimum: 1024,
            default: 2048,
            maximum: 4096,
        },
        sampling: BTreeMap::new(),
        reasoning: None,
        reasoning_budgets: None,
        context_source: ParameterSource::OfficialModelCard,
        provenance: "fixture".into(),
    };
    let identity = DeploymentIdentity {
        provider: "lmstudio".into(),
        model_ref: "qwen-coder".into(),
        digest: Some("sha256:fixed".into()),
        family: Some("qwen".into()),
    };
    let family = profile(ModelProfileSelector::Family {
        family: "qwen".into(),
    });
    let deployment = profile(ModelProfileSelector::Deployment {
        provider: "lmstudio".into(),
        model_ref: "qwen-coder".into(),
    });
    let digest = profile(ModelProfileSelector::Digest {
        digest: "sha256:fixed".into(),
    });

    let profiles = [family.clone(), deployment.clone(), digest.clone()];
    let selected = ModelProfile::select_for(&profiles, &identity).expect("digest selector wins");
    assert_eq!(selected.selectors, digest.selectors);

    assert!(ModelProfile::select_for(&[digest.clone(), digest], &identity).is_none());
}

#[test]
fn artifact_registry_filters_by_host_without_selecting_a_model_for_it() {
    let artifact = |id: &str, format, platform| ModelArtifact {
        schema_version: SCHEMA_VERSION,
        id: id.into(),
        family: "qwen".into(),
        variant: id.into(),
        source: ArtifactSource::BackendManaged {
            backend: "lmstudio".into(),
            model_ref: id.into(),
        },
        format,
        quantization: Some("4bit".into()),
        platform,
        provenance: Provenance {
            source: "fixture".into(),
            observed_at: now(),
            content_hash: "fixture".into(),
        },
    };
    let registry = ArtifactRegistry {
        schema_version: SCHEMA_VERSION,
        artifacts: vec![
            artifact(
                "qwen-mlx",
                ArtifactFormat::Mlx,
                ArtifactPlatform::MacosAppleSilicon,
            ),
            artifact(
                "qwen-gguf",
                ArtifactFormat::Gguf,
                ArtifactPlatform::WindowsX86_64,
            ),
        ],
    };
    let eligible = registry
        .eligible_for(ArtifactPlatform::MacosAppleSilicon, Some("qwen"))
        .unwrap();
    assert_eq!(eligible.len(), 1);
    assert_eq!(eligible[0].id, "qwen-mlx");
    assert!(
        registry
            .eligible_for(ArtifactPlatform::LinuxX86_64, Some("qwen"))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn packaged_artifact_registry_is_valid_and_has_downloadable_ggufs() {
    let registry: ArtifactRegistry =
        serde_json::from_slice(include_bytes!("../../../strategies/artifacts.json"))
            .expect("packaged artifact registry must parse");
    registry
        .validate()
        .expect("packaged registry must validate");
    let mac_artifacts = registry
        .eligible_for(ArtifactPlatform::MacosAppleSilicon, None)
        .unwrap();
    assert_eq!(mac_artifacts.len(), 7);
    assert_eq!(
        mac_artifacts
            .iter()
            .filter(|artifact| artifact.format == ArtifactFormat::Mlx)
            .count(),
        5
    );
    let downloadable_ggufs = mac_artifacts
        .iter()
        .filter(|artifact| matches!(artifact.source, ArtifactSource::HuggingFace { .. }))
        .filter(|artifact| artifact.format == ArtifactFormat::Gguf)
        .collect::<Vec<_>>();
    assert_eq!(downloadable_ggufs.len(), 2);
    assert!(downloadable_ggufs.iter().all(|artifact| {
        artifact
            .download_plan(std::path::Path::new("/models"))
            .unwrap()
            .unwrap()
            .files
            .iter()
            .all(|file| file.expected_bytes.is_some() && file.sha256.is_some())
    }));
}

/// A deployment that grades its reasoning is asked for the least of it.
///
/// Measured on a 27B whose default is `xhigh`: one prompt asking for a small
/// function generated 292 reasoning tokens at `low` and 3,999 at the default —
/// the token ceiling rather than the end of its thinking — taking 30 seconds
/// against 338. A benchmark of reasoning wants the high setting; an agent that
/// must take fifty actions cannot afford it.
#[test]
fn the_least_reasoning_a_deployment_offers_is_chosen_by_name() {
    let offered =
        |names: &[&str]| -> Vec<String> { names.iter().map(|name| (*name).to_owned()).collect() };
    // The measured case, listed in the backend's own order.
    assert_eq!(
        pwr_domain::lowest_reasoning_effort(&offered(&["off", "low", "medium", "xhigh", "on"])),
        Some("low")
    );
    // Position says nothing: `on` trails the graded levels and is the model's
    // own default, not the highest step.
    assert_eq!(
        pwr_domain::lowest_reasoning_effort(&offered(&["xhigh", "on", "medium"])),
        Some("medium")
    );
    // Nothing graded is nothing to choose. Asking for a level a deployment
    // never advertised would be a guess.
    assert_eq!(
        pwr_domain::lowest_reasoning_effort(&offered(&["off", "on"])),
        None
    );
    assert_eq!(pwr_domain::lowest_reasoning_effort(&[]), None);
}

/// Ids taken in the same millisecond, from several threads, still sort in the
/// order they were taken. The ordering guarantee the event log reads back by.
#[test]
fn identifiers_taken_at_once_are_still_ordered() {
    let taken: Vec<(usize, pwr_domain::Id)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..4)
            .map(|_| scope.spawn(|| (0..250).map(|_| pwr_domain::new_id()).collect::<Vec<_>>()))
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap())
            .enumerate()
            .collect()
    });
    for thread_ids in taken.chunks(250) {
        let ids: Vec<_> = thread_ids.iter().map(|(_, id)| *id).collect();
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted, "ids from one thread were not in order");
    }
    let mut all: Vec<_> = taken.iter().map(|(_, id)| *id).collect();
    all.sort();
    all.dedup();
    assert_eq!(all.len(), taken.len(), "ids collided");
}
