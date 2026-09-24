use async_trait::async_trait;
use pwr_domain::{BackendState, DeploymentDescriptor, ModelInspection, ModelRequest};
use pwr_provider::{
    BackendCapabilities, DiscoveredModel, InferenceBackend, ModelProvider, ModelStream,
    ProviderError,
};

struct FixtureBackend;

#[async_trait]
impl ModelProvider for FixtureBackend {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!("not used by this contract test")
    }

    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!("not used by this contract test")
    }

    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        unreachable!("not used by this contract test")
    }
}

#[async_trait]
impl InferenceBackend for FixtureBackend {
    fn backend_id(&self) -> &'static str {
        "fixture"
    }

    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities {
            model_discovery: true,
            model_lifecycle: false,
            streaming: true,
            cancellation: false,
            native_tools: false,
            constrained_tool_calls: false,
            generation_metrics: false,
            context_window_control: true,
        }
    }

    async fn discover_models(&self) -> Result<Vec<DiscoveredModel>, ProviderError> {
        Ok(vec![DiscoveredModel {
            model_ref: "fixture:1".into(),
            context_limit: Some(4096),
            ..Default::default()
        }])
    }
}

#[tokio::test]
async fn inventory_and_unsupported_lifecycle_are_explicit() {
    let backend = FixtureBackend;
    assert_eq!(backend.backend_id(), "fixture");
    assert!(backend.capabilities().model_discovery);
    assert!(!backend.capabilities().model_lifecycle);
    assert_eq!(
        backend.discover_models().await.unwrap()[0].model_ref,
        "fixture:1"
    );
    assert!(matches!(
        backend.load_model("fixture:1").await,
        Err(ProviderError::Protocol { .. })
    ));
}

/// A backend whose context window is fixed when the model is loaded, and which
/// clamps a request to what the loaded artifact supports.
struct FixedWindowBackend {
    serves: u32,
}

#[async_trait]
impl ModelProvider for FixedWindowBackend {
    async fn inspect(&self, _: &DeploymentDescriptor) -> Result<ModelInspection, ProviderError> {
        unreachable!("not used by this contract test")
    }

    async fn runtime_state(&self) -> Result<BackendState, ProviderError> {
        unreachable!("not used by this contract test")
    }

    async fn chat(&self, _: ModelRequest) -> Result<ModelStream, ProviderError> {
        unreachable!("not used by this contract test")
    }

    async fn prepare_context(
        &self,
        _: &DeploymentDescriptor,
        _context_tokens: u32,
    ) -> Result<u32, ProviderError> {
        Ok(self.serves)
    }
}

fn deployment() -> DeploymentDescriptor {
    DeploymentDescriptor {
        schema_version: 1,
        id: pwr_domain::new_id(),
        provider: "fixture".into(),
        endpoint: "http://127.0.0.1:1/".into(),
        model_ref: "fixture:1".into(),
        backend_options: Default::default(),
        auth_ref: None,
    }
}

/// A backend whose window is a per-request option honours what it was asked
/// for, and the default says so without doing anything.
#[tokio::test]
async fn a_per_request_window_is_granted_as_asked() {
    assert_eq!(
        FixtureBackend
            .prepare_context(&deployment(), 8_192)
            .await
            .unwrap(),
        8_192
    );
}

/// The case the contract exists for. A backend that serves a different window
/// must say which one, because a caller that recorded the tier it asked for
/// would have measured something that never ran -- and a whole ladder measured
/// that way looks like several tiers while being one.
#[tokio::test]
async fn a_window_served_instead_of_the_one_asked_for_is_reported_not_hidden() {
    let backend = FixedWindowBackend { serves: 262_144 };
    let granted = backend.prepare_context(&deployment(), 8_192).await.unwrap();
    assert_ne!(granted, 8_192);
    assert_eq!(granted, 262_144);
}
