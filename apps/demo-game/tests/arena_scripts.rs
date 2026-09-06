#[allow(dead_code)]
mod support;
use kitu_demo_game::{
    arena::{
        self,
        script::{self, ScriptVersion},
    },
    build_arena_runtime, build_demo_runtime,
    replay::{Recorder, Session},
};
use kitu_tsq1::recording::Recording;

#[test]
fn saved_initial_and_staged_sources_replay_after_authoring_edit_and_deletion() {
    let path = std::env::temp_dir().join(format!("arena-boss-source-{}.rhai", std::process::id()));
    let first = script::DEFAULT_SOURCE.replace("duration: 0.8", "duration: 1.2");
    std::fs::write(&path, &first).unwrap();
    let initial = script::load_script(&path).unwrap();
    let content = arena::inspect_content(&build_arena_runtime().unwrap())
        .unwrap()
        .pending;
    let mut runtime = build_demo_runtime().unwrap();
    arena::install_with_versions(&mut runtime, content, initial.clone()).unwrap();
    let mut recorder = Recorder::new(&runtime).unwrap();
    support::send(&mut runtime, "unity", 1, "/input/arena/start", vec![]);
    runtime.tick_once().unwrap();
    let output = runtime.drain_output_buffer();
    recorder.capture(&runtime, &output).unwrap();
    let second = first.replace("duration: 1.2", "duration: 2.4");
    std::fs::write(&path, &second).unwrap();
    let edited = script::load_script(&path).unwrap();
    arena::stage_script(&mut runtime, edited.clone(), 1).unwrap();
    arena::stage_script(&mut runtime, edited.clone(), 1).unwrap();
    support::send(&mut runtime, "unity", 2, "/input/arena/menu", vec![]);
    support::send(&mut runtime, "unity", 3, "/input/arena/start", vec![]);
    runtime.tick_once().unwrap();
    let output = runtime.drain_output_buffer();
    recorder.capture(&runtime, &output).unwrap();
    let bytes = recorder.encode().unwrap();
    std::fs::remove_file(&path).unwrap();
    let session = Session::decode(&bytes).unwrap();
    assert_eq!(session.manifest().version, 2);
    assert_eq!(session.manifest().initial_script, initial);
    assert_eq!(session.manifest().runs[0]["script"]["source"], first);
    assert_eq!(session.manifest().runs[1]["script"]["source"], second);
    assert_eq!(
        session.verify().unwrap().state,
        support::projection(&runtime)
    );
    let mut tampered = Recording::decode(&bytes).unwrap();
    tampered.manifest["initialScript"]["policyVersion"] = "untrusted".into();
    assert!(Session::decode(&tampered.encode().unwrap()).is_err());
    let mut tampered = Recording::decode(&bytes).unwrap();
    tampered
        .manifest
        .as_object_mut()
        .unwrap()
        .remove("initialScript");
    assert!(Session::decode(&tampered.encode().unwrap()).is_err());
}

