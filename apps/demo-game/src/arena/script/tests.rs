use super::*;
use crate::{
    arena::{self, ArenaSession, ArenaState, Controls, Vec2},
    build_arena_runtime,
};
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::InputMetadata;

fn send(runtime: &mut crate::DemoRuntime, id: u64, address: &str) {
    runtime
        .try_enqueue_input(
            OscBundle {
                messages: vec![OscMessage::new(address)],
            },
            Some(InputMetadata {
                source: "test:script".into(),
                message_id: id,
                schema_version: arena::SCHEMA_VERSION,
            }),
        )
        .unwrap();
}
fn tick(runtime: &mut crate::DemoRuntime) -> Vec<OscBundle> {
    runtime.tick_once().unwrap();
    runtime.drain_output_buffer()
}
fn boss(runtime: &mut crate::DemoRuntime, floor: i32) {
    let session = runtime.world_mut().resource_mut::<ArenaSession>().unwrap();
    session.state.phase = 3;
    session.state.floor = floor;
    session.state.enemies = vec![Enemy {
        id: 99,
        kind: 3,
        health: 100,
        max_health: 100,
        radius: 0.8,
        phase_remaining: 1.0 / 60.0,
        position: Vec2 { x: 0.0, y: 5.0 },
        ..Enemy::default()
    }];
}
fn state(runtime: &crate::DemoRuntime) -> &ArenaState {
    &runtime.world().resource::<ArenaSession>().unwrap().state
}
fn receipts(output: &[OscBundle]) -> Vec<serde_json::Value> {
    output
        .iter()
        .flat_map(|bundle| &bundle.messages)
        .filter(|message| message.address == "/ui/arena/command")
        .map(|message| {
            let OscArg::Str(json) = &message.args[0] else {
                panic!()
            };
            serde_json::from_str(json).unwrap()
        })
        .collect()
}

#[test]
fn script_identity_bounds_and_action_contract_reject_invalid_candidates() {
    let version = default_script().unwrap();
    assert_eq!(version.source_sha256, sha(DEFAULT_SOURCE.as_bytes()));
    assert_eq!(version, ScriptVersion::from_source(DEFAULT_SOURCE).unwrap());
    for source in [
        "fn boss( {",
        "fn boss(input) { #{action: \"telegraph\", duration: 0.8} }",
        "fn boss(input) { #{action: \"wait\", duration: -1.0} }",
        "fn boss(input) { while true {} }",
    ] {
        assert!(ScriptVersion::from_source(source).is_err(), "{source}");
    }
    assert!(ScriptVersion::from_source(&" ".repeat(MAX_SOURCE_BYTES + 1)).is_err());
    let mut invalid = version.clone();
    invalid.contract_version += 1;
    assert_eq!(validate_version(&invalid).unwrap_err().kind, "version");
    invalid = version.clone();
    invalid.hash = "0".repeat(64);
    assert_eq!(validate_version(&invalid).unwrap_err().kind, "version");
    let mut json = serde_json::to_value(&version).unwrap();
    json["unexpected"] = true.into();
    assert!(serde_json::from_value::<ScriptVersion>(json).is_err());
}

#[test]
fn duration_edit_adopts_only_on_next_start_and_deduplicates_management() {
    let mut runtime = build_arena_runtime().unwrap();
    send(&mut runtime, 1, "/input/arena/start");
    tick(&mut runtime);
    let original = arena::inspect_script(&runtime).unwrap().active.unwrap();
    let edited =
        ScriptVersion::from_source(&DEFAULT_SOURCE.replace("duration: 0.8", "duration: 1.6"))
            .unwrap();
    arena::stage_script(&mut runtime, edited.clone(), 2).unwrap();
    arena::stage_script(&mut runtime, edited.clone(), 2).unwrap();
    arena::stage_script(&mut runtime, original.clone(), 1).unwrap();
    arena::stage_script(&mut runtime, original.clone(), 2).unwrap();
    boss(&mut runtime, 5);
    let output = tick(&mut runtime);
    let receipt = receipts(&output);
    assert_eq!(
        (
            receipt[0]["accepted"].as_bool(),
            receipt[1]["duplicate"].as_bool()
        ),
        (Some(true), Some(true))
    );
    assert_eq!(receipt[2]["code"], "id_conflict");
    assert_eq!(receipt[3]["code"], "id_conflict");
    assert_eq!(state(&runtime).enemies[0].phase_remaining, 0.8_f32);
    let snapshot = arena::inspect_script(&runtime).unwrap();
    assert_eq!(snapshot.active.unwrap(), original);
    assert_eq!(snapshot.pending, edited);
    send(&mut runtime, 2, "/input/arena/menu");
    send(&mut runtime, 3, "/input/arena/start");
    tick(&mut runtime);
    boss(&mut runtime, 5);
    tick(&mut runtime);
    assert_eq!(state(&runtime).enemies[0].phase_remaining, 1.6_f32);
    assert_eq!(
        arena::inspect_script(&runtime).unwrap().active.unwrap(),
        edited
    );
}

#[test]
fn malformed_or_unreserved_input_cannot_change_candidate_or_queue_sequence() {
    let mut runtime = build_arena_runtime().unwrap();
    let original = arena::inspect_script(&runtime).unwrap().pending;
    let mut message = OscMessage::new("/input/arena/script");
    message.args = vec![OscArg::Str(serde_json::to_string(&original).unwrap())];
    let metadata = InputMetadata {
        source: "unity".into(),
        message_id: 1,
        schema_version: 1,
    };
    assert!(runtime
        .try_enqueue_input(
            OscBundle {
                messages: vec![message.clone()]
            },
            Some(metadata.clone())
        )
        .is_err());
    message.args = vec![OscArg::Str("{}".into())];
    assert!(runtime
        .try_enqueue_input(
            OscBundle {
                messages: vec![message]
            },
            Some(InputMetadata {
                source: "host:arena-script".into(),
                ..metadata
            })
        )
        .is_err());
    assert!(arena::stage_script(&mut runtime, original.clone(), 0).is_err());
    assert_eq!(
        arena::stage_script(&mut runtime, original.clone(), 1).unwrap(),
        0
    );
    tick(&mut runtime);
    assert_eq!(arena::inspect_script(&runtime).unwrap().pending, original);
}

