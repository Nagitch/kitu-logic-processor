use super::*;
use crate::arena::Enemy;
use kitu_osc_ir::OscMessage;
use kitu_tsq1::presentation::ScheduledEvent;

fn prepared() -> Arc<PreparedTimeline> {
    default_prepared().unwrap()
}
fn state(phase: i32) -> ArenaState {
    ArenaState {
        phase,
        floor: 5,
        ..ArenaState::default()
    }
}
fn boss(id: i32, phase: i32) -> Enemy {
    Enemy {
        id,
        kind: 3,
        health: 100,
        boss_state: phase,
        ..Enemy::default()
    }
}
fn start_boss(p: &mut Presentation, id: i32, events: &mut Vec<Value>) {
    let mut state = state(3);
    state.enemies.push(boss(id, 1));
    p.synchronize(1, &state, 100);
    p.step(&prepared(), 3, &[(id, 0)], &state, events);
}
#[test]
fn generated_sources_are_exact_bundled_tsqs_and_identity_is_strict() {
    let [boss, floor] = reference_sources().unwrap();
    assert_eq!(boss, DEFAULT_BOSS);
    assert_eq!(floor, DEFAULT_FLOOR);
    let version = default_timeline().unwrap();
    assert!(version
        .clips
        .iter()
        .all(|clip| clip.bytes.starts_with(b"TSQ1")));
    let mut invalid = version.clone();
    invalid.clips.swap(0, 1);
    assert!(validate_version(&invalid).is_err());
    let mut invalid = version.clone();
    invalid.clips[0].bytes[0] ^= 1;
    assert!(validate_version(&invalid).is_err());
    let mut invalid = serde_json::to_value(version).unwrap();
    invalid["unknown"] = true.into();
    assert!(serde_json::from_value::<TimelineVersion>(invalid).is_err());
}
#[test]
fn whitelist_initialization_and_terminal_fade_are_validated() {
    let mut clip = Clip::decode(DEFAULT_BOSS).unwrap();
    clip.events[0].bundle.messages[0].address = "/game/arena/damage".into();
    assert!(TimelineVersion::from_sources(&clip.encode().unwrap(), DEFAULT_FLOOR).is_err());
    let mut clip = Clip::decode(DEFAULT_BOSS).unwrap();
    clip.events[0].bundle.messages[0].args[0] = OscArg::Int(3);
    assert!(TimelineVersion::from_sources(&clip.encode().unwrap(), DEFAULT_FLOOR).is_err());
    let mut clip = Clip::decode(DEFAULT_BOSS).unwrap();
    clip.events[0].offset_tick = 1;
    assert!(TimelineVersion::from_sources(&clip.encode().unwrap(), DEFAULT_FLOOR).is_err());
    let mut clip = Clip::decode(DEFAULT_BOSS).unwrap();
    clip.events[0].bundle.messages[0].args[0] = OscArg::Float(9.0);
    assert!(TimelineVersion::from_sources(&clip.encode().unwrap(), DEFAULT_FLOOR).is_err());
    let mut floor = Clip::decode(DEFAULT_FLOOR).unwrap();
    floor.events.last_mut().unwrap().bundle.messages[0].args[0] = OscArg::Float(0.5);
    assert!(TimelineVersion::from_sources(DEFAULT_BOSS, &floor.encode().unwrap()).is_err());
}
#[test]
fn same_offset_tracks_bundles_and_messages_apply_in_order_once() {
    let message = |radius| OscMessage {
        address: "/render/arena/cue/boss".into(),
        args: vec![OscArg::Float(radius), OscArg::Float(0.5)],
    };
    let clip = Clip {
        tick_rate: 60,
        track_count: 2,
        events: vec![
            ScheduledEvent {
                offset_tick: 0,
                track_index: 0,
                event_index: 0,
                bundle: OscBundle {
                    messages: vec![message(1.0), message(2.0)],
                },
            },
            ScheduledEvent {
                offset_tick: 0,
                track_index: 0,
                event_index: 1,
                bundle: OscBundle { messages: vec![] },
            },
            ScheduledEvent {
                offset_tick: 0,
                track_index: 1,
                event_index: 0,
                bundle: OscBundle {
                    messages: vec![message(4.0)],
                },
            },
        ],
    };
    let version = TimelineVersion::from_sources(&clip.encode().unwrap(), DEFAULT_FLOOR).unwrap();
    let prepared = prepare_version(&version).unwrap();
    let mut presentation = Presentation::default();
    let mut state = state(3);
    state.enemies.push(boss(5, 1));
    let mut events = Vec::new();
    presentation.step(&prepared, 3, &[(5, 0)], &state, &mut events);
    assert_eq!(presentation.snapshot.bosses[0].radius, 4.0);
    assert_eq!(presentation.snapshot.bosses[0].next_event_index, 3);
    let ordered = events
        .iter()
        .filter(|event| event["kind"] == "event")
        .map(|event| (event["trackIndex"].clone(), event["eventIndex"].clone()))
        .collect::<Vec<_>>();
    assert_eq!(
        ordered,
        vec![
            (0.into(), 0.into()),
            (0.into(), 1.into()),
            (1.into(), 0.into())
        ]
    );
    events.clear();
    presentation.step(&prepared, 3, &[(5, 1)], &state, &mut events);
    assert!(events.is_empty());
    assert_eq!(presentation.snapshot.bosses[0].radius, 4.0);
}
#[test]
fn stop_precedes_due_event_and_long_boss_phase_holds_final_value() {
    let mut p = Presentation::default();
    let mut events = Vec::new();
    start_boss(&mut p, 7, &mut events);
    let mut state = state(3);
    state.enemies.push(boss(7, 1));
    for _ in 0..95 {
        p.step(&prepared(), 3, &[(7, 1)], &state, &mut events);
    }
    let cue = &p.snapshot.bosses[0];
    assert_eq!(cue.offset_tick, 95);
    assert_eq!(cue.next_event_index, 5);
    assert_eq!(cue.intensity, 1.0);
    assert_eq!(events.iter().filter(|e| e["kind"] == "event").count(), 5);
    // Stop on the actual burst tick, even when another authored event would be due.
    p.snapshot.bosses[0].offset_tick = 11;
    p.snapshot.bosses[0].next_event_index = 1;
    state.enemies[0].boss_state = 2;
    events.clear();
    p.step(&prepared(), 3, &[(7, 1)], &state, &mut events);
    assert!(p.snapshot.bosses.is_empty());
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["kind"], "stop");
}
#[test]
fn transition_steps_run_fade_into_combat_then_remove_on_terminal_event() {
    let mut p = Presentation::default();
    let mut events = Vec::new();
    let mut state = state(2);
    p.synchronize(1, &state, 50);
    p.step(&prepared(), 4, &[], &state, &mut events);
    assert_eq!(p.snapshot.floor.as_ref().unwrap().offset_tick, 0);
    for offset in 1..60 {
        state.phase = if offset < 18 { 2 } else { 3 };
        p.step(
            &prepared(),
            if offset <= 18 { 2 } else { 3 },
            &[],
            &state,
            &mut events,
        );
        if offset < 59 {
            assert_eq!(p.snapshot.floor.as_ref().unwrap().offset_tick, offset);
        }
    }
    assert!(p.snapshot.floor.is_none());
    assert_eq!(events.last().unwrap()["reason"], "completed");
    assert_eq!(events.iter().filter(|e| e["kind"] == "event").count(), 5);
}
#[test]
fn results_suppress_triggers_and_lifecycle_stops_preserve_old_run_identity() {
    let mut p = Presentation::default();
    let mut events = Vec::new();
    start_boss(&mut p, 7, &mut events);
    let state = state(5);
    events.clear();
    p.step(&prepared(), 3, &[(7, 0)], &state, &mut events);
    assert!(p.snapshot.bosses.is_empty());
    assert_eq!(events.len(), 1);
    start_boss(&mut p, 9, &mut events);
    events.clear();
    p.clear("new-run", true, &mut events);
    assert_eq!(events[0]["run"], 1);
    p.synchronize(2, &state, 101);
    assert_eq!(p.id(), "r2:c1");
}
