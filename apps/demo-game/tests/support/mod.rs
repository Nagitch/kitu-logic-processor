use kitu_demo_game::DemoRuntime;
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::InputMetadata;
use serde_json::Value;

pub fn send(runtime: &mut DemoRuntime, source: &str, id: u64, address: &str, args: Vec<OscArg>) {
    let mut message = OscMessage::new(address);
    message.args = args;
    let mut bundle = OscBundle::new();
    bundle.push(message);
    runtime
        .try_enqueue_input(
            bundle,
            Some(InputMetadata {
                source: source.into(),
                message_id: id,
                schema_version: 1,
            }),
        )
        .unwrap();
}

pub fn projection(runtime: &DemoRuntime) -> Value {
    let bundles = runtime.inspect_application();
    let OscArg::Str(json) = &bundles[0].messages[0].args[0] else {
        panic!("JSON projection");
    };
    serde_json::from_str(json).unwrap()
}

pub fn compare(expected: &Value, actual: &Value, path: &str) {
    match (expected, actual) {
        (Value::Object(expected), Value::Object(actual)) => {
            assert_eq!(
                expected.keys().collect::<Vec<_>>(),
                actual.keys().collect::<Vec<_>>(),
                "{path}: fields"
            );
            for (key, actual) in actual {
                compare(
                    expected
                        .get(key)
                        .unwrap_or_else(|| panic!("unknown projection field {path}/{key}")),
                    actual,
                    &format!("{path}/{key}"),
                );
            }
        }
        (Value::Array(expected), Value::Array(actual)) => {
            assert_eq!(expected.len(), actual.len(), "{path}: membership");
            for (index, (e, a)) in expected.iter().zip(actual).enumerate() {
                compare(e, a, &format!("{path}/{index}"));
            }
        }
        (Value::Number(e), Value::Number(a)) if e.as_i64().is_none() || a.as_i64().is_none() => {
            assert!(
                (e.as_f64().unwrap() - a.as_f64().unwrap()).abs() <= 1e-4,
                "{path}: expected {e}, actual {a}"
            );
        }
        _ => assert_eq!(expected, actual, "{path}"),
    }
}

pub fn replay_reference(name: &str, limit: usize) -> (usize, usize) {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../kitu-integration-runner/scenarios/arena/reference")
        .join(name);
    let scenario: Value =
        serde_json::from_str(&std::fs::read_to_string(directory.join("scenario.json")).unwrap())
            .unwrap();
    let checkpoints: Vec<Value> = std::fs::read_to_string(directory.join("expected.ndjson"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let outcomes: Vec<Value> = std::fs::read_to_string(directory.join("outcomes.ndjson"))
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    let mut runtime = kitu_demo_game::build_arena_runtime().unwrap();
    let mut observed = Vec::new();
    let mut compared = 0;
    for step in scenario["steps"].as_array().unwrap().iter().take(limit) {
        let tick = step["tick"].as_u64().unwrap();
        for command in step["commands"].as_array().unwrap() {
            let address = command["address"].as_str().unwrap();
            let fields: &[&str] = match address {
                "/input/arena/take" | "/input/arena/discard" | "/input/arena/upgrade" => {
                    &["itemId", "index"]
                }
                "/input/arena/equip" => &["itemId", "index", "slot"],
                "/input/arena/unequip" => &["itemId", "slot"],
                _ => &[],
            };
            let args = fields
                .iter()
                .map(|key| OscArg::Int(command[key].as_i64().unwrap() as i32))
                .collect();
            send(
                &mut runtime,
                "reference:commands",
                command["id"].as_u64().unwrap(),
                address,
                args,
            );
        }
        let f = &step["frame"];
        // Normalized C# traces do not contain raw frame IDs. Their generated frame
        // producer is distinct so the preserved recorded command IDs remain intact.
        send(
            &mut runtime,
            "reference:frames",
            tick + 1,
            "/input/arena/frame",
            vec![
                OscArg::Float(f["Move"]["x"].as_f64().unwrap() as f32),
                OscArg::Float(f["Move"]["y"].as_f64().unwrap() as f32),
                OscArg::Bool(f["HasAim"].as_bool().unwrap()),
                OscArg::Float(f["AimPoint"]["x"].as_f64().unwrap() as f32),
                OscArg::Float(f["AimPoint"]["y"].as_f64().unwrap() as f32),
                OscArg::Bool(f["FireA"].as_bool().unwrap()),
                OscArg::Bool(f["FireB"].as_bool().unwrap()),
            ],
        );
        for (index, key) in ["UseA", "UseB"].iter().enumerate() {
            if f[key].as_bool().unwrap() {
                send(
                    &mut runtime,
                    "reference:uses",
                    tick * 2 + index as u64 + 1,
                    "/input/arena/use",
                    vec![OscArg::Int(index as i32 + 2)],
                );
            }
        }
        runtime.tick_once().unwrap();
        for bundle in runtime.drain_output_buffer() {
            for message in bundle.messages {
                if message.address == "/ui/arena/command" {
                    let OscArg::Str(json) = &message.args[0] else {
                        panic!("JSON outcome");
                    };
                    let mut outcome: Value = serde_json::from_str(json).unwrap();
                    if outcome["source"] != "reference:commands" {
                        continue;
                    }
                    outcome.as_object_mut().unwrap().remove("source");
                    outcome.as_object_mut().unwrap().remove("sequence");
                    observed.push(outcome);
                }
            }
        }
        if let Some(expected) = checkpoints.iter().find(|value| value["tick"] == tick) {
            compare(
                expected,
                &projection(&runtime),
                &format!("{name}/tick/{tick}"),
            );
            compared += 1;
        }
    }
    let expected: Vec<_> = outcomes
        .into_iter()
        .filter(|value| value["tick"].as_u64().unwrap() < limit as u64)
        .collect();
    assert_eq!(expected, observed, "{name}: command result/order");
    (compared, observed.len())
}
