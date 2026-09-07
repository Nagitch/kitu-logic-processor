//! Actual loopback HTTP clients observe and control the native-owned Runtime.
use kitu_demo_game_native::*;
use kitu_unity_ffi::application::{ApplicationHandle, BUFFER_TOO_SMALL, DRIVER_ERROR, OK};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    ptr,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::{Duration, Instant},
};

type Reader = unsafe extern "C" fn(*mut ApplicationHandle, *mut u8, usize, *mut usize) -> i32;
struct Native(*mut ApplicationHandle);
impl Native {
    fn create(config: Value) -> Self {
        let bytes = serde_json::to_vec(&config).unwrap();
        let (mut handle, mut needed) = (ptr::null_mut(), 0);
        let mut error = vec![0; 4096];
        let status = unsafe {
            kitu_application_create(
                1,
                bytes.as_ptr(),
                bytes.len(),
                &mut handle,
                error.as_mut_ptr(),
                error.len(),
                &mut needed,
            )
        };
        assert_eq!(
            status,
            OK,
            "{}",
            String::from_utf8_lossy(&error[..needed.min(error.len())])
        );
        Self(handle)
    }
    fn read(&self, reader: Reader) -> Value {
        let mut needed = 0;
        assert_eq!(
            unsafe { reader(self.0, ptr::null_mut(), 0, &mut needed) },
            BUFFER_TOO_SMALL
        );
        let mut bytes = vec![0; needed];
        assert_eq!(
            unsafe { reader(self.0, bytes.as_mut_ptr(), bytes.len(), &mut needed) },
            OK
        );
        serde_json::from_slice(&bytes).unwrap()
    }
    fn metadata(&self) -> Value {
        let output = self.read(kitu_application_inspect_host_json);
        assert_eq!(output[0]["messages"][0]["address"], "/host/arena/status");
        serde_json::from_str(
            output[0]["messages"][0]["args"][0]["value"]
                .as_str()
                .unwrap(),
        )
        .unwrap()
    }
    fn state(&self) -> Value {
        let output = self.read(kitu_application_inspect_json);
        serde_json::from_str(
            output[0]["messages"][0]["args"][0]["value"]
                .as_str()
                .unwrap(),
        )
        .unwrap()
    }
    fn projection(&self, address: &str) -> Value {
        let output = self.read(kitu_application_inspect_json);
        let message = output
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|bundle| bundle["messages"].as_array().unwrap())
            .find(|message| message["address"] == address)
            .unwrap_or_else(|| panic!("missing projection {address}"));
        serde_json::from_str(message["args"][0]["value"].as_str().unwrap()).unwrap()
    }
    fn tick(&self) -> Value {
        assert_eq!(unsafe { kitu_application_tick(self.0) }, OK);
        self.read(kitu_application_read_output)
    }
    fn command(&self, address: &str, id: u64) -> i32 {
        self.submit(address, "native-test", id, json!([]))
    }
    fn submit(&self, address: &str, source: &str, id: u64, args: Value) -> i32 {
        let bytes = serde_json::to_vec(
            &json!({"metadata":{"source":source,"messageId":id,"schemaVersion":1},
            "bundle":{"messages":[{"address":address,"args":args}]}}),
        )
        .unwrap();
        let mut sequence = u64::MAX;
        unsafe { kitu_application_submit_json(self.0, bytes.as_ptr(), bytes.len(), &mut sequence) }
    }
    fn complete(&self, task: thread::JoinHandle<Value>) -> Value {
        let deadline = Instant::now() + Duration::from_secs(15);
        while !task.is_finished() {
            assert!(
                Instant::now() < deadline,
                "bridge operation failed to complete while native owner ticked"
            );
            self.tick();
            thread::sleep(Duration::from_millis(2));
        }
        task.join().unwrap()
    }
}

