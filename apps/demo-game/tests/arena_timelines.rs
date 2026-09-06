#[allow(dead_code)]
mod support;
use kitu_demo_game::{
    arena::{
        self,
        presentation::{self, TimelineVersion},
    },
    build_arena_runtime,
    replay::{Recorder, Session},
};
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_tsq1::{
    presentation::Clip,
    recording::{Recording, TimedBundle},
};
use serde_json::{json, Value};

fn edited(radius: f32) -> TimelineVersion {
    let mut clip = Clip::decode(presentation::DEFAULT_BOSS).unwrap();
    for event in &mut clip.events {
        event.bundle.messages[0].args[0] = OscArg::Float(radius);
    }
    TimelineVersion::from_sources(&clip.encode().unwrap(), presentation::DEFAULT_FLOOR).unwrap()
}
fn capture(runtime: &mut kitu_demo_game::DemoRuntime, recorder: &mut Recorder) -> Vec<OscBundle> {
    runtime.tick_once().unwrap();
    let outputs = runtime.drain_output_buffer();
    recorder.capture(runtime, &outputs).unwrap();
    outputs
}
#[test]
fn next_run_adoption_duplicate_stale_and_invalid_candidates_keep_valid_versions() {
    let mut runtime = build_arena_runtime().unwrap();
    let initial = arena::inspect_timeline(&runtime).unwrap().pending;
    assert_eq!(
        arena::inspect_timeline(&runtime).unwrap().presentation.tick,
        -1
    );
    support::send(&mut runtime, "unity", 1, "/input/arena/start", vec![]);
    runtime.tick_once().unwrap();
    runtime.drain_output_buffer();
    let version = edited(4.5);
    arena::stage_timeline(&mut runtime, version.clone(), 2).unwrap();
    arena::stage_timeline(&mut runtime, version.clone(), 2).unwrap();
    arena::stage_timeline(&mut runtime, edited(2.0), 1).unwrap();
    runtime.tick_once().unwrap();
    let output = runtime.drain_output_buffer();
    let receipts = output
        .iter()
        .flat_map(|b| &b.messages)
        .filter(|m| m.address == "/ui/arena/command")
        .map(|m| {
            let OscArg::Str(s) = &m.args[0] else { panic!() };
            serde_json::from_str::<Value>(s).unwrap()
        })
        .collect::<Vec<_>>();
    assert_eq!(receipts[0]["accepted"], true);
    assert_eq!(receipts[1]["duplicate"], true);
    assert_eq!(receipts[2]["code"], "id_conflict");
    let snapshot = arena::inspect_timeline(&runtime).unwrap();
    assert_eq!(snapshot.pending, version);
    assert_eq!(snapshot.active, Some(initial));
    let mut invalid = version.clone();
    invalid.hash = "0".repeat(64);
    assert!(arena::stage_timeline(&mut runtime, invalid, 3).is_err());
    let input = OscMessage {
        address: "/input/arena/timeline".into(),
        args: vec![OscArg::Str(serde_json::to_string(&version).unwrap())],
    };
    assert!(arena::validate_input(
        &input,
        &kitu_runtime::InputMetadata {
            source: "unity".into(),
            message_id: 3,
            schema_version: 1
        }
    )
    .is_err());
    // Ordinary raw Runtime callers cannot cause TSQ1 decoding from admission/tick.
    assert!(runtime
        .try_enqueue_input(
            OscBundle {
                messages: vec![input]
            },
            Some(kitu_runtime::InputMetadata {
                source: "host:arena-timeline".into(),
                message_id: 3,
                schema_version: 1
            })
        )
        .is_err());
    support::send(&mut runtime, "unity", 2, "/input/arena/menu", vec![]);
    support::send(&mut runtime, "unity", 3, "/input/arena/start", vec![]);
    runtime.tick_once().unwrap();
    assert_eq!(
        arena::inspect_timeline(&runtime).unwrap().active,
        Some(version)
    );
}
#[test]
fn detached_sources_survive_file_edit_deletion_and_replay_seek() {
    let directory = std::env::temp_dir().join(format!("arena-timelines-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    for clip in &edited(4.0).clips {
        std::fs::write(directory.join(format!("{}.tsq", clip.id)), &clip.bytes).unwrap();
    }
    let initial = presentation::load_timeline(&directory).unwrap();
    let mut runtime = build_arena_runtime().unwrap();
    let content = arena::inspect_content(&runtime).unwrap().pending;
    let script = arena::inspect_script(&runtime).unwrap().pending;
    runtime = kitu_demo_game::build_demo_runtime().unwrap();
    arena::install_with_all_versions(&mut runtime, content, script, initial.clone()).unwrap();
    let mut recorder = Recorder::new(&runtime).unwrap();
    support::send(&mut runtime, "unity", 1, "/input/arena/start", vec![]);
    capture(&mut runtime, &mut recorder);
    let second = edited(6.0);
    for clip in &second.clips {
        std::fs::write(directory.join(format!("{}.tsq", clip.id)), &clip.bytes).unwrap();
    }
    arena::stage_timeline(
        &mut runtime,
        presentation::load_timeline(&directory).unwrap(),
        1,
    )
    .unwrap();
    support::send(&mut runtime, "unity", 2, "/input/arena/menu", vec![]);
    support::send(&mut runtime, "unity", 3, "/input/arena/start", vec![]);
    capture(&mut runtime, &mut recorder);
    std::fs::remove_dir_all(&directory).unwrap();
    let bytes = recorder.encode().unwrap();
    let saved = Session::decode(&bytes).unwrap();
    assert_eq!(saved.manifest().version, 3);
    assert_eq!(saved.manifest().initial_timeline, initial);
    assert_eq!(saved.manifest().runs[1]["timeline"]["hash"], second.hash);
    assert_eq!(saved.verify().unwrap().state, support::projection(&runtime));
    let mut seek = saved.runtime().unwrap();
    saved.tick(&mut seek).unwrap();
    saved.tick(&mut seek).unwrap();
    assert_eq!(
        arena::inspect_timeline(&seek).unwrap().presentation,
        arena::inspect_timeline(&runtime).unwrap().presentation
    );
    let mut tampered = Recording::decode(&bytes).unwrap();
    tampered
        .manifest
        .as_object_mut()
        .unwrap()
        .remove("initialTimeline");
    assert!(Session::decode(&tampered.encode().unwrap()).is_err());
    let mut tampered = Recording::decode(&bytes).unwrap();
    tampered.manifest["initialTimeline"]["clips"][0]["bytes"][0] = 0.into();
    assert!(Session::decode(&tampered.encode().unwrap()).is_err());
}
#[test]
fn stock_gameplay_oracle_and_all_timeline_events_replay_exactly_with_pause_resume() {
    let mut saved = None;
    let mut recorded = None;
    let mut boss_offsets = Vec::new();
    let mut floor_after_transition = false;
    let mut cue = None;
    let mut counts = (0, 0);
    support::replay_reference_observing_ticks(
        "stock-eleven-death-retry",
        5528,
        |_| {},
        |runtime, outputs| {
            if saved.is_none() {
                let initial = build_arena_runtime().unwrap();
                saved = Some(Recorder::new(&initial).unwrap());
            }
            saved.as_mut().unwrap().capture(runtime, outputs).unwrap();
            let snap = arena::inspect_timeline(runtime).unwrap();
            let state = support::projection(runtime);
            assert_eq!(snap.presentation.tick, state["tick"].as_i64().unwrap());
            assert_eq!(
                snap.presentation.simulation_step,
                state["simulationSteps"].as_u64().unwrap()
            );
            for b in &snap.presentation.bosses {
                if b.floor == 5 && b.started_tick == 1625 {
                    boss_offsets.push(b.offset_tick);
                }
            }
            floor_after_transition |= snap.presentation.floor.is_some() && state["phase"] == 3;
            if snap.presentation.tick == 1637 {
                recorded = Some((
                    snap.presentation.clone(),
                    saved.as_ref().unwrap().encode().unwrap(),
                ));
            }
            if let Some(b) = snap.presentation.bosses.first() {
                cue = Some(b.id.clone());
            }
            for bundle in outputs {
                let state_index = bundle
                    .messages
                    .iter()
                    .position(|m| m.address == "/ui/arena/state")
                    .unwrap();
                assert_eq!(
                    bundle.messages[state_index + 1].address,
                    "/render/arena/presentation"
                );
                for (index, message) in bundle
                    .messages
                    .iter()
                    .enumerate()
                    .filter(|(_, m)| m.address == "/ui/arena/timeline/event")
                {
                    let OscArg::Str(s) = &message.args[0] else {
                        panic!()
                    };
                    let event: Value = serde_json::from_str(s).unwrap();
                    assert_eq!(event["order"], index);
                    counts.0 += 1;
                    if event["kind"] == "event" {
                        counts.1 += 1;
                    }
                }
            }
        },
    );
    assert_eq!(boss_offsets, (0..48).collect::<Vec<_>>());
    assert!(floor_after_transition);
    assert!(cue.is_some());
    assert!(counts.0 > 30 && counts.1 > 20);
    let saved = Session::decode(&saved.unwrap().encode().unwrap()).unwrap();
    assert_eq!(saved.verify().unwrap().ticks, 5528);
    let (expected, bytes) = recorded.unwrap();
    let session = Session::decode(&bytes).unwrap();
    let mut runtime = session.runtime().unwrap();
    while runtime.current_tick().get() < session.manifest().ticks {
        session.tick(&mut runtime).unwrap();
    }
    assert_eq!(
        arena::inspect_timeline(&runtime).unwrap().presentation,
        expected
    );
    support::send(
        &mut runtime,
        "pause-test",
        1,
        "/input/arena/disconnect",
        vec![],
    );
    runtime.tick_once().unwrap();
    let paused = arena::inspect_timeline(&runtime).unwrap().presentation;
    for _ in 0..5 {
        runtime.tick_once().unwrap();
        assert_eq!(
            arena::inspect_timeline(&runtime)
                .unwrap()
                .presentation
                .bosses,
            paused.bosses
        );
    }
    support::send(&mut runtime, "pause-test", 2, "/input/arena/resume", vec![]);
    runtime.tick_once().unwrap();
    assert_eq!(
        arena::inspect_timeline(&runtime)
            .unwrap()
            .presentation
            .bosses[0]
            .offset_tick,
        paused.bosses[0].offset_tick + 1
    );
}
#[test]
fn timeline_recording_limit_is_symmetric_and_preserves_valid_prefix() {
    let mut runtime = build_arena_runtime().unwrap();
    let mut recorder = Recorder::new(&runtime).unwrap();
    let versions = (0..64)
        .map(|index| edited(0.3 + index as f32 / 100.0))
        .collect::<Vec<_>>();
    for (index, version) in versions[..63].iter().enumerate() {
        arena::stage_timeline(&mut runtime, version.clone(), index as u64 + 1).unwrap();
    }
    capture(&mut runtime, &mut recorder);
    let prefix = recorder.encode().unwrap();
    let saved = Session::decode(&prefix).unwrap();
    assert_eq!(saved.verify().unwrap().ticks, 1);
    arena::stage_timeline(&mut runtime, versions[63].clone(), 64).unwrap();
    runtime.tick_once().unwrap();
    let output = runtime.drain_output_buffer();
    assert!(recorder
        .capture(&runtime, &output)
        .unwrap_err()
        .to_string()
        .contains("64 timeline versions"));
    assert_eq!(recorder.encode().unwrap(), prefix);
    let mut invalid = Recording::decode(&prefix).unwrap();
    invalid.manifest["ticks"] = 2.into();
    invalid.manifest["proofs"]
        .as_array_mut()
        .unwrap()
        .push(json!({"state":"0".repeat(64),"output":"0".repeat(64)}));
    invalid.entries.push(TimedBundle {tick:1,order:0,metadata:json!({"sequence":63,"identity":{"source":"host:arena-timeline","messageId":64,"schemaVersion":1}}),bundle:OscBundle{messages:vec![OscMessage{address:"/input/arena/timeline".into(),args:vec![OscArg::Str(serde_json::to_string(&versions[63]).unwrap())]}]}});
    assert!(Session::decode(&invalid.encode().unwrap())
        .err()
        .unwrap()
        .to_string()
        .contains("64 timeline versions"));
}
