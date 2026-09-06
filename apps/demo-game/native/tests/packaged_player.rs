//! Exports an initial-package Player oracle without staging any replacement versions.
#[allow(dead_code)]
#[path = "../../tests/support/mod.rs"]
mod support;

use kitu_demo_game::{arena, build_demo_runtime};
use kitu_demo_game_native::*;
use kitu_osc_ir::{OscArg, OscBundle};
use kitu_transport::wire::WireBundle;
use kitu_unity_ffi::application::{ApplicationHandle, BUFFER_TOO_SMALL, OK};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, io::Write, path::PathBuf, ptr};

struct Native(*mut ApplicationHandle);
impl Native {
    fn create(config: Value) -> Self {
        let bytes = serde_json::to_vec(&config).unwrap();
        let mut handle = ptr::null_mut();
        let mut diagnostic = [0; 4096];
        let mut required = 0;
        let result = unsafe {
            kitu_application_create(
                1,
                bytes.as_ptr(),
                bytes.len(),
                &mut handle,
                diagnostic.as_mut_ptr(),
                diagnostic.len(),
                &mut required,
            )
        };
        assert_eq!(
            result,
            OK,
            "{}",
            String::from_utf8_lossy(&diagnostic[..required.min(diagnostic.len())])
        );
        assert!(!handle.is_null());
        Self(handle)
    }
    fn read(
        &self,
        reader: unsafe extern "C" fn(*mut ApplicationHandle, *mut u8, usize, *mut usize) -> i32,
    ) -> Value {
        let mut required = 0;
        assert_eq!(
            unsafe { reader(self.0, ptr::null_mut(), 0, &mut required) },
            BUFFER_TOO_SMALL
        );
        let mut bytes = vec![0; required];
        assert_eq!(
            unsafe { reader(self.0, bytes.as_mut_ptr(), bytes.len(), &mut required) },
            OK
        );
        serde_json::from_slice(&bytes).unwrap()
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

#[test]
fn edited_packaged_sources_drive_the_first_run_and_export_a_player_oracle() {
    use arena::package::{PackageFile, PackageManifest, DEFAULT_UNITY_ASSETS, FILE_PATHS};
    use kitu_tsq1::presentation::Clip;
    let retained = std::env::var_os("KITU_PACKAGED_PLAYER_EVIDENCE_DIR").map(PathBuf::from);
    let root = retained.clone().unwrap_or_else(|| {
        std::env::temp_dir().join(format!(
            "kitu-packaged-player-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    });
    fs::create_dir_all(&root).unwrap();
    let package_dir = root.join("edited-package");
    fs::create_dir_all(package_dir.join("timelines")).unwrap();
    let mut values = arena::config::ArenaConfig::default();
    values.items[0].damage = 32;
    let tmd = values.to_tmd().unwrap();
    let script_source = arena::script::DEFAULT_SOURCE.replace("duration: 0.8", "duration: 1.6");
    let default_timeline = arena::presentation::default_timeline().unwrap();
    let mut boss = Clip::decode(&default_timeline.clips[0].bytes).unwrap();
    for event in &mut boss.events {
        for message in &mut event.bundle.messages {
            message.args[0] = OscArg::Float(4.5);
        }
    }
    let mut floor = Clip::decode(&default_timeline.clips[1].bytes).unwrap();
    for event in &mut floor.events {
        if event.offset_tick == 12 {
            for message in &mut event.bundle.messages {
                message.args[0] = OscArg::Float(0.85);
            }
        }
    }
    let boss_bytes = boss.encode().unwrap();
    let floor_bytes = floor.encode().unwrap();
    let sources = [
        DEFAULT_UNITY_ASSETS,
        tmd.as_slice(),
        script_source.as_bytes(),
        boss_bytes.as_slice(),
        floor_bytes.as_slice(),
    ];
    let mut files = Vec::new();
    for (path, bytes) in FILE_PATHS.into_iter().zip(sources) {
        fs::write(package_dir.join(path), bytes).unwrap();
        files.push(PackageFile {
            path: path.into(),
            bytes: bytes.len() as u64,
            sha256: Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>(),
        });
    }
    let mut manifest = serde_json::to_vec(&PackageManifest {
        schema_version: 1,
        files,
    })
    .unwrap();
    manifest.push(b'\n');
    fs::write(package_dir.join("package.json"), manifest).unwrap();
    let package = arena::package::load_package(&package_dir).unwrap();
    let mut runtime = build_demo_runtime().unwrap();
    // The oracle uses independent detached constructors, not native initialization.
    arena::install_with_all_versions(
        &mut runtime,
        arena::config::ContentVersion::from_tmd(&tmd).unwrap(),
        arena::script::ScriptVersion::from_source(&script_source).unwrap(),
        arena::presentation::TimelineVersion::from_sources(&boss_bytes, &floor_bytes).unwrap(),
    )
    .unwrap();
    let native = Native::create(
        json!({"bundledContentDirectory":package_dir,"storageDirectory":root.join("storage"),"bridge":{"enabled":false}}),
    );
    let initial = native.read(kitu_application_inspect_json);
    assert_eq!(initial, wire(&runtime.inspect_application()));
    assert_eq!(state(&initial)["tick"], -1);
    let initial_host = native.read(kitu_application_inspect_host_json);
    let host: Value = serde_json::from_str(
        initial_host[0]["messages"][0]["args"][0]["value"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        host["package"],
        serde_json::to_value(&package.identity).unwrap()
    );
    let mut trace = fs::File::create(root.join("bundled-edited.trace")).unwrap();
    let mut expected = fs::File::create(root.join("bundled-edited.expected.ndjson")).unwrap();
    fs::write(
        root.join("bundled-edited.initial.json"),
        serde_json::to_vec_pretty(&json!({
            "state":initial, "package":package.identity,
        }))
        .unwrap(),
    )
    .unwrap();
    let mut recorder = kitu_demo_game::replay::Recorder::new(&runtime).unwrap();
    let mut telegraph_ticks = 0;
    let mut floor_cue = false;
    let mut boss_cue = false;
    let mut damage_in_inventory = false;
    let mut inputs = 0;
    let mut staging_inputs = 0;
    let reference = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(
        "../../../kitu-integration-runner/scenarios/arena/reference/stock-eleven-death-retry",
    );
    support::replay_reference_from_directory(
        &reference,
        "stock-eleven-death-retry",
        1800,
        |_| {},
        |reference, _| {
            for input in reference.committed_input_records() {
                staging_inputs += input
                    .bundle
                    .messages
                    .iter()
                    .filter(|message| {
                        matches!(
                            message.address.as_str(),
                            "/input/arena/config" | "/input/arena/script" | "/input/arena/timeline"
                        )
                    })
                    .count();
                assert_eq!(
                    staging_inputs, 0,
                    "initial package proof cannot stage versions"
                );
                let sequence = runtime
                    .try_enqueue_input(input.bundle.clone(), input.metadata.clone())
                    .unwrap();
                let bytes = serde_json::to_vec(&json!({"metadata":input.metadata,"bundle":WireBundle::try_from(&input.bundle).unwrap()})).unwrap();
                let mut actual = u64::MAX;
                assert_eq!(
                    unsafe {
                        kitu_application_submit_json(
                            native.0,
                            bytes.as_ptr(),
                            bytes.len(),
                            &mut actual,
                        )
                    },
                    OK
                );
                assert_eq!(actual, sequence);
                trace.write_all(b"I").unwrap();
                trace.write_all(&bytes).unwrap();
                trace.write_all(b"\n").unwrap();
                inputs += 1;
            }
            runtime.tick_once().unwrap();
            let output = runtime.drain_output_buffer();
            recorder.capture(&runtime, &output).unwrap();
            assert_eq!(unsafe { kitu_application_tick(native.0) }, OK);
            let actual_output = native.read(kitu_application_read_output);
            let actual_state = native.read(kitu_application_inspect_json);
            assert_eq!(actual_output, wire(&output));
            assert_eq!(actual_state, wire(&runtime.inspect_application()));
            let observed = state(&actual_state);
            let inventory = &observed["inventory"];
            for group in ["chest", "backpack", "equipment"] {
                damage_in_inventory |= inventory[group]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item["kind"] == 0 && item["damage"] == 32);
            }
            for enemy in observed["enemies"].as_array().unwrap() {
                if observed["floor"] == 5 && enemy["Kind"] == 3 && enemy["BossState"] == 1 {
                    if telegraph_ticks == 0 {
                        assert!((enemy["PhaseRemaining"].as_f64().unwrap() - 1.6).abs() < 1e-6);
                    }
                    telegraph_ticks += 1;
                }
            }
            let presentation = arena::inspect_timeline(&runtime).unwrap().presentation;
            if let Some(cue) = presentation.floor {
                if cue.offset_tick == 12 {
                    assert_eq!(cue.opacity, 0.85);
                    floor_cue = true;
                }
            }
            for cue in presentation.bosses {
                if cue.offset_tick == 24 {
                    assert_eq!(cue.radius, 4.5);
                    boss_cue = true;
                }
            }
            trace.write_all(b"T\n").unwrap();
            serde_json::to_writer(&mut expected, &json!({"tick":runtime.current_tick().get()-1,"state":actual_state,"output":actual_output})).unwrap();
            expected.write_all(b"\n").unwrap();
        },
    );
    assert!(damage_in_inventory && floor_cue && boss_cue);
    assert_eq!(telegraph_ticks, 96);
    let recording = recorder.encode().unwrap();
    assert_eq!(
        kitu_demo_game::replay::Session::decode(&recording)
            .unwrap()
            .verify()
            .unwrap()
            .ticks,
        1800
    );
    fs::write(root.join("bundled-edited.tsq"), recording).unwrap();
    fs::write(
        root.join("bundled-edited.result.json"),
        serde_json::to_vec_pretty(&json!({
            "ticks":1800,"inputs":inputs,"completeOutputAndStateMatched":true,"stagingInputs":staging_inputs,
            "weaponDamage":32,"telegraphTicks":telegraph_ticks,"floorOpacity":0.85,"bossRadius":4.5,
            "package":package.identity,"initialInspectionTick":-1,"replayVerified":true,
        }))
        .unwrap(),
    )
    .unwrap();
    drop(native);
    drop(trace);
    drop(expected);
    if retained.is_none() {
        fs::remove_dir_all(root).unwrap();
    }
}
