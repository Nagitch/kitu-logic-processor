//! Demo game application wiring for the Kitu framework.
//!
//! This crate is intentionally outside `crates/`: it represents an application
//! using the framework crates, not another reusable framework component.

use anyhow::{Context, Result};
use kitu_runtime::{build_runtime, Runtime};
use kitu_transport::LocalChannel;

pub mod arena;
pub mod replay;

/// Stable app identifier used in project-scoped app actions.
pub const APP_ID: &str = "demo-game";

/// Project-owned action manifest for the demo game.
pub const APP_ACTIONS_TOML: &str = include_str!("../kitu-app-actions.toml");

/// Runtime type used by the demo game host and scenario tests.
pub type DemoRuntime = Runtime<LocalChannel>;

/// Builds a demo-game runtime from Kitu framework crates.
pub fn build_demo_runtime() -> Result<DemoRuntime> {
    let mut runtime = build_runtime(LocalChannel::connected());
    runtime
        .load_project_app_actions_from_toml(APP_ID, APP_ACTIONS_TOML)
        .context("load demo-game app actions")?;
    Ok(runtime)
}

/// Builds the same demo runtime with the authoritative Arena application installed.
///
/// # Examples
/// ```
/// let mut runtime = kitu_demo_game::build_arena_runtime().unwrap();
/// assert!(!runtime.inspect_application().is_empty());
/// runtime.tick_once().unwrap();
/// ```
pub fn build_arena_runtime() -> Result<DemoRuntime> {
    let mut runtime = build_demo_runtime()?;
    arena::install(&mut runtime)?;
    Ok(runtime)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arena_catalog_actions_match_the_versioned_input_contract() {
        use kitu_app_actions::{ActionInputType, ActionValue};
        let runtime = build_arena_runtime().unwrap();
        for action in runtime
            .app_action_catalog()
            .actions
            .iter()
            .filter(|a| a.id.starts_with("arena."))
        {
            let inputs = action
                .inputs
                .iter()
                .map(|spec| {
                    assert_eq!(spec.value_type, ActionInputType::Int);
                    (spec.name.clone(), ActionValue::Int(0))
                })
                .collect();
            let message = runtime
                .app_action_catalog()
                .materialize_message(&action.id, &inputs)
                .unwrap();
            arena::validate_input(
                &message,
                &kitu_runtime::InputMetadata {
                    source: "catalog-test".into(),
                    message_id: 1,
                    schema_version: arena::SCHEMA_VERSION,
                },
            )
            .unwrap_or_else(|e| panic!("{}: {e}", action.id));
        }
    }

    #[test]
    fn runtime_loads_project_actions() {
        let runtime = build_demo_runtime().unwrap();
        let catalog = runtime.app_action_catalog();

        assert!(catalog.action("spawn-object").is_some());
        assert!(catalog.action("enemy.spawn").is_some());
        assert!(catalog.action("player.godmode").is_some());
        assert!(catalog.action("map.fast-travel").is_some());
    }
}