#[test]
fn conditional_late_boss_fault_is_recorded_and_reexecuted_from_detached_source() {
    let version = ScriptVersion::from_source(&script::DEFAULT_SOURCE.replace(
        "fn boss(input) {",
        "fn boss(input) { if input.floor == 10 { throw \"floor ten fault\"; }",
    ))
    .unwrap();
    let content = arena::inspect_content(&build_arena_runtime().unwrap())
        .unwrap()
        .pending;
    let mut actual = build_demo_runtime().unwrap();
    arena::install_with_versions(&mut actual, content, version.clone()).unwrap();
    let mut recorder = Recorder::new(&actual).unwrap();
    let mut fault_tick = None;
    let mut compared_boss_ticks = 0;
    support::replay_reference_observing_ticks(
        "stock-eleven-death-retry",
        5528,
        |_| {},
        |reference, expected| {
            if fault_tick.is_some() {
                return;
            }
            for input in reference.committed_input_records() {
                actual
                    .try_enqueue_input(input.bundle, input.metadata)
                    .unwrap();
            }
            actual.tick_once().unwrap();
            let output = actual.drain_output_buffer();
            recorder.capture(&actual, &output).unwrap();
            let fault = arena::inspect_script(&actual).unwrap().fault;
            if let Some(fault) = fault {
                fault_tick = Some(fault.tick);
                assert_eq!(support::projection(&actual)["floor"], 10);
                assert_eq!(support::projection(&actual)["overlay"], "pause");
                assert_eq!(fault.script_hash, version.hash);
            } else {
                assert_eq!(support::projection(&actual), support::projection(reference));
                // Domain events and their absolute order remain equal even though the
                // detached script source metadata intentionally differs.
                let gameplay = |bundles: &[kitu_osc_ir::OscBundle]| {
                    bundles
                        .iter()
                        .flat_map(|bundle| &bundle.messages)
                        .filter(|message| {
                            message.address.starts_with("/game/")
                                && message.address != "/game/arena/run"
                        })
                        .cloned()
                        .collect::<Vec<_>>()
                };
                assert_eq!(gameplay(&output), gameplay(expected));
                if support::projection(&actual)["floor"] == 5 {
                    compared_boss_ticks += 1;
                }
            }
        },
    );
    assert!(fault_tick.is_some());
    assert!(compared_boss_ticks > 100);
    // Empty ticks after the fault preserve the paused state and never rerun it.
    for _ in 0..3 {
        actual.tick_once().unwrap();
        let output = actual.drain_output_buffer();
        recorder.capture(&actual, &output).unwrap();
    }
    let saved = Session::decode(&recorder.encode().unwrap()).unwrap();
    assert_eq!(saved.verify().unwrap().state, support::projection(&actual));
    assert_eq!(saved.manifest().initial_script, version);
}

#[test]
fn recording_version_bound_is_symmetric_and_prepared_sources_survive_cache_churn() {
    use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
    use kitu_runtime::InputMetadata;
    use kitu_tsq1::recording::TimedBundle;
    use serde_json::json;
    let mut runtime = build_arena_runtime().unwrap();
    let mut recorder = Recorder::new(&runtime).unwrap();
    let versions = (0..64)
        .map(|index| {
            ScriptVersion::from_source(&format!(
                "// replay bound {index}\n{}",
                script::DEFAULT_SOURCE
            ))
            .unwrap()
        })
        .collect::<Vec<_>>();
    for (index, version) in versions[..63].iter().enumerate() {
        arena::stage_script(&mut runtime, version.clone(), index as u64 + 1).unwrap();
    }
    runtime.tick_once().unwrap();
    let output = runtime.drain_output_buffer();
    recorder.capture(&runtime, &output).unwrap();
    let prefix = recorder.encode().unwrap();
    let saved = Session::decode(&prefix).unwrap();
    for index in 0..40 {
        ScriptVersion::from_source(&format!(
            "// unrelated cache churn {index}\n{}",
            script::DEFAULT_SOURCE
        ))
        .unwrap();
    }
    // Retained initial and staged programs enter the normal queue without needing
    // any of the globally cached source versions during replay runtime creation.
    assert_eq!(saved.verify().unwrap().state, support::projection(&runtime));
    arena::stage_script(&mut runtime, versions[63].clone(), 64).unwrap();
    runtime.tick_once().unwrap();
    let output = runtime.drain_output_buffer();
    assert!(recorder
        .capture(&runtime, &output)
        .unwrap_err()
        .to_string()
        .contains("64 script versions"));
    assert_eq!(recorder.encode().unwrap(), prefix);
    let mut invalid = Recording::decode(&prefix).unwrap();
    invalid.manifest["ticks"] = 2.into();
    invalid.manifest["proofs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"state":"0".repeat(64),"output":"0".repeat(64)}));
    let mut message = OscMessage::new("/input/arena/script");
    message
        .args
        .push(OscArg::Str(serde_json::to_string(&versions[63]).unwrap()));
    invalid.entries.push(TimedBundle { tick: 1, order: 0,
        metadata: json!({"sequence":63,"identity":InputMetadata {source:"host:arena-script".into(),message_id:64,schema_version:1}}),
        bundle: OscBundle { messages: vec![message] },
    });
    assert!(Session::decode(&invalid.encode().unwrap())
        .err()
        .unwrap()
        .to_string()
        .contains("64 script versions"));
}
