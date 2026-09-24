//! Quick Calibration and Reasoning Effort against a real model on this
//! machine. Ignored by default: it needs Apple Silicon, an MLX engine and a
//! downloaded model, and takes minutes. Run it by hand:
//!
//! ```text
//! PWR_MLX_PYTHON=$PWD/.venv-mlx/bin/python \
//! PWR_LIVE_MODEL=Qwen/Qwen3-14B-MLX-4bit \
//! cargo test -p pwr-models --test live_model -- --ignored --nocapture
//! ```
//!
//! It writes nothing to the evidence store; it prints what it saw.
use pwr_domain::{
    ChatMessage, GenerationEnvelope, ModelRequest, ReasoningDirective, ReasoningEffort,
    plan_reasoning,
};
use pwr_models::calibration::quick_calibrate;
use pwr_models::profile::{
    AssessInput, Provenance, assess, static_incompatibility, template_reasoning,
};
use pwr_provider::{Cancel, InferenceBackend, ModelProvider};
use std::time::{Duration, Instant};

fn model() -> Option<String> {
    std::env::var("PWR_LIVE_MODEL").ok()
}

#[tokio::test(flavor = "multi_thread")]
#[ignore = "needs a local MLX model; see the file's header"]
async fn live_quick_calibration_and_reasoning_effort() {
    let Some(model) = model() else {
        eprintln!("PWR_LIVE_MODEL is not set; nothing to do");
        return;
    };
    let runtime = pwr_runtime::RuntimeFactory::local(pwr_runtime::BackendKind::Mlx);
    let selection = runtime.select(&model, Duration::from_secs(600)).unwrap();
    let backend = selection.backend;
    let inspection = backend.inspect(&selection.deployment).await.unwrap();
    let version = backend.backend_version().await.unwrap();
    println!("backend version: {version:?}");
    let provenance = Provenance::of(&inspection, backend.backend_id(), version, None);
    let reasoning = template_reasoning(&inspection, false, None);
    println!(
        "template reasoning: {}",
        serde_json::to_string(&reasoning).unwrap()
    );
    let before = assess(AssessInput {
        current: provenance.clone(),
        reasoning: reasoning.clone(),
        incompatible: static_incompatibility(&inspection),
        verified: &[],
        local: None,
    });
    println!(
        "before calibration: {:?} / {}",
        before.status, before.summary
    );

    let started = Instant::now();
    let evidence = quick_calibrate(
        &backend,
        &inspection,
        provenance.clone(),
        &reasoning,
        &Cancel::new(),
        &mut |step: usize, total: usize, name: &str| {
            println!("  [{step}/{total}] {name} ({:.0?})", started.elapsed())
        },
    )
    .await
    .unwrap();
    println!(
        "calibration: {:?} in {} ms",
        evidence.status, evidence.duration_ms
    );
    for check in &evidence.checks {
        println!("  {:?} {} -- {}", check.passed, check.name, check.detail);
    }
    println!(
        "  reasoning: {}",
        serde_json::to_string(&evidence.reasoning).unwrap()
    );
    let after = assess(AssessInput {
        current: provenance,
        reasoning,
        incompatible: None,
        verified: &[],
        local: Some(&evidence),
    });
    println!(
        "after calibration: {:?} ({:?}); reasoning {:?}, finalization {:?}",
        after.status,
        after.confidence,
        after.reasoning.capability,
        after.reasoning.finalization_verified
    );
    for line in &after.capabilities {
        println!("  {}: {}", line.label, line.result);
    }

    // One request per effort, with the plan the turn would make.
    for effort in ReasoningEffort::ALL {
        let plan = plan_reasoning(
            effort,
            &after.reasoning,
            GenerationEnvelope {
                context_limit: 32_768,
                input_tokens: 300,
                answer_allowance: 2_048,
            },
        );
        let mut request = ModelRequest {
            deployment: selection.deployment.clone(),
            messages: vec![ChatMessage::text(
                "user",
                "How many prime numbers are there below 60? Think it through, then give the \
                 count as a number.",
            )],
            context_tokens: 32_768,
            tools: None,
            seed: Some(7),
            sampling: Default::default(),
        };
        request
            .sampling
            .insert("temperature".into(), serde_json::json!(0));
        match plan.directive {
            ReasoningDirective::Budget { tokens } => {
                request
                    .sampling
                    .insert("reasoning_budget".into(), serde_json::json!(tokens));
            }
            ReasoningDirective::Off => {
                request
                    .sampling
                    .insert("think".into(), serde_json::json!(false));
            }
            ReasoningDirective::Level { effort } => {
                request.sampling.insert(
                    "reasoning_effort".into(),
                    serde_json::json!(effort.as_str()),
                );
            }
            ReasoningDirective::TemplateDefault => {}
        }
        request
            .sampling
            .insert("max_tokens".into(), serde_json::json!(plan.max_tokens));
        let started = Instant::now();
        let outcome = async {
            let stream = backend.chat_cancellable(request, Cancel::new()).await?;
            pwr_provider::collect_reply(stream).await
        }
        .await;
        match outcome {
            Ok(reply) => {
                let metrics = reply.metrics.unwrap_or_default();
                println!(
                    "{:?}: directive {:?}, reasoning {:?} tokens, budget reached {:?}, answer {:?} ({:.1?})",
                    effort,
                    plan.directive,
                    metrics.reasoning_tokens,
                    metrics.reasoning_budget_reached,
                    reply.content.trim().chars().take(80).collect::<String>(),
                    started.elapsed()
                );
                if let (ReasoningDirective::Budget { tokens }, Some(used)) =
                    (plan.directive, metrics.reasoning_tokens)
                {
                    assert!(used <= u64::from(tokens) + 1, "{used} > {tokens}");
                }
            }
            Err(error) => println!("{effort:?}: {error} ({:.1?})", started.elapsed()),
        }
    }
}
