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
    fn tick(&self) -> Value {
        assert_eq!(unsafe { kitu_application_tick(self.0) }, OK);
        self.read(kitu_application_read_output)
    }
    fn command(&self, address: &str, id: u64) -> i32 {
        let bytes = serde_json::to_vec(
            &json!({"metadata":{"source":"native-test","messageId":id,"schemaVersion":1},
            "bundle":{"messages":[{"address":address,"args":[]}]}}),
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