#[test]
fn native_management_ids_do_not_exhaust_admin_or_shell_staging() {
    use kitu_demo_game::arena::{
        config::{ArenaConfig, ContentVersion},
        script::{default_script, ScriptVersion},
    };
    let storage = Storage::new();
    let native = Native::create(json!({
        "bridge": {"enabled": true, "address": "127.0.0.1:0"},
        "storageDirectory": storage.0,
    }));
    let metadata = native.metadata();
    let endpoint = metadata["bridgeEndpoint"].as_str().unwrap();
    let script = default_script().unwrap();
    let native_script =
        ScriptVersion::from_source(&script.source.replace("duration: 0.8", "duration: 2.4"))
            .unwrap();
    std::fs::write(
        storage.0.join("boss.rhai"),
        script.source.replace("duration: 0.8", "duration: 1.6"),
    )
    .unwrap();
    let native_content =
        ContentVersion::from_tmd(&ArenaConfig::default().to_tmd().unwrap()).unwrap();
    let mut edited = ArenaConfig::default();
    edited.items[0].damage += 3;
    std::fs::write(storage.0.join("arena.tmd"), edited.to_tmd().unwrap()).unwrap();
    let native_timeline = edited_timeline(3.0, 0.35);
    write_timeline(&storage.0.join("timelines"), &edited_timeline(4.5, 0.85));

    for (kind_index, (kind, address, detached)) in [
        ("script", "/input/arena/script", json!(native_script)),
        (
            "content",
            "/input/arena/config",
            serde_json::from_str(&serde_json::to_string(&native_content).unwrap()).unwrap(),
        ),
        ("timeline", "/input/arena/timeline", json!(native_timeline)),
    ]
    .into_iter()
    .enumerate()
    {
        let status_path = format!("/arena/{kind}");
        let validated = native.complete(post(
            endpoint,
            &format!("{status_path}/validate"),
            json!({}),
        ));
        let candidate = &validated["candidate"];
        assert_ne!(candidate["hash"], detached["hash"]);
        for (index, native_id) in [1_000_000, u64::MAX].into_iter().enumerate() {
            let source = format!("host:arena-{kind}");
            assert_eq!(
                native.submit(
                    address,
                    &source,
                    native_id,
                    json!([{"type":"str","value":detached.to_string()}]),
                ),
                OK
            );
            native.tick();
            assert_eq!(get(endpoint, &status_path)["runtime"]["pending"], detached);
            if index == 0 {
                // Admin returns queue admission; the next native tick must commit it.
                post(
                    endpoint,
                    &format!("{status_path}/stage"),
                    if kind != "content" {
                        json!({"hash":candidate["hash"]})
                    } else {
                        json!({"hash":candidate["hash"],"sourceSha256":candidate["sourceSha256"]})
                    },
                )
                .join()
                .unwrap();
                let committed = native.tick();
                let receipt: Value = committed
                    .as_array()
                    .unwrap()
                    .iter()
                    .flat_map(|bundle| bundle["messages"].as_array().unwrap())
                    .filter(|message| message["address"] == "/ui/arena/command")
                    .map(|message| {
                        serde_json::from_str(message["args"][0]["value"].as_str().unwrap()).unwrap()
                    })
                    .find(|receipt: &Value| receipt["source"] == format!("host:arena-{kind}-admin"))
                    .expect("Admin command is committed through the native tick");
                assert_eq!(receipt["id"], 1);
                assert_eq!(receipt["accepted"], true, "{receipt}");
                assert_eq!(receipt["duplicate"], false, "{receipt}");
            } else {
                let mut args = vec![kind, "stage", candidate["hash"].as_str().unwrap()];
                if kind == "content" {
                    args.push(candidate["sourceSha256"].as_str().unwrap());
                }
                let staged = native.complete(shell(
                    endpoint,
                    &metadata["sessionId"],
                    kind_index as u64 + 1,
                    &args,
                ));
                assert_eq!(staged["ok"], true, "{staged}");
                assert_eq!(staged["data"]["receipt"]["accepted"], true, "{staged}");
                assert_eq!(staged["data"]["receipt"]["duplicate"], false, "{staged}");
                assert_eq!(staged["data"]["receipt"]["id"], 2);
                assert_eq!(
                    staged["data"]["receipt"]["source"],
                    format!("host:arena-{kind}-admin")
                );
            }
            assert_eq!(
                get(endpoint, &status_path)["runtime"]["pending"],
                *candidate
            );
        }
    }
    // The recording contains both caller-chosen high/MAX IDs and host-owned IDs.
    // Re-execution accepts those original identities without renumbering either.
    let saved = native.complete(post(endpoint, "/arena/recording/save", json!({})));
    let verified = native.complete(post(
        endpoint,
        &format!("/arena/recordings/{}/verify", saved["id"].as_str().unwrap()),
        json!({}),
    ));
    assert_eq!(verified["inputs"], 12);
}

