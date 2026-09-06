//! Exercises the exported C boundary with the full frozen C# scenarios.
#[allow(dead_code)]
#[path = "../../tests/support/mod.rs"]
mod support;

use kitu_demo_game::{arena, build_arena_runtime, build_demo_runtime, DemoRuntime};
use kitu_demo_game_native::*;
use kitu_osc_ir::{OscArg, OscBundle};
use kitu_runtime::InputMetadata;
use kitu_transport::wire::WireBundle;
use kitu_unity_ffi::application::{ApplicationHandle, BUFFER_TOO_SMALL, EMPTY, OK, PENDING_OUTPUT};
use serde_json::{json, Value};
use std::{fs::File, io::Write, path::Path, ptr};

type Read = unsafe extern "C" fn(*mut ApplicationHandle, *mut u8, usize, *mut usize) -> i32;

struct Native(*mut ApplicationHandle);
impl Native {
    fn create(config: &[u8]) -> Self {
        let mut handle = ptr::null_mut();
        let mut error = [0u8; 1024];
        let mut required = 0;
        let code = unsafe {
            kitu_application_create(
                1,
                config.as_ptr(),
                config.len(),
                &mut handle,
                error.as_mut_ptr(),
                error.len(),
                &mut required,
            )
        };
        assert_eq!(
            code,
            OK,
            "{}",
            String::from_utf8_lossy(&error[..required.min(error.len())])
        );
        assert!(!handle.is_null());
        Self(handle)
    }
    fn read(&self, read: Read) -> Vec<u8> {
        let mut required = 0;
        assert_eq!(
            unsafe { read(self.0, ptr::null_mut(), 0, &mut required) },
            BUFFER_TOO_SMALL
        );
        assert!(required > 0);
        let mut bytes = vec![0x55; required];
        assert_eq!(
            unsafe { read(self.0, bytes.as_mut_ptr(), required - 1, &mut required) },
            BUFFER_TOO_SMALL
        );
        assert!(
            bytes.iter().all(|byte| *byte == 0x55),
            "short read must not partially write"
        );
        assert_eq!(
            unsafe { read(self.0, bytes.as_mut_ptr(), bytes.len(), &mut required) },
            OK
        );
        bytes
    }
    fn submit(
        &self,
        bundle: &OscBundle,
        metadata: Option<InputMetadata>,
        sequence: u64,
    ) -> Vec<u8> {
        let bytes = serde_json::to_vec(
            &json!({"bundle":WireBundle::try_from(bundle).unwrap(), "metadata":metadata}),
        )
        .unwrap();
        let mut actual = u64::MAX;
        assert_eq!(
            unsafe {
                kitu_application_submit_json(self.0, bytes.as_ptr(), bytes.len(), &mut actual)
            },
            OK
        );
        assert_eq!(actual, sequence);
        bytes
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        assert_eq!(unsafe { kitu_application_destroy(self.0) }, OK);
    }
}

fn wire(bundles: &[OscBundle]) -> Value {
    serde_json::to_value(
        bundles
            .iter()
            .map(|bundle| WireBundle::try_from(bundle).unwrap())
            .collect::<Vec<_>>(),
    )
    .unwrap()
}
fn state(bundles: &Value) -> Value {
    serde_json::from_str(
        bundles[0]["messages"][0]["args"][0]["value"]
            .as_str()
            .unwrap(),
    )
    .unwrap()
}
fn compare_tick(native: &Native, runtime: &DemoRuntime, outputs: &[OscBundle]) -> Value {
    assert_eq!(unsafe { kitu_application_tick(native.0) }, OK);
    assert_eq!(unsafe { kitu_application_tick(native.0) }, PENDING_OUTPUT);
    let actual: Value = serde_json::from_slice(&native.read(kitu_application_read_output)).unwrap();
    assert_eq!(
        actual,
        wire(outputs),
        "complete output bundle/type/order mismatch"
    );
    let mut length = 999;
    assert_eq!(
        unsafe { kitu_application_read_output(native.0, ptr::null_mut(), 0, &mut length) },
        EMPTY
    );
    assert_eq!(length, 0);
    let projection: Value =
        serde_json::from_slice(&native.read(kitu_application_inspect_json)).unwrap();
    assert_eq!(
        projection,
        wire(&runtime.inspect_application()),
        "complete state projection mismatch"
    );
    assert_eq!(
        native.read(kitu_application_inspect_json),
        serde_json::to_vec(&projection).unwrap()
    );
    json!({"tick":runtime.current_tick().get()-1, "output":actual, "state":projection})
}

