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
    Subject {
        provenance: Provenance::of(inspection, backend.backend_id(), version, Some(class)),
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
    assess_evidence(AssessInput {
        current: subject.provenance,
        reasoning: subject.reasoning,
        incompatible: subject.incompatible,
        verified: &verified,
        local: local.as_ref(),
    })
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
