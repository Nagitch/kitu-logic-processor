//! Demo game application wiring for the Kitu framework.
//!
//! This crate is intentionally outside `crates/`: it represents an application
//! using the framework crates, not another reusable framework component.

use anyhow::{Context, Result};
use kitu_runtime::{build_runtime, Runtime};
use kitu_transport::LocalChannel;

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

#[cfg(test)]
mod tests {
    use super::*;

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
