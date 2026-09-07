use std::{collections::HashMap, fs, path::Path};

use anyhow::{bail, Context, Result};
use kitu_app_actions::ActionValue;
use kitu_demo_game::{build_demo_runtime, DemoRuntime};
use serde::Deserialize;

#[test]
fn admin_world_basic_scenario() {
    run_scenario(
        &fixture_path("admin-world-basic/scenario.json"),
        &fixture_path("admin-world-basic/expected.json"),
    )
    .unwrap();
}

fn fixture_path(relative: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scenarios")
        .join(relative)
}

fn run_scenario(scenario_path: &Path, expected_path: &Path) -> Result<()> {
    let scenario: Scenario = read_json(scenario_path)?;
    let expected: Expected = read_json(expected_path)?;
    validate_pair(&scenario, &expected)?;

    let mut runtime = build_demo_runtime()?;
    execute_steps(&mut runtime, scenario.steps)?;
    assert_expected_state(&runtime, &expected)
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let source = fs::read_to_string(path).with_context(|| format!("read `{}`", path.display()))?;
    serde_json::from_str(&source).with_context(|| format!("parse `{}`", path.display()))
}

fn validate_pair(scenario: &Scenario, expected: &Expected) -> Result<()> {
    if scenario.schema_version != 1 || expected.schema_version != 1 {
        bail!("only schemaVersion 1 is supported");
    }
    if scenario.scenario_id != expected.scenario_id {
        bail!(
            "scenarioId mismatch: scenario `{}`, expected `{}`",
            scenario.scenario_id,
            expected.scenario_id
        );
    }
    Ok(())
}

fn execute_steps(runtime: &mut DemoRuntime, steps: Vec<ScenarioStep>) -> Result<()> {
    let mut last_object_id = None;

    for step in steps {
        let mut inputs = step.inputs;
        let action_id = if step.action_id == "move-last-object" {
            let id = last_object_id
                .clone()
                .context("move-last-object requires a previously spawned object")?;
            inputs.insert("id".to_string(), ActionValue::String(id));
            "move-object".to_string()
        } else {
            step.action_id
        };

        runtime
            .run_app_action(&action_id, &inputs)
            .with_context(|| format!("run app action `{action_id}`"))?;

        if action_id == "spawn-object" {
            last_object_id = runtime
                .inspect_world_state()
                .objects
                .last()
                .map(|object| object.id.clone());
        }

        runtime.tick_once().context("advance runtime tick")?;
    }

    Ok(())
}

fn assert_expected_state(runtime: &DemoRuntime, expected: &Expected) -> Result<()> {
    let snapshot = runtime.inspect_world_state();
    if runtime.current_tick().get() != expected.expected_tick {
        bail!(
            "tick mismatch: expected {}, observed {}",
            expected.expected_tick,
            runtime.current_tick().get()
        );
    }
    if snapshot.objects.len() != expected.expected_objects.len() {
        bail!(
            "object count mismatch: expected {}, observed {}",
            expected.expected_objects.len(),
            snapshot.objects.len()
        );
    }

    for (index, expected_object) in expected.expected_objects.iter().enumerate() {
        let observed = &snapshot.objects[index];
        if observed.kind != expected_object.kind {
            bail!(
                "object {index} kind mismatch: expected `{}`, observed `{}`",
                expected_object.kind,
                observed.kind
            );
        }
        assert_close(index, "x", expected_object.x, observed.transform.x)?;
        assert_close(index, "y", expected_object.y, observed.transform.y)?;
        assert_close(index, "z", expected_object.z, observed.transform.z)?;
    }

    Ok(())
}

fn assert_close(index: usize, field: &str, expected: f32, observed: f32) -> Result<()> {
    if (expected - observed).abs() > 0.00001 {
        bail!("object {index} {field} mismatch: expected {expected}, observed {observed}");
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Scenario {
    schema_version: u32,
    scenario_id: String,
    steps: Vec<ScenarioStep>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioStep {
    action_id: String,
    #[serde(default)]
    inputs: HashMap<String, ActionValue>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Expected {
    schema_version: u32,
    scenario_id: String,
    expected_tick: u64,
    expected_objects: Vec<ExpectedObject>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExpectedObject {
    kind: String,
    x: f32,
    y: f32,
    z: f32,
}
