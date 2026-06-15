//! Entry point for the kitu-replay-runner binary.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::build_runtime;
use kitu_transport::LocalChannel;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const FLOAT_TOLERANCE: f64 = 0.00001;

fn main() -> Result<()> {
    let args = CliArgs::parse(env::args().skip(1).collect())?;
    let summary = run_replay(&args.scenario, &args.expected)?;

    fs::create_dir_all(&args.output_dir)
        .with_context(|| format!("create output dir `{}`", args.output_dir.display()))?;
    let summary_path = args.output_dir.join("summary.json");
    fs::write(&summary_path, serde_json::to_string_pretty(&summary)?)
        .with_context(|| format!("write `{}`", summary_path.display()))?;

    println!("{}", summary_path.display());
    if summary.status != "pass" {
        bail!(
            "replay failed with {} mismatch(es)",
            summary.observed.mismatch_count
        );
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct CliArgs {
    scenario: PathBuf,
    expected: PathBuf,
    output_dir: PathBuf,
}

impl CliArgs {
    fn parse(args: Vec<String>) -> Result<Self> {
        if args.is_empty() || args.iter().any(|arg| arg == "-h" || arg == "--help") {
            print_help();
            bail!("missing required arguments");
        }

        let scenario = required_path_flag(&args, "--scenario")?;
        let expected = optional_path_flag(&args, "--expected").unwrap_or_else(|| {
            scenario
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join("expected.json")
        });
        let output_dir = optional_path_flag(&args, "--output-dir")
            .unwrap_or_else(|| PathBuf::from("kitu-integration-runner/reports/latest"));

        Ok(Self {
            scenario,
            expected,
            output_dir,
        })
    }
}

fn required_path_flag(args: &[String], flag: &str) -> Result<PathBuf> {
    optional_path_flag(args, flag).with_context(|| format!("missing `{flag}`"))
}

fn optional_path_flag(args: &[String], flag: &str) -> Option<PathBuf> {
    args.windows(2)
        .find_map(|pair| (pair[0] == flag).then(|| PathBuf::from(&pair[1])))
        .or_else(|| {
            let prefix = format!("{flag}=");
            args.iter()
                .find_map(|arg| arg.strip_prefix(&prefix).map(PathBuf::from))
        })
}

fn print_help() {
    println!(
        "kitu-replay-runner --scenario <scenario.json> [--expected <expected.json>] [--output-dir <dir>]"
    );
}

#[derive(Debug, Deserialize)]
struct Scenario {
    schema_version: u32,
    scenario_id: String,
    steps: Vec<ScenarioStep>,
}

#[derive(Debug, Deserialize)]
struct ScenarioStep {
    at_tick: u64,
    inbound: Vec<InboundMessage>,
}

#[derive(Debug, Deserialize)]
struct InboundMessage {
    address: String,
    args: InboundArgs,
}

#[derive(Debug, Deserialize)]
struct InboundArgs {
    entity_id: String,
    x: f32,
    y: f32,
}

#[derive(Debug, Deserialize)]
struct Expected {
    schema_version: u32,
    scenario_id: String,
    expected_outputs: Vec<ExpectedOutput>,
    expected_summary: ExpectedSummary,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
struct ExpectedOutput {
    tick: u64,
    address: String,
    args: Value,
}

#[derive(Debug, Deserialize)]
struct ExpectedSummary {
    status: String,
    output_count: usize,
}

#[derive(Debug, Serialize, PartialEq)]
struct ReplaySummary {
    schema_version: u32,
    run_id: String,
    scenario_id: String,
    mode: String,
    status: String,
    started_at: String,
    finished_at: String,
    observed: ObservedSummary,
    files: SummaryFiles,
}

#[derive(Debug, Serialize, PartialEq)]
struct ObservedSummary {
    output_count: usize,
    mismatch_count: usize,
}

#[derive(Debug, Serialize, PartialEq)]
struct SummaryFiles {
    scenario: String,
    expected: String,
}

fn run_replay(scenario_path: &Path, expected_path: &Path) -> Result<ReplaySummary> {
    let scenario: Scenario = read_json(scenario_path)?;
    let expected: Expected = read_json(expected_path)?;
    validate_contract_pair(&scenario, &expected)?;

    let mut runtime = build_runtime(LocalChannel::default());
    let mut observed_outputs = Vec::new();
    let run_until_tick = run_until_tick(&scenario, &expected);

    for tick in 0..run_until_tick {
        for step in scenario.steps.iter().filter(|step| step.at_tick == tick) {
            for inbound in &step.inbound {
                runtime.enqueue_input(to_bundle(inbound)?);
            }
        }

        runtime.tick_once()?;
        let visible_tick = runtime.current_tick().get();
        for bundle in runtime.drain_output_buffer() {
            observed_outputs.extend(observed_from_bundle(visible_tick, &bundle)?);
        }
    }

    let mismatch_count = mismatch_count(&expected.expected_outputs, &observed_outputs);
    let output_count_matches = observed_outputs.len() == expected.expected_summary.output_count;
    let expected_status_allows_pass = expected.expected_summary.status == "pass";
    let status = if mismatch_count == 0 && output_count_matches && expected_status_allows_pass {
        "pass"
    } else {
        "fail"
    };

    Ok(ReplaySummary {
        schema_version: scenario.schema_version,
        run_id: format!("{}-{}", scenario.scenario_id, status),
        scenario_id: scenario.scenario_id,
        mode: "integration".to_string(),
        status: status.to_string(),
        started_at: "1970-01-01T00:00:00Z".to_string(),
        finished_at: "1970-01-01T00:00:00Z".to_string(),
        observed: ObservedSummary {
            output_count: observed_outputs.len(),
            mismatch_count,
        },
        files: SummaryFiles {
            scenario: scenario_path.display().to_string(),
            expected: expected_path.display().to_string(),
        },
    })
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let source = fs::read_to_string(path).with_context(|| format!("read `{}`", path.display()))?;
    serde_json::from_str(&source).with_context(|| format!("parse `{}`", path.display()))
}

fn validate_contract_pair(scenario: &Scenario, expected: &Expected) -> Result<()> {
    if scenario.schema_version != 1 || expected.schema_version != 1 {
        bail!("only schema_version 1 is supported");
    }
    if scenario.scenario_id != expected.scenario_id {
        bail!(
            "scenario_id mismatch: scenario `{}`, expected `{}`",
            scenario.scenario_id,
            expected.scenario_id
        );
    }
    Ok(())
}

fn run_until_tick(scenario: &Scenario, expected: &Expected) -> u64 {
    let last_step = scenario
        .steps
        .iter()
        .map(|step| step.at_tick + 1)
        .max()
        .unwrap_or(0);
    let last_expected = expected
        .expected_outputs
        .iter()
        .map(|output| output.tick)
        .max()
        .unwrap_or(0);
    last_step.max(last_expected)
}

fn to_bundle(inbound: &InboundMessage) -> Result<OscBundle> {
    if inbound.address != "/input/move" {
        bail!("unsupported inbound address `{}`", inbound.address);
    }

    let mut message = OscMessage::new(&inbound.address);
    message.push_arg(OscArg::Str(inbound.args.entity_id.clone()));
    message.push_arg(OscArg::Float(inbound.args.x));
    message.push_arg(OscArg::Float(inbound.args.y));

    let mut bundle = OscBundle::new();
    bundle.push(message);
    Ok(bundle)
}

fn observed_from_bundle(visible_tick: u64, bundle: &OscBundle) -> Result<Vec<ExpectedOutput>> {
    bundle
        .messages
        .iter()
        .map(|message| observed_from_message(visible_tick, message))
        .collect()
}

fn observed_from_message(visible_tick: u64, message: &OscMessage) -> Result<ExpectedOutput> {
    if message.address != "/render/player/transform" {
        bail!("unsupported outbound address `{}`", message.address);
    }
    if message.args.len() != 5 {
        bail!("{} expects five output args", message.address);
    }

    let entity_id = string_arg(&message.args[0], "entity_id")?;
    let tick = int64_arg(&message.args[1], "tick")?;
    let x = float_arg(&message.args[2], "x")?;
    let y = float_arg(&message.args[3], "y")?;
    let z = float_arg(&message.args[4], "z")?;

    Ok(ExpectedOutput {
        tick: visible_tick,
        address: message.address.clone(),
        args: serde_json::json!({
            "entity_id": entity_id,
            "tick": tick,
            "position": {
                "x": x,
                "y": y,
                "z": z
            }
        }),
    })
}

fn string_arg(arg: &OscArg, name: &str) -> Result<String> {
    match arg {
        OscArg::Str(value) => Ok(value.clone()),
        _ => bail!("{name} must be a string"),
    }
}

fn int64_arg(arg: &OscArg, name: &str) -> Result<i64> {
    match arg {
        OscArg::Int(value) => Ok(i64::from(*value)),
        OscArg::Int64(value) => Ok(*value),
        _ => bail!("{name} must be an integer"),
    }
}

fn float_arg(arg: &OscArg, name: &str) -> Result<f32> {
    match arg {
        OscArg::Float(value) => Ok(*value),
        OscArg::Int(value) => Ok(*value as f32),
        OscArg::Int64(value) => Ok(*value as f32),
        _ => bail!("{name} must be numeric"),
    }
}

fn mismatch_count(expected: &[ExpectedOutput], observed: &[ExpectedOutput]) -> usize {
    let pair_mismatches = expected
        .iter()
        .zip(observed)
        .filter(|(expected, observed)| !outputs_match(expected, observed))
        .count();
    let length_mismatches = expected.len().abs_diff(observed.len());
    pair_mismatches + length_mismatches
}

fn outputs_match(expected: &ExpectedOutput, observed: &ExpectedOutput) -> bool {
    expected.tick == observed.tick
        && expected.address == observed.address
        && args_match(&expected.args, &observed.args)
}

fn args_match(expected: &Value, observed: &Value) -> bool {
    let Some(expected_object) = expected.as_object() else {
        return expected == observed;
    };
    let Some(observed_object) = observed.as_object() else {
        return false;
    };

    for (key, expected_value) in expected_object {
        let Some(observed_value) = observed_object.get(key) else {
            return false;
        };

        if key == "position" {
            if !position_matches(expected_value, observed_value) {
                return false;
            }
        } else if expected_value != observed_value {
            return false;
        }
    }

    expected_object.len() == observed_object.len()
}

fn position_matches(expected: &Value, observed: &Value) -> bool {
    ["x", "y", "z"].iter().all(|axis| {
        let expected = expected.get(axis).and_then(Value::as_f64);
        let observed = observed.get(axis).and_then(Value::as_f64);
        match (expected, observed) {
            (Some(expected), Some(observed)) => (expected - observed).abs() <= FLOAT_TOLERANCE,
            _ => false,
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_required_cli_args() {
        let args = CliArgs::parse(vec![
            "--scenario".to_string(),
            "scenario.json".to_string(),
            "--expected=expected.json".to_string(),
            "--output-dir".to_string(),
            "reports/out".to_string(),
        ])
        .unwrap();

        assert_eq!(args.scenario, PathBuf::from("scenario.json"));
        assert_eq!(args.expected, PathBuf::from("expected.json"));
        assert_eq!(args.output_dir, PathBuf::from("reports/out"));
    }

    #[test]
    fn defaults_expected_next_to_scenario() {
        let args = CliArgs::parse(vec![
            "--scenario".to_string(),
            "scenarios/smoke/player-move-basic/scenario.json".to_string(),
        ])
        .unwrap();

        assert_eq!(
            args.expected,
            PathBuf::from("scenarios/smoke/player-move-basic/expected.json")
        );
    }

    #[test]
    fn converts_runtime_transform_output_to_expected_shape() {
        let mut message = OscMessage::new("/render/player/transform");
        message.push_arg(OscArg::Str("player:local".to_string()));
        message.push_arg(OscArg::Int64(0));
        message.push_arg(OscArg::Float(1.5));
        message.push_arg(OscArg::Float(2.0));
        message.push_arg(OscArg::Float(0.0));

        let observed = observed_from_message(1, &message).unwrap();

        assert_eq!(observed.tick, 1);
        assert_eq!(observed.address, "/render/player/transform");
        assert_eq!(
            observed.args,
            serde_json::json!({
                "entity_id": "player:local",
                "tick": 0,
                "position": {
                    "x": 1.5,
                    "y": 2.0,
                    "z": 0.0
                }
            })
        );
    }

    #[test]
    fn output_matching_allows_f32_rounding_differences() {
        let expected = ExpectedOutput {
            tick: 1,
            address: "/render/player/transform".to_string(),
            args: serde_json::json!({
                "entity_id": "player:local",
                "tick": 0,
                "position": {
                    "x": 0.1,
                    "y": 0.2,
                    "z": 0.0
                }
            }),
        };
        let observed = ExpectedOutput {
            tick: 1,
            address: "/render/player/transform".to_string(),
            args: serde_json::json!({
                "entity_id": "player:local",
                "tick": 0,
                "position": {
                    "x": 0.10000000149011612,
                    "y": 0.20000000298023224,
                    "z": 0.0
                }
            }),
        };

        assert!(outputs_match(&expected, &observed));
    }

    #[test]
    fn replay_summary_is_deterministic_for_checked_in_smoke_fixture() {
        let fixture_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../kitu-integration-runner/scenarios/smoke/player-move-basic");
        let scenario = fixture_root.join("scenario.json");
        let expected = fixture_root.join("expected.json");

        let first = run_replay(&scenario, &expected).unwrap();
        let second = run_replay(&scenario, &expected).unwrap();

        assert_eq!(first, second);
        assert_eq!(first.status, "pass");
        assert_eq!(first.observed.output_count, 1);
        assert_eq!(first.observed.mismatch_count, 0);
    }
}
