use super::*;
use crate::replay::Session;
use serde_json::json;

fn host() -> ArenaHost {
    ArenaHost::new(build_arena_runtime().unwrap(), HostOptions::default()).unwrap()
}
fn input(host: &mut ArenaHost, id: u64, name: &str, args: Vec<OscArg>) {
    host.submit(
        OscBundle {
            messages: vec![OscMessage {
                address: format!("/input/arena/{name}"),
                args,
            }],
        },
        Some(InputMetadata {
            source: "inspection-test".into(),
            message_id: id,
            schema_version: 1,
        }),
    )
    .unwrap();
}
fn view(host: &ArenaHost) -> Value {
    serde_json::to_value(host.inspection().unwrap()).unwrap()
}
fn action(host: &mut ArenaHost, name: &str) {
    let (complete, _) = tokio::sync::oneshot::channel();
    host.state
        .inner
        .lock()
        .unwrap()
        .controls
        .push_back(playback::Control::Action(name.into(), complete));
    host.tick().unwrap();
}
fn record(start: bool, corrupt: bool) -> Arc<Session> {
    let mut host = host();
    if start {
        input(&mut host, 1, "start", vec![]);
    }
    for _ in 0..4 {
        host.tick().unwrap();
    }
    let bytes = host.state.inner.lock().unwrap().recorder.encode().unwrap();
    if !corrupt {
        return Arc::new(Session::decode(&bytes).unwrap());
    }
    let mut record = kitu_tsq1::recording::Recording::decode(&bytes).unwrap();
    record.manifest["proofs"][0]["state"] = Value::String("0".repeat(64));
    Arc::new(Session::decode(&record.encode().unwrap()).unwrap())
}
fn prepared(session: Arc<Session>, tick: i64) -> playback::Playback {
    playback::Playback::at("b".repeat(64), session, tick).unwrap()
}
fn load(host: &mut ArenaHost, session: Arc<Session>, tick: i64) {
    host.state.inner.lock().unwrap().pending_playback = Some(prepared(session, tick));
    host.tick().unwrap();
}
fn seek(host: &mut ArenaHost, generation: u64, value: Result<playback::Playback>) {
    let (complete, _) = tokio::sync::oneshot::channel();
    host.state
        .inner
        .lock()
        .unwrap()
        .controls
        .push_back(playback::Control::Prepared(
            generation,
            value.map(Box::new),
            complete,
        ));
    host.tick().unwrap();
}
fn replace_json(bundles: &mut [OscBundle], address: &str, edit: impl FnOnce(&mut Value)) {
    let message = bundles
        .iter_mut()
        .flat_map(|b| &mut b.messages)
        .find(|m| m.address == address)
        .unwrap();
    let OscArg::Str(raw) = &mut message.args[0] else {
        panic!()
    };
    let mut value: Value = serde_json::from_str(raw).unwrap();
    edit(&mut value);
    *raw = serde_json::to_string(&value).unwrap();
}

#[tokio::test]
async fn initial_get_is_coherent_bounded_and_has_no_tick_or_controller_effects() {
    let host = host();
    let before = host.inspect().unwrap();
    for _ in 0..4 {
        let response = get(State(host.state.clone())).await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "no-store");
        let bytes = axum::body::to_bytes(response.into_body(), RESPONSE_BYTES)
            .await
            .unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(value["state"]["tick"], "-1");
        assert_eq!(value["state"]["simulationSteps"], "0");
        assert_eq!(value["presentation"]["run"], value["run"]);
        assert_eq!(value["attempt"], "0");
        assert_eq!(value["timing"]["last"], Value::Null);
        assert_eq!(value["events"]["entries"], json!([]));
        assert_eq!(
            value["arena"]["portal"],
            json!({"x":0.0,"y":7.5,"triggerRadius":1.25})
        );
    }
    assert_eq!(host.inspect().unwrap(), before);
    let game = host.state.inner.lock().unwrap();
    assert_eq!(game.runtime.current_tick().get(), 0);
    assert!(game.controller.is_none());
    assert!(game.controls.is_empty());
}