#[test]
fn native_inputs_cannot_impersonate_admin_management_producers() {
    let native = Native::create(json!({}));
    for source in [
        "host:arena-script-admin",
        "host:arena-content-admin",
        "host:arena-timeline-admin",
    ] {
        // Even an ordinary gameplay command would poison the source high-water.
        assert_eq!(
            native.submit("/input/arena/start", source, u64::MAX, json!([])),
            DRIVER_ERROR
        );
    }
    assert_eq!(native.state()["phase"], 0);
    assert_eq!(native.command("/input/arena/start", 1), OK);
    native.tick();
    assert_eq!(native.state()["phase"], 1);
}
impl Drop for Native {
    fn drop(&mut self) {
        assert_eq!(unsafe { kitu_application_destroy(self.0) }, OK);
    }
}
struct Storage(PathBuf);
impl Storage {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(std::env::temp_dir().join(format!(
            "arena-native-bridge-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}
impl Drop for Storage {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn agent() -> ureq::Agent {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(10)))
            .build(),
    )
}
fn get(endpoint: &str, path: &str) -> Value {
    agent()
        .get(format!("{endpoint}{path}"))
        .call()
        .unwrap()
        .body_mut()
        .read_json()
        .unwrap()
}
fn post(endpoint: &str, path: &str, body: Value) -> thread::JoinHandle<Value> {
    let url = format!("{endpoint}{path}");
    thread::spawn(move || {
        agent()
            .post(url)
            .send_json(body)
            .unwrap()
            .body_mut()
            .read_json()
            .unwrap()
    })
}
fn shell(endpoint: &str, session: &Value, id: u64, args: &[&str]) -> thread::JoinHandle<Value> {
    post(
        endpoint,
        "/shell/execute",
        json!({"version":1,"sessionId":session,"clientId":"native-integration","id":id,"args":args}),
    )
}

fn edited_timeline(
    radius: f32,
    opacity: f32,
) -> kitu_demo_game::arena::presentation::TimelineVersion {
    use kitu_demo_game::arena::presentation::{default_timeline, TimelineVersion};
    use kitu_osc_ir::OscArg;
    use kitu_tsq1::presentation::Clip;
    let defaults = default_timeline().unwrap();
    let mut boss = Clip::decode(&defaults.clips[0].bytes).unwrap();
    for event in &mut boss.events {
        for message in &mut event.bundle.messages {
            message.args[0] = OscArg::Float(radius);
        }
    }
    let mut floor = Clip::decode(&defaults.clips[1].bytes).unwrap();
    for event in &mut floor.events {
        if event.offset_tick == 12 {
            for message in &mut event.bundle.messages {
                message.args[0] = OscArg::Float(opacity);
            }
        }
    }
    TimelineVersion::from_sources(&boss.encode().unwrap(), &floor.encode().unwrap()).unwrap()
}

fn write_timeline(
    directory: &std::path::Path,
    version: &kitu_demo_game::arena::presentation::TimelineVersion,
) {
    std::fs::create_dir_all(directory).unwrap();
    for clip in &version.clips {
        std::fs::write(directory.join(format!("{}.tsq", clip.id)), &clip.bytes).unwrap();
    }
}

#[test]
fn timeline_authoring_reloads_next_run_and_replays_live_cues_without_source_files() {
    use kitu_demo_game::arena::presentation::default_timeline;

    let storage = Storage::new();
    let detached = edited_timeline(3.0, 0.35);
    let native = Native::create(json!({
        "bridge": {"enabled":true,"address":"127.0.0.1:0"},
        "storageDirectory":storage.0, "timeline":detached,
    }));
    let metadata = native.metadata();
    let endpoint = metadata["bridgeEndpoint"].as_str().unwrap();
    let directory = storage.0.join("timelines");
    for clip in default_timeline().unwrap().clips {
        assert_eq!(
            std::fs::read(directory.join(format!("{}.tsq", clip.id))).unwrap(),
            clip.bytes
        );
    }
    assert_eq!(
        get(endpoint, "/arena/timeline")["runtime"]["pending"],
        json!(detached),
        "seeding editable clips must preserve detached factory rules"
    );
    assert_eq!(native.command("/input/arena/start", 1), OK);
    native.tick();

    let authored = edited_timeline(4.5, 0.85);
    write_timeline(&directory, &authored);
    let valid = native.complete(shell(
        endpoint,
        &metadata["sessionId"],
        1,
        &["timeline", "validate"],
    ));
    assert_eq!(valid["ok"], true, "{valid}");
    assert_eq!(valid["data"]["candidate"], json!(authored));
    let staged = native.complete(shell(
        endpoint,
        &metadata["sessionId"],
        2,
        &["timeline", "stage", &authored.hash],
    ));
    assert_eq!(staged["ok"], true, "{staged}");
    let pending = get(endpoint, "/arena/timeline");
    assert_eq!(pending["runtime"]["active"], json!(detached));
    assert_eq!(pending["runtime"]["pending"], json!(authored));
    assert_eq!(native.command("/input/arena/menu", 2), OK);
    native.tick();
    assert_eq!(native.command("/input/arena/start", 3), OK);
    native.tick();
    assert_eq!(
        get(endpoint, "/arena/timeline")["runtime"]["active"],
        json!(authored)
    );

    // Walk the real starter weapon through the preparation portal. No test-only
    // state setter creates the floor cue or changes its authored cursor.
    assert_eq!(
        native.submit(
            "/input/arena/frame",
            "timeline-walk",
            1,
            json!([
                {"type":"float","value":0.0},{"type":"float","value":1.0},
                {"type":"bool","value":false},{"type":"float","value":0.0},
                {"type":"float","value":0.0},{"type":"bool","value":false},
                {"type":"bool","value":false},
            ])
        ),
        OK
    );
    let mut cue_observed = false;
    for _ in 0..240 {
        native.tick();
        let presentation = native.projection("/render/arena/presentation");
        if presentation["floor"]["offsetTick"] == 12 {
            assert_eq!(presentation["floor"]["opacity"], 0.85);
            cue_observed = true;
            break;
        }
    }
    assert!(
        cue_observed,
        "authored floor cue must be reached through gameplay"
    );
    assert_eq!(native.command("/input/arena/pause", 4), OK);
    native.tick();
    let saved_presentation = native.projection("/render/arena/presentation");
    let saved_state = native.state();
    assert_eq!(saved_state["overlay"], "pause");
    assert_eq!(saved_presentation["floor"]["offsetTick"], 12);
    assert_eq!(
        get(endpoint, "/arena/timeline")["runtime"]["presentation"],
        saved_presentation
    );
    // Saving needs background I/O, not a new owner tick. Preserve the exact final
    // presentation (including its management tick) as an independent seek oracle.
    let saved = post(endpoint, "/arena/recording/save", json!({}))
        .join()
        .unwrap();
    assert_eq!(
        native.projection("/render/arena/presentation"),
        saved_presentation
    );

    std::fs::write(directory.join("boss-telegraph.tsq"), b"not TSQ1").unwrap();
    let invalid = native.complete(shell(
        endpoint,
        &metadata["sessionId"],
        3,
        &["timeline", "validate"],
    ));
    assert_eq!(invalid["ok"], false);
    assert!(invalid["data"]["candidate"].is_null());
    assert_eq!(invalid["data"]["runtime"]["active"], json!(authored));
    assert_eq!(invalid["data"]["runtime"]["pending"], json!(authored));
    write_timeline(&directory, &authored);
    std::fs::remove_file(directory.join("floor-transition.tsq")).unwrap();
    let missing = native.complete(post(endpoint, "/arena/timeline/validate", json!({})));
    assert!(!missing["diagnostics"].as_array().unwrap().is_empty());
    assert!(missing["candidate"].is_null());
    assert_eq!(missing["runtime"]["active"], json!(authored));
    assert_eq!(missing["runtime"]["pending"], json!(authored));

    // Hold a different valid candidate, then remove both files. Old replay
    // behavior must come from recorded bytes, and replay staging must fail for
    // read-only admission rather than merely a missing/invalid candidate.
    let next = edited_timeline(6.0, 0.25);
    write_timeline(&directory, &next);
    let validated = native.complete(shell(
        endpoint,
        &metadata["sessionId"],
        4,
        &["timeline", "validate"],
    ));
    assert_eq!(validated["ok"], true);
    assert_eq!(validated["data"]["candidate"], json!(next));
    std::fs::remove_dir_all(&directory).unwrap();
    let id = saved["id"].as_str().unwrap();
    let verified = post(
        endpoint,
        &format!("/arena/recordings/{id}/verify"),
        json!({}),
    )
    .join()
    .unwrap();
    assert_eq!(verified["runs"], 2);
    assert_eq!(verified["state"], saved_state);
    let last_tick = verified["ticks"].as_i64().unwrap() - 1;
    native.complete(post(endpoint, "/arena/playback/load", json!({"id":id})));
    // HTTP acknowledges the prepared replay; activation belongs to the next
    // native owner tick even if the request finishes between complete() polls.
    native.tick();
    assert_eq!(
        get(endpoint, "/arena/timeline")["runtime"]["pending"],
        json!(detached)
    );
    native.complete(post(
        endpoint,
        "/arena/playback/command",
        json!({"action":"step"}),
    ));
    assert_eq!(
        get(endpoint, "/arena/timeline")["runtime"]["active"],
        json!(detached)
    );
    native.complete(post(
        endpoint,
        "/arena/playback/seek",
        json!({"tick":last_tick}),
    ));
    assert_eq!(native.state(), saved_state);
    assert_eq!(
        native.projection("/render/arena/presentation"),
        saved_presentation
    );
    let replay = get(endpoint, "/arena/timeline");
    assert_eq!(replay["readOnly"], true);
    assert_eq!(replay["runtime"]["active"], json!(authored));
    assert_eq!(replay["runtime"]["presentation"], saved_presentation);
    assert_eq!(replay["candidate"], json!(next));
    let refused = native.complete(shell(
        endpoint,
        &metadata["sessionId"],
        5,
        &["timeline", "stage", &next.hash],
    ));
    assert_eq!(refused["ok"], false);
    assert!(
        refused["error"]
            .as_str()
            .unwrap()
            .contains("replay is read-only"),
        "{refused}"
    );
    native.complete(post(
        endpoint,
        "/arena/playback/command",
        json!({"action":"live"}),
    ));
    assert_eq!(native.state()["overlay"], "pause");
    assert_eq!(
        get(endpoint, "/arena/timeline")["runtime"]["active"],
        json!(authored)
    );
    write_timeline(&directory, &next);
}

#[test]
fn explicit_timeline_authoring_directories_are_never_seeded_or_replaced() {
    let storage = Storage::new();
    let external = Storage::new();
    let authored = edited_timeline(5.0, 0.75);
    write_timeline(&external.0, &authored);
    let native = Native::create(json!({
        "storageDirectory":storage.0,"timelineDirectory":external.0,
    }));
    for clip in &authored.clips {
        assert_eq!(
            std::fs::read(external.0.join(format!("{}.tsq", clip.id))).unwrap(),
            clip.bytes
        );
    }
    assert!(!storage.0.join("timelines").exists());
    assert_ne!(
        native.projection("/ui/arena/timeline")["pending"],
        json!(authored),
        "authoring files are only read by explicit validation"
    );
    drop(native);
    let missing = external.0.join("missing");
    let _native = Native::create(json!({
        "storageDirectory":storage.0,"timelineDirectory":missing,
    }));
    assert!(
        !missing.exists(),
        "an external source directory is never created"
    );
}

#[test]
fn script_reload_uses_the_native_clock_and_replays_after_source_removal() {
    use kitu_demo_game::arena::script::{default_script, ScriptVersion};
    let storage = Storage::new();
    let initial = default_script().unwrap();
    let detached =
        ScriptVersion::from_source(&initial.source.replace("duration: 0.8", "duration: 1.2"))
            .unwrap();
    let native = Native::create(json!({
        "bridge": {"enabled": true, "address": "127.0.0.1:0"},
        "storageDirectory": storage.0, "script": detached,
    }));
    let metadata = native.metadata();
    let endpoint = metadata["bridgeEndpoint"].as_str().unwrap();
    let path = storage.0.join("boss.rhai");
    assert_eq!(std::fs::read_to_string(&path).unwrap(), initial.source);
    assert_eq!(
        get(endpoint, "/arena/script")["runtime"]["pending"]["hash"],
        detached.hash,
        "seeding the authoring source must not replace detached factory rules"
    );
    assert_eq!(native.command("/input/arena/start", 1), OK);
    native.tick();
    std::fs::write(
        &path,
        initial.source.replace("duration: 0.8", "duration: 1.6"),
    )
    .unwrap();
    let validated = native.complete(shell(
        endpoint,
        &metadata["sessionId"],
        1,
        &["script", "validate"],
    ));
    assert_eq!(validated["ok"], true, "{validated}");
    let candidate = validated["data"]["candidate"].clone();
    let staged = native.complete(shell(
        endpoint,
        &metadata["sessionId"],
        2,
        &["script", "stage", candidate["hash"].as_str().unwrap()],
    ));
    assert_eq!(staged["ok"], true, "{staged}");
    assert_eq!(
        get(endpoint, "/arena/script")["runtime"]["active"]["hash"],
        detached.hash
    );
    assert_eq!(
        get(endpoint, "/arena/script")["runtime"]["pending"],
        candidate
    );
    assert_eq!(native.command("/input/arena/menu", 2), OK);
    native.tick();
    assert_eq!(native.command("/input/arena/start", 3), OK);
    native.tick();
    assert_eq!(
        get(endpoint, "/arena/script")["runtime"]["active"],
        candidate
    );
    let saved = native.complete(post(endpoint, "/arena/recording/save", json!({})));
    std::fs::remove_file(&path).unwrap();
    let invalid = native.complete(shell(
        endpoint,
        &metadata["sessionId"],
        3,
        &["script", "validate"],
    ));
    assert_eq!(invalid["ok"], false);
    assert!(invalid["data"]["candidate"].is_null());
    assert_eq!(invalid["data"]["runtime"]["active"], candidate);
    let id = saved["id"].as_str().unwrap();
    let verified = native.complete(post(
        endpoint,
        &format!("/arena/recordings/{id}/verify"),
        json!({}),
    ));
    assert_eq!(verified["runs"], 2);
    native.complete(post(endpoint, "/arena/playback/load", json!({"id":id})));
    native.tick();
    let initial_replay = get(endpoint, "/arena/script");
    assert_eq!(initial_replay["readOnly"], true);
    assert_eq!(initial_replay["runtime"]["pending"]["hash"], detached.hash);
    let last_tick = verified["ticks"].as_u64().unwrap() as i64 - 1;
    native.complete(post(
        endpoint,
        "/arena/playback/seek",
        json!({"tick":last_tick}),
    ));
    assert_eq!(
        get(endpoint, "/arena/script")["runtime"]["active"],
        candidate
    );
    // Keep a valid candidate while replay is observed, so refusal specifically
    // exercises read-only admission rather than the earlier missing source.
    std::fs::write(&path, candidate["source"].as_str().unwrap()).unwrap();
    let revalidated = native.complete(post(endpoint, "/arena/script/validate", json!({})));
    assert_eq!(revalidated["candidate"], candidate);
    let refused = native.complete(shell(
        endpoint,
        &metadata["sessionId"],
        4,
        &["script", "stage", candidate["hash"].as_str().unwrap()],
    ));
    assert_eq!(refused["ok"], false);
    assert!(
        refused["error"].as_str().unwrap().contains("replay"),
        "{refused}"
    );
    native.complete(post(
        endpoint,
        "/arena/playback/command",
        json!({"action":"live"}),
    ));
    assert_eq!(native.state()["overlay"], "pause");
}

#[test]
fn layered_sqlite_content_survives_source_removal_and_replays_in_the_native_host() {
    use kitu_demo_game::arena::config::{write_sqlite, ArenaConfig};

    let storage = Storage::new();
    std::fs::create_dir_all(&storage.0).unwrap();
    let database = storage.0.join("base.sqlite");
    let debug = storage.0.join("debug.tmd");
    let plan = storage.0.join("arena.arena.json");
    let mut base = ArenaConfig::default();
    base.items[0].damage = 23;
    write_sqlite(&database, &base.to_tables().unwrap()).unwrap();
    let mut edited = base.clone();
    edited.items[0].damage = 37;
    std::fs::write(&debug, edited.to_tmd().unwrap()).unwrap();
    std::fs::write(
        &plan,
        serde_json::to_vec(&json!({
            "version": 1,
            "base": {"format": "sqlite", "path": "base.sqlite"},
            "debug": {"format": "tmd", "path": "debug.tmd"}
        }))
        .unwrap(),
    )
    .unwrap();

    let native = Native::create(json!({
        "bridge": {"enabled": true, "address": "127.0.0.1:0"},
        "storageDirectory": storage.0,
        "contentPath": plan
    }));
    let metadata = native.metadata();
    let endpoint = metadata["bridgeEndpoint"].as_str().unwrap();
    assert_eq!(
        get(endpoint, "/arena/content")["runtime"]["pending"]["hash"],
        ArenaConfig::default().hash().unwrap(),
        "an authoring source cannot replace the factory's detached initial settings"
    );
    assert_eq!(native.command("/input/arena/start", 1), OK);
    native.tick();
    assert_eq!(native.state()["inventory"]["equipment"][0]["damage"], 20);

    let validated = native.complete(post(endpoint, "/arena/content/validate", json!({})));
    assert_eq!(validated["diagnostics"], json!([]));
    assert_eq!(validated["sources"][0]["format"], "sqlite");
    assert_eq!(validated["sources"][1]["layer"], "debug");
    assert_eq!(
        validated["origins"]["candidate"]["/items/starter/damage"],
        "debug"
    );
    let difference = validated["differences"]["active"]
        .as_array()
        .unwrap()
        .iter()
        .find(|difference| difference["path"] == "/items/starter/damage")
        .unwrap();
    assert_eq!(
        (difference["before"].clone(), difference["after"].clone()),
        (json!(20), json!(37))
    );
    assert_eq!(difference["winningLayer"], "debug");
    let candidate = validated["candidate"].clone();
    assert!(
        candidate.get("tanuRevision").is_none(),
        "stack evaluators are recorded per source"
    );
    native.complete(post(
        endpoint,
        "/arena/content/stage",
        json!({
            "hash": candidate["hash"], "sourceSha256": candidate["sourceSha256"]
        }),
    ));
    native.tick();
    assert_eq!(
        get(endpoint, "/arena/content")["runtime"]["pending"],
        candidate
    );
    assert_eq!(native.state()["inventory"]["equipment"][0]["damage"], 20);
    assert_eq!(native.command("/input/arena/menu", 2), OK);
    native.tick();
    assert_eq!(native.command("/input/arena/start", 3), OK);
    native.tick();
    assert_eq!(native.state()["inventory"]["equipment"][0]["damage"], 37);
    let saved = native.complete(post(endpoint, "/arena/recording/save", json!({})));

    std::fs::remove_file(database).unwrap();
    std::fs::remove_file(debug).unwrap();
    let invalid = native.complete(post(endpoint, "/arena/content/validate", json!({})));
    assert!(!invalid["diagnostics"].as_array().unwrap().is_empty());
    assert!(invalid["candidate"].is_null());
    assert_eq!(invalid["sources"], json!([]));
    assert!(invalid["differences"]["active"].is_null());
    assert_eq!(invalid["runtime"]["active"], candidate);
    assert_eq!(invalid["runtime"]["pending"], candidate);
    let id = saved["id"].as_str().unwrap();
    let verified = native.complete(post(
        endpoint,
        &format!("/arena/recordings/{id}/verify"),
        json!({}),
    ));
    let last_tick = verified["ticks"].as_u64().unwrap() as i64 - 1;
    native.complete(post(endpoint, "/arena/playback/load", json!({"id": id})));
    native.tick();
    native.complete(post(
        endpoint,
        "/arena/playback/command",
        json!({"action": "step"}),
    ));
    let sought = native.complete(post(
        endpoint,
        "/arena/playback/seek",
        json!({"tick": last_tick}),
    ));
    assert_eq!(sought["mode"]["tick"], last_tick);
    assert_eq!(sought["contentHash"], candidate["hash"]);
    assert_eq!(native.state()["inventory"]["equipment"][0]["damage"], 37);
    assert_eq!(
        get(endpoint, "/arena/content")["runtime"]["active"],
        candidate
    );
    native.complete(post(
        endpoint,
        "/arena/playback/command",
        json!({"action": "live"}),
    ));
    assert_eq!(native.state()["overlay"], "pause");
}

#[test]
fn bridge_commands_content_and_replay_share_the_native_clock_and_session() {
    let storage = Storage::new();
    let native = Native::create(
        json!({"bridge":{"enabled":true,"address":"127.0.0.1:0"},"storageDirectory":storage.0}),
    );
    let host = native.metadata();
    let endpoint = host["bridgeEndpoint"].as_str().unwrap();
    assert_eq!(
        get(endpoint, "/shell/catalog")["sessionId"],
        host["sessionId"]
    );
    let before = native.state();
    thread::sleep(Duration::from_millis(30));
    assert_eq!(get(endpoint, "/state")["tick"], 0);
    assert_eq!(
        native.state(),
        before,
        "bridge must never start an independent clock"
    );
    let start = native.complete(shell(
        endpoint,
        &host["sessionId"],
        1,
        &["app", "action", "run", "arena.start"],
    ));
    assert_eq!(start["ok"], true, "{start}");
    assert_eq!(native.state()["phase"], 1);
    let again = native.complete(shell(
        endpoint,
        &host["sessionId"],
        1,
        &["app", "action", "run", "arena.start"],
    ));
    assert_eq!(
        again, start,
        "retried operator input must keep its original receipt"
    );
    assert_eq!(native.command("/input/arena/disconnect", 1), OK);
    native.tick();
    assert_eq!(native.state()["overlay"], "pause");
    let elapsed = native.state()["elapsed"].clone();
    for _ in 0..8 {
        native.tick();
    }
    assert_eq!(native.state()["elapsed"], elapsed);
    assert_eq!(native.metadata()["sessionId"], host["sessionId"]);
    assert_eq!(native.command("/input/arena/resume", 2), OK);
    native.tick();
    assert_eq!(native.state()["overlay"], "none");
    let earlier = native.complete(post(endpoint, "/arena/recording/save", json!({})));

    // Real editable TMD remains next-run content and invalid edits preserve it.
    let initial = get(endpoint, "/arena/content");
    let mut config = kitu_demo_game::arena::config::ArenaConfig::default();
    config.items[0].damage += 3;
    std::fs::write(storage.0.join("arena.tmd"), config.to_tmd().unwrap()).unwrap();
    let validated = native.complete(post(endpoint, "/arena/content/validate", json!({})));
    assert_eq!(validated["diagnostics"], json!([]));
    let candidate = &validated["candidate"];
    native.complete(post(
        endpoint,
        "/arena/content/stage",
        json!({"hash":candidate["hash"],"sourceSha256":candidate["sourceSha256"]}),
    ));
    native.tick();
    let staged = get(endpoint, "/arena/content");
    assert_eq!(staged["runtime"]["active"], initial["runtime"]["active"]);
    assert_eq!(staged["runtime"]["pending"]["hash"], candidate["hash"]);
    std::fs::write(storage.0.join("arena.tmd"), b"invalid TMD").unwrap();
    let invalid = native.complete(post(endpoint, "/arena/content/validate", json!({})));
    assert!(!invalid["diagnostics"].as_array().unwrap().is_empty());
    assert_eq!(invalid["runtime"]["pending"]["hash"], candidate["hash"]);
    assert_eq!(native.command("/input/arena/menu", 3), OK);
    native.tick();
    assert_eq!(native.command("/input/arena/start", 4), OK);
    native.tick();
    assert_eq!(
        get(endpoint, "/arena/content")["runtime"]["active"]["hash"],
        candidate["hash"]
    );
    let earlier_id = earlier["id"].as_str().unwrap();
    native.complete(post(
        endpoint,
        &format!("/arena/recordings/{earlier_id}/verify"),
        json!({}),
    ));

    let saved = native.complete(post(endpoint, "/arena/recording/save", json!({})));
    let recording = saved["id"].as_str().unwrap();
    let verified = native.complete(post(
        endpoint,
        &format!("/arena/recordings/{recording}/verify"),
        json!({}),
    ));
    assert!(verified["ticks"].as_u64().unwrap() > 0);
    native.complete(post(
        endpoint,
        "/arena/playback/load",
        json!({"id":recording}),
    ));
    native.tick();
    assert_eq!(native.metadata()["playbackMode"]["active"], true);
    assert_eq!(native.metadata()["readOnly"], true);
    assert_eq!(native.command("/input/arena/start", 5), DRIVER_ERROR);
    native.complete(shell(endpoint, &host["sessionId"], 2, &["replay", "step"]));
    native.tick();
    assert!(native.metadata()["playbackMode"]["tick"].as_i64().unwrap() >= 0);
    native.complete(shell(
        endpoint,
        &host["sessionId"],
        3,
        &["replay", "seek", "0"],
    ));
    native.tick();
    assert_eq!(native.metadata()["playbackMode"]["tick"], 0);
    native.complete(shell(endpoint, &host["sessionId"], 4, &["replay", "live"]));
    native.tick();
    assert_eq!(native.metadata()["readOnly"], false);
    assert_eq!(native.state()["overlay"], "pause");
    let address = endpoint.trim_start_matches("http://").to_string();
    drop(native);
    assert!(
        std::net::TcpStream::connect(&address).is_err(),
        "destroy must join and release bridge listener"
    );
    let rebound = Native::create(json!({"bridge":{"enabled":true,"address":address}}));
    assert_ne!(rebound.metadata()["sessionId"], host["sessionId"]);
}

#[test]
fn bridge_configuration_rejects_remote_addresses_and_occupied_ports_without_a_handle() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    for config in [
        json!({"bridge":{"enabled":true,"address":"0.0.0.0:8789"}}),
        json!({"bridge":{"enabled":true,"address":listener.local_addr().unwrap().to_string()}}),
        json!({"storageDirectory":"relative-path"}),
        json!({"scriptPath":"relative-script.rhai"}),
        json!({"timelineDirectory":"relative-timelines"}),
    ] {
        let bytes = serde_json::to_vec(&config).unwrap();
        let (mut handle, mut needed) = (ptr::null_mut(), 0);
        let mut error = [0; 4096];
        assert_eq!(
            unsafe {
                kitu_application_create(
                    1,
                    bytes.as_ptr(),
                    bytes.len(),
                    &mut handle,
                    error.as_mut_ptr(),
                    error.len(),
                    &mut needed,
                )
            },
            DRIVER_ERROR
        );
        assert!(handle.is_null());
        assert!(needed > 0);
    }
}

#[test]
fn native_lifetime_is_safe_inside_an_existing_tokio_context() {
    let outer = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    outer.block_on(async {
        let native = Native::create(json!({"bridge":{"enabled":true,"address":"127.0.0.1:0"}}));
        native.tick();
        drop(native);
    });
}

#[test]
fn observer_disconnect_cannot_pause_native_game_and_destroy_closes_remaining_observers() {
    let native = Native::create(json!({"bridge":{"enabled":true,"address":"127.0.0.1:0"}}));
    let host = native.metadata();
    let endpoint = host["bridgeEndpoint"].as_str().unwrap();
    let ws_url = format!("{}/ws/runtime", endpoint.replace("http://", "ws://"));
    let (mut observer, _) = tungstenite::connect(&ws_url).unwrap();
    assert_eq!(native.command("/input/arena/start", 1), OK);
    native.tick();
    observer.close(None).unwrap();
    drop(observer);
    for _ in 0..10 {
        native.tick();
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(native.state()["overlay"], "none");
    let (mut observer, _) = tungstenite::connect(&ws_url).unwrap();
    // Keep an attached websocket alive across destruction. Its server task must
    // observe shutdown rather than holding the unload indefinitely.
    if let tungstenite::stream::MaybeTlsStream::Plain(stream) = observer.get_mut() {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
    }
    for _ in 0..600 {
        native.tick();
        thread::sleep(Duration::from_millis(1));
    }
    drop(native);
    while let Ok(message) = observer.read() {
        if message.is_close() {
            return;
        }
    }
    assert!(std::net::TcpStream::connect(endpoint.trim_start_matches("http://")).is_err());
}

#[test]
fn destroy_cancels_incomplete_http_requests_before_unloading_the_library() {
    use std::io::{Read, Write};
    let native = Native::create(json!({"bridge":{"enabled":true,"address":"127.0.0.1:0"}}));
    let host = native.metadata();
    let endpoint = host["bridgeEndpoint"]
        .as_str()
        .unwrap()
        .trim_start_matches("http://");
    let mut partial_body = std::net::TcpStream::connect(endpoint).unwrap();
    partial_body
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    partial_body.write_all(b"POST /arena/recordings/import HTTP/1.1\r\nHost: localhost\r\nContent-Length: 999999\r\n\r\nx").unwrap();
    let mut partial_header = std::net::TcpStream::connect(endpoint).unwrap();
    partial_header
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    partial_header
        .write_all(b"POST /shell/execute HTTP/1.1\r\nHost:")
        .unwrap();
    thread::sleep(Duration::from_millis(20));
    let started = Instant::now();
    drop(native);
    assert!(started.elapsed() < Duration::from_secs(2));
    for mut socket in [partial_body, partial_header] {
        let mut bytes = [0; 1024];
        loop {
            match socket.read(&mut bytes) {
                Ok(0) => break,
                Ok(_) => continue,
                Err(error) => {
                    assert!(
                        !matches!(
                            error.kind(),
                            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                        ),
                        "HTTP connection survived native destruction"
                    );
                    break;
                }
            }
        }
    }
    assert!(std::net::TcpStream::connect(endpoint).is_err());
}
