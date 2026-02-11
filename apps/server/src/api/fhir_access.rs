//! Helpers for enforcing config-driven FHIR API access rules.

use crate::runtime_config::ConfigKey;
use crate::{state::AppState, Result};

pub(crate) fn ensure_interaction_enabled(enabled: bool, interaction: &str) -> Result<()> {
    if enabled {
        Ok(())
    } else {
        Err(crate::Error::MethodNotAllowed(format!(
            "FHIR interaction '{}' is disabled by configuration",
            interaction
        )))
    }
}

pub(crate) async fn ensure_interaction_enabled_runtime(
    state: &AppState,
    key: ConfigKey,
    interaction: &str,
) -> Result<()> {
    let enabled: bool = state.runtime_config_cache.get(key).await;
    ensure_interaction_enabled(enabled, interaction)
}

pub(crate) fn ensure_resource_type_supported(state: &AppState, resource_type: &str) -> Result<()> {
    let configured = &state.config.fhir.capability_statement.supported_resources;
    if configured.is_empty() {
        return Ok(());
    }

    if configured.iter().any(|rt| rt == resource_type) {
        Ok(())
    } else {
        Err(crate::Error::MethodNotAllowed(format!(
            "Resource type '{}' is not supported by this server",
            resource_type
        )))
    }
}
