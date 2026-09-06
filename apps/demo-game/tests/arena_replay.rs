#[allow(dead_code)] // Shared oracle helpers are used by different integration binaries.
mod support;
use kitu_demo_game::{
    arena::{
        self,
        config::{ArenaConfig, ContentVersion},
    },
    build_arena_runtime,
    replay::{Recorder, Session},
};
use kitu_osc_ir::OscArg;
use kitu_tsq1::recording::Recording;
use serde_json::{json, Value};

#[test]
fn real_tsq1_replays_every_stock_tick_state_and_event_through_eleven_death_retry() {
    let initial = build_arena_runtime().unwrap();
    let mut recorder = Recorder::new(&initial).unwrap();
    let (checkpoints, outcomes) = support::replay_reference_observing_ticks(
        "stock-eleven-death-retry",
        5528,
        |_| {},
        |runtime, outputs| recorder.capture(runtime, outputs).unwrap(),
    );
    assert_eq!((checkpoints, outcomes), (550, 53));
    let bytes = recorder.encode().unwrap();
    assert_eq!(&bytes[..4], b"TSQ1");
    let session = Session::decode(&bytes).unwrap();
    let verified = session.verify().unwrap();
    assert_eq!((verified.ticks, verified.runs), (5528, 2));
    assert_eq!(verified.state["phase"], 1);
    assert_eq!(verified.state["inventory"]["maxHealth"], 100);
    assert_eq!(session.manifest().proofs.len(), 5528);
    if let Some(dir) = std::env::var_os("KITU_REPLAY_EVIDENCE_DIR") {
        std::fs::create_dir_all(&dir).unwrap();
        let dir = std::path::Path::new(&dir);
        std::fs::write(dir.join("stock-eleven-death-retry.tsq"), bytes).unwrap();
        std::fs::write(
            dir.join("stock-verification.json"),
            serde_json::to_vec_pretty(&verified).unwrap(),
        )
        .unwrap();
    }
}

#[test]
fn saved_content_management_ticks_and_duplicates_survive_later_tanu_edits() {
    let mut runtime = build_arena_runtime().unwrap();
    let mut recorder = Recorder::new(&runtime).unwrap();
    let mut edited = ArenaConfig::default();
    edited.items[0].damage = 37;
    let saved = ContentVersion::from_tmd(&edited.to_tmd().unwrap()).unwrap();
    arena::stage_content(&mut runtime, saved.clone(), 1).unwrap();
    support::send(&mut runtime, "unity", 1, "/input/arena/start", vec![]);
    support::send(&mut runtime, "unity", 1, "/input/arena/start", vec![]);
    for tick in 0..15 {
        match tick {
            2 => support::send(&mut runtime, "unity", 2, "/input/arena/pause", vec![]),
            5 => support::send(&mut runtime, "unity", 3, "/input/arena/resume", vec![]),
            7 => {
                edited.items[0].damage = 99;
                arena::stage_content(
                    &mut runtime,
                    ContentVersion::from_tmd(&edited.to_tmd().unwrap()).unwrap(),
                    2,
                )
                .unwrap();
            }
            8 => {
                support::send(&mut runtime, "unity", 4, "/input/arena/menu", vec![]);
                support::send(&mut runtime, "unity", 5, "/input/arena/start", vec![]);
            }
            10 => support::send(
                &mut runtime,
                "unity",
                6,
                "/input/arena/use",
                vec![OscArg::Int(2)],
            ),
            _ => {}
        }
        runtime.tick_once().unwrap();
        let outputs = runtime.drain_output_buffer();
        recorder.capture(&runtime, &outputs).unwrap();
    }
    let bytes = recorder.encode().unwrap();
    let session = Session::decode(&bytes).unwrap();
    assert_eq!(session.manifest().runs[0]["content"]["hash"], saved.hash);
    assert_eq!(
        session.manifest().runs[0]["content"]["values"]["items"][0]["damage"],
        37
    );
    assert_eq!(
        session.manifest().runs[1]["content"]["values"]["items"][0]["damage"],
        99
    );
    let expected = support::projection(&runtime);
    assert_eq!(session.verify().unwrap().state, expected);
    // Reconstruct from the old detached initial values even if current authoring differs.
    assert_eq!(
        arena::inspect_content(&session.runtime().unwrap())
            .unwrap()
            .pending,
        session.manifest().initial_content
    );
    let entries = Recording::decode(&bytes).unwrap().entries;
    assert_eq!(entries[1].tick, entries[2].tick);
    assert_eq!(
        entries[1].metadata["identity"]["messageId"],
        entries[2].metadata["identity"]["messageId"]
    );
    assert_ne!(entries[1].order, entries[2].order);
}

#[test]
fn incompatible_tampered_and_truncated_sessions_are_rejected_with_tick_diagnostics() {
    let mut runtime = build_arena_runtime().unwrap();
    let mut recorder = Recorder::new(&runtime).unwrap();
    support::send(&mut runtime, "unity", 1, "/input/arena/start", vec![]);
    runtime.tick_once().unwrap();
    let outputs = runtime.drain_output_buffer();
    recorder.capture(&runtime, &outputs).unwrap();
    let bytes = recorder.encode().unwrap();
    let original = Recording::decode(&bytes).unwrap();
    for key in ["version", "contractVersion", "tickRate", "ticks"] {
        let mut doc = original.clone();
        doc.manifest[key] = json!(999999999);
        assert!(Session::decode(&doc.encode().unwrap()).is_err(), "{key}");
    }
    let mut doc = original.clone();
    doc.manifest["execution"]["sourceHash"] = json!("old");
    assert!(Session::decode(&doc.encode().unwrap()).is_err());
    let mut doc = original.clone();
    doc.entries[0].metadata["sequence"] = json!(1);
    assert!(Session::decode(&doc.encode().unwrap()).is_err());
    let mut doc = original.clone();
    doc.manifest["proofs"][0]["output"] = json!("0".repeat(64));
    let session = Session::decode(&doc.encode().unwrap()).unwrap();
    assert!(session
        .verify()
        .unwrap_err()
        .to_string()
        .contains("event order diverged at tick 0"));
    let mut doc = original;
    doc.manifest["runs"] = Value::Array(vec![]);
    assert!(Session::decode(&doc.encode().unwrap())
        .unwrap()
        .verify()
        .is_err());
    assert!(Session::decode(&bytes[..bytes.len() - 1]).is_err());
    assert!(Recorder::new(&runtime).is_err());
    runtime.tick_once().unwrap();
    runtime.tick_once().unwrap();
    assert!(recorder.capture(&runtime, &[]).is_err());
}