#[test]
fn live_pause_and_repeated_reads_preserve_exact_outputs_and_recording_bytes() {
    let (mut observed, mut baseline) = (host(), host());
    for tick in 0..30 {
        let command = match tick {
            0 => Some("start"),
            5 => Some("pause"),
            11 => Some("resume"),
            15 => Some("inventory"),
            19 => Some("close"),
            _ => None,
        };
        if let Some(command) = command {
            input(&mut observed, tick + 1, command, vec![]);
            input(&mut baseline, tick + 1, command, vec![]);
        }
        for _ in 0..5 {
            let _ = view(&observed);
        }
        assert_eq!(observed.tick().unwrap(), baseline.tick().unwrap());
        let snapshot = view(&observed);
        assert_eq!(snapshot["state"]["tick"], tick.to_string());
        assert_eq!(snapshot["timing"]["last"]["runtimeAdvanced"], true);
        if (5..11).contains(&tick) || (15..19).contains(&tick) {
            assert_eq!(snapshot["timing"]["last"]["simulationAdvanced"], false);
        }
        assert_eq!(
            snapshot["state"]["simulationSteps"],
            snapshot["presentation"]["simulationStep"]
        );
    }
    assert_eq!(
        observed
            .state
            .inner
            .lock()
            .unwrap()
            .recorder
            .encode()
            .unwrap(),
        baseline
            .state
            .inner
            .lock()
            .unwrap()
            .recorder
            .encode()
            .unwrap()
    );
}