fn full_scenario(name: &str, expected_counts: (usize, usize)) {
    let native = Native::create(&[]);
    let initial: Value =
        serde_json::from_slice(&native.read(kitu_application_inspect_json)).unwrap();
    assert_eq!(state(&initial)["tick"], -1);
    assert_eq!(
        initial,
        wire(&build_arena_runtime().unwrap().inspect_application())
    );
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../kitu-integration-runner/scenarios/arena/reference")
        .join(name);
    let mut evidence = std::env::var_os("KITU_NATIVE_EVIDENCE_DIR").map(|directory| {
        std::fs::create_dir_all(&directory).unwrap();
        let path = Path::new(&directory);
        (
            File::create(path.join(format!("{name}.trace"))).unwrap(),
            File::create(path.join(format!("{name}.expected.ndjson"))).unwrap(),
        )
    });
    let mut ticks = 0;
    let mut inputs = 0;
    let counts = support::replay_reference_from_directory(
        &directory,
        name,
        usize::MAX,
        |_| {},
        |runtime, outputs| {
            for input in runtime.committed_input_records() {
                let bytes = native.submit(&input.bundle, input.metadata.clone(), input.sequence);
                if let Some((trace, _)) = &mut evidence {
                    trace.write_all(b"I").unwrap();
                    trace.write_all(&bytes).unwrap();
                    trace.write_all(b"\n").unwrap();
                }
                inputs += 1;
            }
            let result = compare_tick(&native, runtime, outputs);
            if name == "stock-eleven-death-retry" && ticks == 5526 {
                let value = state(&result["state"]);
                assert_eq!(value["phase"], 5);
                assert_eq!(value["floor"], 11);
                assert_eq!(value["inventory"]["health"], 0);
            }
            if let Some((trace, expected)) = &mut evidence {
                trace.write_all(b"T\n").unwrap();
                serde_json::to_writer(&mut *expected, &result).unwrap();
                expected.write_all(b"\n").unwrap();
            }
            ticks += 1;
        },
    );
    assert_eq!(counts, expected_counts);
    if name == "stock-eleven-death-retry" {
        assert_eq!((ticks, inputs), (5528, 5581));
    }
}

#[test]
fn complete_stock_run_matches_c_abi_server_and_frozen_csharp_oracle() {
    full_scenario("stock-eleven-death-retry", (550, 53));
}
#[test]
fn preparation_inventory_and_equipment_match_c_abi_and_frozen_csharp_oracle() {
    full_scenario("preparation", (12, 12));
}

#[test]
fn native_uses_detached_validated_content_and_refuses_incompatible_configuration() {
    let mut values = arena::config::ArenaConfig::default();
    values.items[0].damage = 37;
    let content = arena::config::ContentVersion::from_tmd(&values.to_tmd().unwrap()).unwrap();
    let native = Native::create(
        &serde_json::to_vec(&json!({"contractVersion":1,"content":content})).unwrap(),
    );
    let mut runtime = build_demo_runtime().unwrap();
    arena::install_with_content(&mut runtime, content.clone()).unwrap();
    support::send(&mut runtime, "native-test", 1, "/input/arena/start", vec![]);
    runtime.tick_once().unwrap();
    for input in runtime.committed_input_records() {
        native.submit(&input.bundle, input.metadata.clone(), input.sequence);
    }
    let output = runtime.drain_output_buffer();
    compare_tick(&native, &runtime, &output);
    for config in [json!({"contractVersion":2}), json!({"unknown":true}), {
        let mut invalid = content;
        invalid.hash = "0".repeat(64);
        json!({"content":invalid})
    }] {
        let bytes = serde_json::to_vec(&config).unwrap();
        let mut handle = ptr::null_mut();
        let mut required = 0;
        assert!(
            unsafe {
                kitu_application_create(
                    1,
                    bytes.as_ptr(),
                    bytes.len(),
                    &mut handle,
                    ptr::null_mut(),
                    0,
                    &mut required,
                )
            } < 0
        );
        assert!(handle.is_null());
        assert!(required > 0);
    }
}

#[test]
fn inputs_do_not_advance_time_and_duplicate_operations_keep_server_receipts() {
    let native = Native::create(&[]);
    let mut runtime = build_arena_runtime().unwrap();
    let before = native.read(kitu_application_inspect_json);
    for _ in 0..2 {
        support::send(&mut runtime, "retry", 1, "/input/arena/start", vec![]);
    }
    runtime.tick_once().unwrap();
    for input in runtime.committed_input_records() {
        native.submit(&input.bundle, input.metadata.clone(), input.sequence);
    }
    assert_eq!(native.read(kitu_application_inspect_json), before);
    let output = runtime.drain_output_buffer();
    assert_eq!(
        output
            .iter()
            .flat_map(|b| &b.messages)
            .filter(|m| m.address == "/game/arena/run")
            .count(),
        1
    );
    compare_tick(&native, &runtime, &output);
    // Invalid contract input cannot consume the next sequence or advance either clock.
    let bad = serde_json::to_vec(&json!({"metadata":{"source":"retry","messageId":2,"schemaVersion":99},"bundle":{"messages":[{"address":"/input/arena/menu","args":[]}]}})).unwrap();
    let mut sequence = u64::MAX;
    assert!(
        unsafe { kitu_application_submit_json(native.0, bad.as_ptr(), bad.len(), &mut sequence) }
            < 0
    );
    support::send(&mut runtime, "retry", 2, "/input/arena/pause", vec![]);
    runtime.tick_once().unwrap();
    for input in runtime.committed_input_records() {
        native.submit(&input.bundle, input.metadata.clone(), input.sequence);
    }
    let output = runtime.drain_output_buffer();
    compare_tick(&native, &runtime, &output);
    let elapsed = state(&wire(&runtime.inspect_application()))["elapsed"].clone();
    for _ in 0..5 {
        runtime.tick_once().unwrap();
        let output = runtime.drain_output_buffer();
        let result = compare_tick(&native, &runtime, &output);
        assert_eq!(state(&result["state"])["elapsed"], elapsed);
    }
    // Ordinary ordered generic movement retains its string/float types through the same C boundary.
    let mut bundle = OscBundle::new();
    let mut message = kitu_osc_ir::OscMessage::new("/input/move");
    message.args = vec![
        OscArg::Str("marker".into()),
        OscArg::Float(-0.0),
        OscArg::Float(2.0),
    ];
    bundle.push(message);
    let seq = runtime.try_enqueue_input(bundle.clone(), None).unwrap();
    native.submit(&bundle, None, seq);
    runtime.tick_once().unwrap();
    let output = runtime.drain_output_buffer();
    compare_tick(&native, &runtime, &output);
}