#[test]
fn late_fault_consumes_one_management_tick_without_partial_gameplay_or_retry() {
    let source = DEFAULT_SOURCE.replace(
        "fn boss(input) {",
        "fn boss(input) { if input.floor == 10 && input.hp == 50 { while true {} }",
    );
    let version = ScriptVersion::from_source(&source).unwrap();
    let mut runtime = build_arena_runtime().unwrap();
    arena::stage_script(&mut runtime, version, 1).unwrap();
    send(&mut runtime, 1, "/input/arena/start");
    tick(&mut runtime);
    boss(&mut runtime, 10);
    let session = runtime.world_mut().resource_mut::<ArenaSession>().unwrap();
    let mut second = session.state.enemies[0];
    second.id = 100;
    second.health = 50;
    session.state.enemies.push(second);
    session.controls = Controls {
        movement: Vec2 { x: 1.0, y: 0.0 },
        fire_a: true,
        ..Controls::default()
    };
    session.queued_use = [true; 2];
    let before = session.state.clone();
    let output = tick(&mut runtime);
    let after = state(&runtime);
    assert_eq!(after.overlay, "pause");
    assert_eq!(after.elapsed, before.elapsed);
    assert_eq!(after.player_position, before.player_position);
    assert_eq!(after.enemies, before.enemies);
    assert_eq!(after.simulation_steps, before.simulation_steps);
    assert_eq!(after.inventory, before.inventory);
    assert_eq!(after.next_entity_id, before.next_entity_id);
    assert_eq!(runtime.current_tick().get(), 2);
    assert_eq!(
        output
            .iter()
            .flat_map(|bundle| &bundle.messages)
            .filter(|message| message.address.starts_with("/game/"))
            .count(),
        1
    );
    let fault = arena::inspect_script(&runtime).unwrap().fault.unwrap();
    assert_eq!((fault.tick, fault.enemy_id), (1, 100));
    assert!(!fault.diagnostic.kind.is_empty());
    send(&mut runtime, 2, "/input/arena/resume");
    let output = tick(&mut runtime);
    assert_eq!(receipts(&output)[0]["accepted"], false);
    assert!(!output
        .iter()
        .flat_map(|bundle| &bundle.messages)
        .any(|message| message.address == "/game/arena/script-fault"));
    assert_eq!(state(&runtime).elapsed, before.elapsed);
    arena::stage_script(&mut runtime, default_script().unwrap(), 2).unwrap();
    tick(&mut runtime);
    assert!(arena::inspect_script(&runtime).unwrap().fault.is_some());
    send(&mut runtime, 3, "/input/arena/menu");
    send(&mut runtime, 4, "/input/arena/start");
    tick(&mut runtime);
    assert!(arena::inspect_script(&runtime).unwrap().fault.is_none());
}

#[test]
fn pending_program_pool_is_bounded_and_pins_versions_past_authoring_cache_eviction() {
    let mut runtime = build_arena_runtime().unwrap();
    let versions = (0..65)
        .map(|index| {
            ScriptVersion::from_source(&format!("// queued unique {index}\n{DEFAULT_SOURCE}"))
                .unwrap()
        })
        .collect::<Vec<_>>();
    for (index, version) in versions[..64].iter().enumerate() {
        arena::stage_script(&mut runtime, version.clone(), index as u64 + 1).unwrap();
    }
    assert!(arena::stage_script(&mut runtime, versions[64].clone(), 65).is_err());
    // A retransmission stays admissible even when the distinct-program pool is full.
    arena::stage_script(&mut runtime, versions[0].clone(), 1).unwrap();
    tick(&mut runtime);
    assert_eq!(
        arena::inspect_script(&runtime).unwrap().pending,
        versions[63]
    );
    arena::stage_script(&mut runtime, versions[64].clone(), 65).unwrap();
    tick(&mut runtime);
    assert_eq!(
        arena::inspect_script(&runtime).unwrap().pending,
        versions[64]
    );
}

#[test]
fn authoring_reads_reject_nonfiles_invalid_encoding_and_oversized_source() {
    let dir = std::env::temp_dir().join(format!("arena-script-io-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    assert_eq!(load_script(&dir).unwrap_err().kind, "source_io");
    let path = dir.join("boss.rhai");
    std::fs::write(&path, [0xff, 0xfe]).unwrap();
    assert_eq!(load_script(&path).unwrap_err().kind, "source_encoding");
    std::fs::write(&path, vec![b' '; MAX_SOURCE_BYTES + 1]).unwrap();
    assert_eq!(load_script(&path).unwrap_err().kind, "source_limit");
    std::fs::remove_file(&path).unwrap();
    #[cfg(unix)]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let c_path = CString::new(path.as_os_str().as_bytes()).unwrap();
        // SAFETY: the temporary path is NUL terminated and valid for this call.
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        assert_eq!(load_script(&path).unwrap_err().kind, "source_io");
        std::fs::remove_file(&path).unwrap();
    }
    std::fs::remove_dir(&dir).unwrap();
    let diagnostic = diagnostic("source_io", "あ".repeat(1000));
    assert!(diagnostic.message.len() <= 1024);
}