#[test]
fn same_tick_runs_and_late_old_run_timeline_audits_keep_source_attribution() {
    let mut host = host();
    for (id, name) in [(1, "start"), (2, "menu"), (3, "start")] {
        input(&mut host, id, name, vec![]);
    }
    host.tick().unwrap();
    let snapshot = view(&host);
    assert_eq!(snapshot["run"], "2");
    let run_events: Vec<_> = snapshot["events"]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["detail"]["kind"] == "run")
        .collect();
    assert_eq!(
        run_events
            .iter()
            .map(|v| v["run"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["1", "2"]
    );
    assert!(!serde_json::to_string(&run_events)
        .unwrap()
        .contains("sourceSha256"));
    input(
        &mut host,
        4,
        "frame",
        vec![
            OscArg::Float(0.0),
            OscArg::Float(1.0),
            OscArg::Bool(false),
            OscArg::Float(0.0),
            OscArg::Float(0.0),
            OscArg::Bool(false),
            OscArg::Bool(false),
        ],
    );
    for _ in 0..200 {
        host.tick().unwrap();
        if !view(&host)["presentation"]["floor"].is_null() {
            break;
        }
    }
    assert!(
        !view(&host)["presentation"]["floor"].is_null(),
        "normal movement triggers an actual floor cue"
    );
    input(&mut host, 5, "menu", vec![]);
    input(&mut host, 6, "start", vec![]);
    let output = host.tick().unwrap();
    let snapshot = view(&host);
    assert_eq!(snapshot["run"], "3");
    let mut old_audit = false;
    for (bi, bundle) in output.iter().enumerate() {
        for (mi, message) in bundle.messages.iter().enumerate() {
            if message.address != "/ui/arena/timeline/event" {
                continue;
            }
            let OscArg::Str(raw) = &message.args[0] else {
                panic!()
            };
            let value: Value = serde_json::from_str(raw).unwrap();
            if value["run"] != 2 {
                continue;
            }
            let event = snapshot["events"]["entries"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| {
                    v["tick"] == snapshot["state"]["tick"]
                        && v["bundleIndex"] == bi
                        && v["messageIndex"] == mi
                })
                .unwrap();
            assert_eq!(event["run"], "2");
            old_audit = true;
        }
    }
    assert!(old_audit, "actual late floor-cue stop retains its old run");
}

#[test]
fn load_step_idle_seek_stop_and_live_have_explicit_epochs_and_real_flags() {
    let mut host = host();
    input(&mut host, 1, "start", vec![]);
    for _ in 0..8 {
        host.tick().unwrap();
    }
    input(&mut host, 2, "pause", vec![]);
    host.tick().unwrap();
    let live_steps = view(&host)["state"]["simulationSteps"].clone();
    let session = record(true, false);
    load(&mut host, session.clone(), -1);
    let initial = view(&host);
    assert_eq!(initial["epoch"], "1");
    assert_eq!(initial["state"]["tick"], "-1");
    assert_eq!(initial["timing"]["last"]["outcome"], "replacement");
    assert_eq!(initial["timing"]["last"]["runtimeAdvanced"], false);
    action(&mut host, "step");
    let stepped = view(&host);
    assert_eq!(stepped["epoch"], "1");
    assert_eq!(stepped["state"]["tick"], "0");
    let events = stepped["events"].clone();
    host.tick().unwrap();
    let idle = view(&host);
    assert_eq!(idle["events"], events);
    assert_eq!(idle["timing"]["last"]["outcome"], "idle");
    for (tick, epoch) in [(2, "2"), (0, "3"), (0, "4"), (-1, "5")] {
        seek(&mut host, 0, Ok(prepared(session.clone(), tick)));
        let snapshot = view(&host);
        assert_eq!(snapshot["epoch"], epoch);
        assert_eq!(snapshot["state"]["tick"], tick.to_string());
        assert_eq!(snapshot["events"]["entries"], json!([]));
        assert_eq!(snapshot["timing"]["totalSamples"], "1");
    }
    action(&mut host, "live");
    let live = view(&host);
    assert_eq!(live["epoch"], "6");
    assert_eq!(live["timing"]["last"]["outcome"], "replacement");
    assert_eq!(live["timing"]["last"]["runtimeAdvanced"], true);
    assert_eq!(live["timing"]["last"]["simulationAdvanced"], false);
    assert_eq!(live["state"]["simulationSteps"], live_steps);
    assert_eq!(live["mode"]["active"], false);
    action(&mut host, "live");
    assert_eq!(
        view(&host)["epoch"],
        "6",
        "no-op live does not replace the observed Runtime"
    );
}

#[test]
fn pending_cancel_failed_and_stale_seek_do_not_invent_replacement_epochs() {
    let mut host = host();
    let session = record(false, false);
    host.state.inner.lock().unwrap().pending_playback = Some(prepared(session.clone(), -1));
    assert_eq!(view(&host)["readOnly"], true);
    action(&mut host, "live");
    assert_eq!(view(&host)["epoch"], "0");
    assert_eq!(view(&host)["readOnly"], false);
    load(&mut host, session.clone(), -1);
    let generation = host.state.inner.lock().unwrap().playback_generation;
    host.state
        .inner
        .lock()
        .unwrap()
        .controls
        .push_back(playback::Control::BeginSeek(generation));
    host.tick().unwrap();
    assert_eq!(view(&host)["epoch"], "1");
    assert_eq!(view(&host)["mode"]["seeking"], true);
    seek(
        &mut host,
        generation,
        Err(anyhow::anyhow!("deliberate failed seek")),
    );
    assert_eq!(view(&host)["epoch"], "1");
    assert_eq!(view(&host)["state"]["tick"], "-1");
    assert!(view(&host)["mode"]["error"]
        .as_str()
        .unwrap()
        .contains("deliberate"));
    seek(&mut host, generation + 1, Ok(prepared(session, 2)));
    assert_eq!(view(&host)["epoch"], "1");
    assert_eq!(view(&host)["state"]["tick"], "-1");
}

#[test]
fn proof_failure_keeps_all_last_verified_facets_even_after_runtime_advances() {
    let mut host = host();
    load(&mut host, record(true, true), -1);
    let before = view(&host);
    action(&mut host, "step");
    let after = view(&host);
    for field in ["state", "presentation", "versions", "run", "events"] {
        assert_eq!(before[field], after[field], "cached {field}");
    }
    assert_eq!(after["mode"]["tick"], "-1");
    assert!(after["mode"]["error"]
        .as_str()
        .unwrap()
        .contains("diverged"));
    assert_eq!(after["timing"]["last"]["outcome"], "fault");
    assert_eq!(after["timing"]["last"]["runtimeAdvanced"], false);
    let game = host.state.inner.lock().unwrap();
    let failed = game.playback.as_ref().unwrap();
    assert_eq!(failed.runtime.current_tick().get(), 1);
    assert!(arena::inspect_content(&failed.runtime)
        .unwrap()
        .active
        .is_some());
    assert!(after["versions"]["content"]["activeHash"].is_null());
}

#[test]
fn failed_owner_attempt_is_fresh_without_a_successful_publication() {
    let mut host = host();
    host.state.inner.lock().unwrap().publication_id = u64::MAX;
    let before = view(&host);
    assert!(host.tick().is_err());
    let after = view(&host);
    assert_eq!(before["revision"], after["revision"]);
    assert_eq!(after["revision"], u64::MAX.to_string());
    assert_eq!(after["attempt"], "1");
    assert_eq!(after["state"], before["state"]);
    assert!(after["diagnostics"]["hostError"]
        .as_str()
        .unwrap()
        .contains("publication"));
    assert_eq!(after["timing"]["last"]["outcome"], "fault");
}

#[test]
fn observer_counter_or_capture_failures_do_not_discard_successful_game_outputs() {
    let (mut broken, mut normal) = (host(), host());
    broken.state.inner.lock().unwrap().inspection.attempt = u64::MAX;
    input(&mut broken, 1, "start", vec![]);
    input(&mut normal, 1, "start", vec![]);
    assert_eq!(broken.tick().unwrap(), normal.tick().unwrap());
    assert!(broken
        .inspection()
        .unwrap_err()
        .to_string()
        .contains("counter exhausted"));
    let mut event_overflow = host();
    let mut event_baseline = host();
    event_overflow
        .state
        .inner
        .lock()
        .unwrap()
        .inspection
        .next_sequence = u64::MAX;
    input(&mut event_overflow, 1, "start", vec![]);
    input(&mut event_baseline, 1, "start", vec![]);
    assert_eq!(
        event_overflow.tick().unwrap(),
        event_baseline.tick().unwrap()
    );
    assert!(event_overflow
        .inspection()
        .unwrap_err()
        .to_string()
        .contains("event counter"));
    let mut game = normal.state.inner.lock().unwrap();
    // An inspection-only malformed projection cannot bubble through game publication.
    let mut projection = game.application_projection();
    replace_json(&mut projection, "/render/arena/presentation", |p| {
        p["tick"] = json!(99)
    });
    let revision = game.publication_id;
    let error = game
        .inspection
        .commit(&projection, &[], revision, false, false, None)
        .unwrap_err();
    game.inspection.disable(error);
    drop(game);
    let output = normal.tick().unwrap();
    assert!(!output.is_empty());
    assert!(normal
        .inspection()
        .unwrap_err()
        .to_string()
        .contains("incoherent"));
}

#[test]
fn wide_adaptation_is_exact_and_does_not_convert_narrow_actor_fields() {
    let mut projection = host().inspect().unwrap();
    for address in [
        "/ui/arena/content",
        "/ui/arena/script",
        "/ui/arena/timeline",
    ] {
        replace_json(&mut projection, address, |p| p["run"] = json!(u64::MAX));
    }
    let change = |p: &mut Value| {
        p["run"] = json!(u64::MAX);
        p["tick"] = json!(i64::MAX);
        p["simulationStep"] = json!(u64::MAX);
    };
    replace_json(&mut projection, "/ui/arena/state", |p| {
        p["tick"] = json!(i64::MAX);
        p["simulationSteps"] = json!(u64::MAX);
    });
    replace_json(&mut projection, "/render/arena/presentation", change);
    replace_json(&mut projection, "/ui/arena/timeline", |p| {
        change(&mut p["presentation"])
    });
    let boss = json!({"id":"r18446744073709551615:c1","clipId":"boss-telegraph","entityId":199,"floor":5,"startedTick":i64::MAX,"offsetTick":u64::MAX,"nextEventIndex":3,"eventCount":5,"radius":3.0,"intensity":1.0});
    replace_json(&mut projection, "/render/arena/presentation", |p| {
        p["bosses"] = json!([boss.clone()])
    });
    replace_json(&mut projection, "/ui/arena/timeline", |p| {
        p["presentation"]["bosses"] = json!([boss])
    });
    replace_json(
        &mut projection,
        "/ui/arena/script",
        |p| p["fault"] = json!({"tick":i64::MAX,"enemyId":199,"scriptHash":"f".repeat(64),"diagnostic":{"kind":"runtime","message":"test","line":1,"column":2}}),
    );
    let captured = Projection::capture(&projection).unwrap();
    assert_eq!(
        captured.presentation.0["bosses"][0]["startedTick"],
        i64::MAX.to_string()
    );
    assert_eq!(
        captured.presentation.0["bosses"][0]["offsetTick"],
        u64::MAX.to_string()
    );
    assert_eq!(captured.presentation.0["bosses"][0]["entityId"], 199);
    assert_eq!(captured.presentation.0["bosses"][0]["nextEventIndex"], 3);
    assert_eq!(
        captured.script_fault.as_ref().unwrap().tick,
        i64::MAX.to_string()
    );
    assert_eq!(captured.state.0["tick"], i64::MAX.to_string());
    assert_eq!(captured.state.0["simulationSteps"], u64::MAX.to_string());
    assert_eq!(captured.presentation.0["run"], u64::MAX.to_string());
    assert_eq!(captured.state.0["inventory"]["equipment"][0]["id"], 1);
    let raw = json!({"id":u64::MAX,"sequence":9007199254740993u64}).to_string();
    let message = OscMessage {
        address: "/ui/arena/command".into(),
        args: vec![OscArg::Int64(i64::MIN), OscArg::Str(raw)],
    };
    let EventDetail::Message { message_json } = event_detail(&message).unwrap() else {
        panic!()
    };
    assert!(
        message_json.contains("18446744073709551615") && message_json.contains("9007199254740993")
    );
    assert!(message_json.contains("-9223372036854775808"));
}

#[test]
fn event_retention_has_count_bytes_exact_oversize_hash_and_stable_sequences() {
    let mut inspection = Inspection::new(&host().inspect().unwrap());
    let small = OscMessage {
        address: "/game/arena/test".into(),
        args: vec![OscArg::Int64(i64::MAX)],
    };
    let batch = vec![OscBundle {
        messages: vec![small; 300],
    }];
    inspection.capture_events(&batch, 1, 0, 1).unwrap();
    assert_eq!(inspection.events.len(), 256);
    assert_eq!(inspection.dropped, 44);
    assert_eq!(inspection.events.front().unwrap().0.sequence, "44");
    let big = OscMessage {
        address: "/game/arena/test".into(),
        args: vec![OscArg::Str("é\"".repeat(20000))],
    };
    let canonical = serde_json::to_vec(&WireMessageRef::new(&big)).unwrap();
    assert!(canonical.len() > PAYLOAD_BYTES);
    let EventDetail::Oversize {
        encoded_bytes,
        sha256,
    } = event_detail(&big).unwrap()
    else {
        panic!()
    };
    assert_eq!(encoded_bytes, canonical.len().to_string());
    assert_eq!(sha256, hex::encode(Sha256::digest(&canonical)));
    let escaped = OscMessage {
        address: "/game/arena/test".into(),
        args: vec![OscArg::Str("\"".repeat(15000))],
    };
    for _ in 0..70 {
        inspection
            .capture_events(
                &[OscBundle {
                    messages: vec![escaped.clone()],
                }],
                2,
                1,
                1,
            )
            .unwrap();
    }
    assert!(inspection.events.len() < 70);
    assert!(inspection.event_bytes <= EVENT_BYTES);
    assert_eq!(
        inspection.event_bytes,
        inspection
            .events
            .iter()
            .map(|(e, n)| {
                assert_eq!(*n, serde_json::to_vec(e).unwrap().len());
                n
            })
            .sum::<usize>()
    );
    inspection
        .capture_events(
            &[OscBundle {
                messages: vec![big],
            }],
            3,
            2,
            1,
        )
        .unwrap();
    assert_eq!(inspection.omitted_payloads, 1);
}

#[test]
fn honest_timing_window_percentiles_and_clamping_use_measured_attempts() {
    let host = host();
    let mut game = host.state.inner.lock().unwrap();
    for n in 1..=20 {
        game.inspection.begin_attempt();
        game.inspection
            .finish(
                0,
                Outcome::Idle,
                false,
                false,
                Duration::from_micros(n),
                Duration::ZERO,
            )
            .unwrap();
    }
    let mut snapshot = detached(&game).unwrap();
    statistics(&mut snapshot);
    assert_eq!(snapshot.timing.mean_us, Some(10.5));
    assert_eq!(snapshot.timing.p95_us, Some(19.0));
    game.inspection.begin_attempt();
    game.inspection
        .finish(
            0,
            Outcome::Idle,
            false,
            false,
            Duration::from_secs(86401),
            Duration::from_secs(86402),
        )
        .unwrap();
    let last = game.inspection.samples.back().unwrap();
    assert!(last.duration_clamped && last.lock_wait_clamped && last.over_budget);
    assert_eq!(last.duration_us, MAX_DURATION_US);
    for _ in 0..300 {
        game.inspection.begin_attempt();
        game.inspection
            .finish(
                0,
                Outcome::Idle,
                false,
                false,
                Duration::ZERO,
                Duration::ZERO,
            )
            .unwrap();
    }
    assert_eq!(game.inspection.samples.len(), 256);
    assert_eq!(game.inspection.total_samples, 321);
}

#[test]
fn response_limit_and_bounded_diagnostic_are_explicit_not_partial_success() {
    let mut writer = ResponseWriter(Vec::new());
    writer.write_all(&vec![b'a'; RESPONSE_BYTES]).unwrap();
    assert!(writer.write_all(b"x").is_err());
    assert_eq!(writer.0.len(), RESPONSE_BYTES);
    assert!(writer.0.capacity() <= RESPONSE_BYTES);
    let text = bounded("界".repeat(3000));
    assert!(text.len() <= 4096);
    assert!(text.ends_with("… [truncated]"));
}

#[test]
fn export_backend_inspection_contract_examples_when_requested() {
    let Ok(path) = std::env::var("KITU_INSPECTION_EVIDENCE_DIR") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    std::fs::create_dir_all(&path).unwrap();
    let mut host = host();
    std::fs::write(
        path.join("initial.json"),
        serde_json::to_vec_pretty(&view(&host)).unwrap(),
    )
    .unwrap();
    input(&mut host, 1, "start", vec![]);
    host.tick().unwrap();
    std::fs::write(
        path.join("live.json"),
        serde_json::to_vec_pretty(&view(&host)).unwrap(),
    )
    .unwrap();
    load(&mut host, record(true, false), 0);
    std::fs::write(
        path.join("replay.json"),
        serde_json::to_vec_pretty(&view(&host)).unwrap(),
    )
    .unwrap();
}