#[test]
fn native_consumable_intents_cancel_on_pause_and_duplicate_use_consumes_once() {
    fn advance(native: &Native, runtime: &mut DemoRuntime) -> Value {
        runtime.tick_once().unwrap();
        for input in runtime.committed_input_records() {
            native.submit(&input.bundle, input.metadata.clone(), input.sequence);
        }
        let output = runtime.drain_output_buffer();
        compare_tick(native, runtime, &output)
    }
    fn command(runtime: &mut DemoRuntime, id: u64, name: &str, args: Vec<OscArg>) {
        support::send(
            runtime,
            "consumables",
            id,
            &format!("/input/arena/{name}"),
            args,
        );
    }
    let native = Native::create(&[]);
    let mut runtime = build_arena_runtime().unwrap();
    command(&mut runtime, 1, "start", vec![]);
    let length = 58.0_f32.sqrt();
    support::send(
        &mut runtime,
        "frames",
        1,
        "/input/arena/frame",
        vec![
            OscArg::Float(-3.0 / length),
            OscArg::Float(7.0 / length),
            OscArg::Bool(true),
            OscArg::Float(0.0),
            OscArg::Float(5.0),
            OscArg::Bool(false),
            OscArg::Bool(false),
        ],
    );
    for _ in 0..75 {
        advance(&native, &mut runtime);
    }
    for (id, name, args) in [
        (2, "chest", vec![]),
        (3, "take", vec![OscArg::Int(9), OscArg::Int(0)]),
        (
            4,
            "equip",
            vec![OscArg::Int(9), OscArg::Int(0), OscArg::Int(2)],
        ),
        (5, "close", vec![]),
    ] {
        command(&mut runtime, id, name, args);
    }
    advance(&native, &mut runtime);
    command(&mut runtime, 6, "use", vec![OscArg::Int(2)]);
    command(&mut runtime, 7, "pause", vec![]);
    advance(&native, &mut runtime);
    command(&mut runtime, 8, "resume", vec![]);
    let resumed = advance(&native, &mut runtime);
    assert_eq!(
        state(&resumed["state"])["inventory"]["equipment"][2]["id"],
        9
    );
    support::send(
        &mut runtime,
        "frames",
        2,
        "/input/arena/frame",
        vec![
            OscArg::Float(0.0),
            OscArg::Float(0.0),
            OscArg::Bool(true),
            OscArg::Float(0.0),
            OscArg::Float(5.0),
            OscArg::Bool(false),
            OscArg::Bool(false),
        ],
    );
    for id in [9, 9, 10] {
        command(&mut runtime, id, "use", vec![OscArg::Int(2)]);
    }
    let used = advance(&native, &mut runtime);
    assert_eq!(
        state(&used["state"])["grenades"].as_array().unwrap().len(),
        1
    );
    assert_eq!(state(&used["state"])["inventory"]["equipment"][2]["id"], 0);
    let addresses: Vec<_> = used["output"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|b| b["messages"].as_array().unwrap())
        .map(|m| m["address"].as_str().unwrap())
        .collect();
    assert_eq!(
        addresses.iter().filter(|a| **a == "/ui/arena/use").count(),
        1
    );
    assert_eq!(
        addresses
            .iter()
            .filter(|a| **a == "/game/arena/inventory")
            .count(),
        1
    );
    command(&mut runtime, 10, "use", vec![OscArg::Int(2)]);
    let duplicate = advance(&native, &mut runtime);
    assert!(!duplicate["output"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|b| b["messages"].as_array().unwrap())
        .any(|m| m["address"] == "/ui/arena/use"));
    for _ in 0..35 {
        advance(&native, &mut runtime);
    }
    assert!(support::projection(&runtime)["grenades"]
        .as_array()
        .unwrap()
        .is_empty());
}
