//! The console's view of a model's compatibility: the backend, the declared
//! profiles and this machine, handed to `pwr_models::profile`, which holds
//! every rule. Nothing here decides a status.
use pwr_domain::{ModelInspection, ModelProfile, ModelProfileSelector, ReasoningControl};
use pwr_models::profile::{
    AssessInput, Assessment, EvidenceStore, Provenance, assess as assess_evidence, hardware_class,
    static_incompatibility, template_reasoning, verified_entries,
};
use pwr_provider::InferenceBackend;
use tokio::sync::OnceCell;

/// Whether a declared profile turns thinking off for this deployment.
fn thinking_disabled(profile: Option<&ModelProfile>) -> bool {
    profile.is_some_and(|profile| {
        matches!(profile.reasoning, Some(ReasoningControl::Think { enabled: false }))
            || matches!(&profile.reasoning, Some(ReasoningControl::PromptDirective { text }) if text.contains("no_think"))
            || profile
                .sampling
                .get("think")
                .is_some_and(|value| value.value == serde_json::json!(false))
    })
}

/// A budget mapping is taken from a profile only when the profile names this
/// exact artifact or deployment -- never a family, never a bare tag -- so a
/// measurement on one artifact cannot become a claim about another.
fn exact_budgets(
    profile: Option<&ModelProfile>,
    identity: &pwr_domain::DeploymentIdentity,
) -> Option<pwr_domain::ReasoningBudgets> {
    let profile = profile?;
    let exact = profile.selectors.iter().any(|selector| match selector {
        ModelProfileSelector::Digest { digest } => identity.digest.as_ref() == Some(digest),
        ModelProfileSelector::Deployment {
            provider,
            model_ref,
        } => provider == &identity.provider && model_ref == &identity.model_ref,
        ModelProfileSelector::Family { .. } => false,
    });
    profile
        .reasoning_budgets
        .filter(|budgets| exact && budgets.valid())
}

/// This machine's class, detected once per process.
async fn machine_class(models_root: &std::path::Path) -> String {
    static CLASS: OnceCell<String> = OnceCell::const_new();
    CLASS
        .get_or_init(|| async {
            hardware_class(&pwr_runtime::host::detect_host(models_root).await)
        })
        .await
        .clone()
}

/// The inputs to calibration and assessment for one inspected artifact.
pub struct Subject {
    pub provenance: Provenance,
    pub reasoning: pwr_domain::ReasoningProfile,
    pub incompatible: Option<String>,
}

pub async fn subject<B: InferenceBackend>(
    backend: &B,
    inspection: &ModelInspection,
    declared: Option<&ModelProfile>,
) -> Subject {
    let kind = if backend.backend_id() == "llama" {
        pwr_runtime::BackendKind::Llama
    } else {
        pwr_runtime::BackendKind::Mlx
    };
    let identity = pwr_domain::DeploymentIdentity::from_inspection(
        &inspection.deployment,
        &inspection.definition,
    );
    let version = backend.backend_version().await.ok().flatten();
    let class = machine_class(&pwr_runtime::models_root(kind)).await;
    let mut provenance = Provenance::of(inspection, backend.backend_id(), version, Some(class));
    provenance.adapter_revision = Some(
        pwr_compat::adapter_for(
            inspection.definition.family.as_deref(),
            &inspection.deployment.model_ref,
        )
        .version()
        .to_owned(),
    );
    Subject {
        provenance,
        reasoning: template_reasoning(
            inspection,
            thinking_disabled(declared),
            exact_budgets(declared, &identity),
        ),
        incompatible: static_incompatibility(inspection),
    }
}

/// Everything known about the artifact, from the verified registry and this
/// machine's calibrations.
pub fn assess(subject: Subject) -> Assessment {
    let local = EvidenceStore::default_location()
        .and_then(|store| store.load(&subject.provenance.backend, &subject.provenance.model));
    let verified = verified_entries();
    let notes = known_issues(
        &subject.provenance.model,
        subject.provenance.architecture.as_deref(),
    );
    let mut assessment = assess_evidence(AssessInput {
        current: subject.provenance,
        reasoning: subject.reasoning,
        incompatible: subject.incompatible,
        verified: &verified,
        local: local.as_ref(),
    });
    assessment.reasons.extend(notes);
    assessment
}

/// What is publicly known to go wrong with a family of models and that no
/// calibration of a few short requests can show, with where it is reported.
/// Said beside the model, whatever its status: a model that passes every check
/// here is not thereby free of it.
pub fn known_issues(model: &str, architecture: Option<&str>) -> Vec<String> {
    let evidence = format!("{model} {}", architecture.unwrap_or_default()).to_ascii_lowercase();
    let mut notes = Vec::new();
    if evidence.contains("gemma4") || evidence.contains("gemma-4") {
        notes.push(
            "Known upstream issue: Gemma 4 can fall into a loop that writes \"thought\" or its \
             channel markers over and over on long prompts with many tools (reported against the \
             12B, 26B and 31B, at full precision too; google-deepmind/gemma#622 and #727). No \
             sampling setting removes it. PWR retries and compacts, but for a long agent task \
             another model is more dependable; Gemma is fine for chat and short tasks."
                .to_owned(),
        );
    }
    notes
}

/// What the app shows beside the Reasoning Effort control.
pub fn reasoning_view(
    assessment: &Assessment,
    effort: pwr_domain::ReasoningEffort,
) -> serde_json::Value {
    use pwr_domain::ReasoningCapability as Capability;
    let reasoning = &assessment.reasoning;
    let control = if reasoning.disabled_by_profile {
        "off_by_profile"
    } else if reasoning.budget_enforceable() {
        "budget"
    } else if reasoning.capability == Capability::TemplateControlled {
        "level"
    } else if reasoning.finalization_verified == Some(false) {
        "unsafe"
    } else {
        match reasoning.capability {
            Capability::None => "none",
            Capability::ObservableOnly => "observable_only",
            _ => "unknown",
        }
    };
    let (budgets, source) = reasoning.budgets();
    serde_json::json!({
        "effort": effort,
        "applies": reasoning.effort_applies(),
        "control": control,
        "capability": reasoning.capability,
        "evidence": reasoning.evidence,
        "budgets": reasoning.budget_enforceable().then_some(budgets),
        "budgetSource": reasoning.budget_enforceable().then_some(source),
    })
}

#[cfg(test)]
mod known_issue_tests {
    use super::known_issues;

    #[test]
    fn a_gemma_4_model_carries_its_upstream_issue_and_others_do_not() {
        for model in [
            "mlx-community/gemma-4-12B-it-4bit",
            "mlx-community/gemma-4-26b-a4b-it-4bit",
            "lmstudio-community/gemma-4-31B-it-MLX-6bit",
        ] {
            let notes = known_issues(model, None);
            assert_eq!(notes.len(), 1, "{model}");
            assert!(notes[0].contains("google-deepmind/gemma#622"));
        }
        assert_eq!(known_issues("some/model", Some("gemma4")).len(), 1);
        assert!(
            known_issues(
                "mlx-community/Qwen3-Coder-30B-A3B-Instruct-4bit",
                Some("qwen3_moe")
            )
            .is_empty()
        );
        assert!(known_issues("mlx-community/gemma-3-12b-it", None).is_empty());
    }
}
