//! Real JSON/MessagePack WebSockets and the exported C ABI share one logical
//! input stream and explicit ticks. Socket I/O never owns a simulation timer.
#[allow(dead_code)]
#[path = "../../tests/support/mod.rs"]
mod support;

use kitu_demo_game::{
    arena, build_arena_runtime, build_demo_runtime,
    host::{arena_wire, ArenaHost, HostOptions},
    DemoRuntime,
};
use kitu_demo_game_native::*;
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_transport::{
    application::{
        ClientFrame, ClientHello, Codec, Encoding, InputFrame, InputMetadata, Role, ServerFrame,
        ServerPayload, SnapshotReason, NETWORK_INPUT_LIMITS, NETWORK_OUTPUT_LIMITS,
    },
    wire::WireBundle,
};
use kitu_unity_ffi::application::{ApplicationHandle, BUFFER_TOO_SMALL, OK};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    net::TcpStream,
    path::Path,
    ptr,
    time::Duration,
};
use tokio::runtime::Runtime;
use tungstenite::{client::IntoClientRequest, stream::MaybeTlsStream, Message, WebSocket};

const CONTROLLER: &str = "wire-parity-操作";
type Reader = unsafe extern "C" fn(*mut ApplicationHandle, *mut u8, usize, *mut usize) -> i32;

struct Native(*mut ApplicationHandle);
impl Native {
    fn new(setup: &Setup) -> Self {
        let config = serde_json::to_vec(&json!({
            "content": setup.content, "script": setup.script, "timeline": setup.timeline,
        }))
        .unwrap();
        let (mut handle, mut needed) = (ptr::null_mut(), 0);
        let mut diagnostic = [0; 4096];
        // Every ABI operation, including destruction, stays on this test thread.
        let result = unsafe {
            kitu_application_create(
                1,
                config.as_ptr(),
                config.len(),
                &mut handle,
                diagnostic.as_mut_ptr(),
                diagnostic.len(),
                &mut needed,
            )
        };
        assert_eq!(
            result,
            OK,
            "{}",
            String::from_utf8_lossy(&diagnostic[..needed.min(diagnostic.len())])
        );
        assert!(!handle.is_null());
        Self(handle)
    }
    fn read(&self, reader: Reader) -> Vec<WireBundle> {
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
    fn submit(&self, input: &InputFrame, sequence: u64) {
        let bytes = serde_json::to_vec(input).unwrap();
        assert!(
            bytes.len() <= NETWORK_INPUT_LIMITS.max_bytes,
            "fixtures use the common network/FFI domain"
        );
        let mut actual = u64::MAX;
        assert_eq!(
            unsafe {
                kitu_application_submit_json(self.0, bytes.as_ptr(), bytes.len(), &mut actual)
            },
            OK
        );
        assert_eq!(actual, sequence);
    }
    fn tick(&self) -> Vec<WireBundle> {
        assert_eq!(unsafe { kitu_application_tick(self.0) }, OK);
        self.read(kitu_application_read_output)
    }
}
impl Drop for Native {
    fn drop(&mut self) {
        let code = unsafe { kitu_application_destroy(self.0) };
        if !std::thread::panicking() {
            assert_eq!(code, OK);
        }
    }
}

#[derive(Clone)]
struct Setup {
    content: arena::config::ContentVersion,
    script: arena::script::ScriptVersion,
    timeline: arena::presentation::TimelineVersion,
}
impl Setup {
    fn stock() -> Self {
        let runtime = build_arena_runtime().unwrap();
        Self {
            content: arena::inspect_content(&runtime).unwrap().pending,
            script: arena::inspect_script(&runtime).unwrap().pending,
            timeline: arena::inspect_timeline(&runtime).unwrap().pending,
        }
    }
    fn edited() -> Self {
        let mut setup = Self::stock();
        setup.script = arena::script::ScriptVersion::from_source(
            &setup
                .script
                .source
                .replace("duration: 0.8", "duration: 1.6"),
        )
        .unwrap();
        let mut boss =
            kitu_tsq1::presentation::Clip::decode(&setup.timeline.clips[0].bytes).unwrap();
        for event in &mut boss.events {
            event.bundle.messages[0].args[0] = OscArg::Float(4.5);
        }
        let mut floor =
            kitu_tsq1::presentation::Clip::decode(&setup.timeline.clips[1].bytes).unwrap();
        floor
            .events
            .iter_mut()
            .find(|event| event.offset_tick == 12)
            .unwrap()
            .bundle
            .messages[0]
            .args[0] = OscArg::Float(0.85);
        setup.timeline = arena::presentation::TimelineVersion::from_sources(
            &boss.encode().unwrap(),
            &floor.encode().unwrap(),
        )
        .unwrap();
        setup
    }
    fn runtime(&self) -> DemoRuntime {
        let mut runtime = build_demo_runtime().unwrap();
        arena::install_with_all_versions(
            &mut runtime,
            self.content.clone(),
            self.script.clone(),
            self.timeline.clone(),
        )
        .unwrap();
        runtime
    }
}

struct Peer {
    socket: WebSocket<MaybeTlsStream<TcpStream>>,
    encoding: Encoding,
    input: Codec,
    output: Codec,
    next_delivery: u64,
    session: String,
}
impl Peer {
    fn connect(url: &str, encoding: Encoding, session: Option<String>) -> (Self, Vec<WireBundle>) {
        let protocol = match encoding {
            Encoding::Json => "kitu-arena-json-v1",
            Encoding::MessagePack => "kitu-arena-msgpack-v1",
        };
        let mut request = url.into_client_request().unwrap();
        request
            .headers_mut()
            .insert("Sec-WebSocket-Protocol", protocol.parse().unwrap());
        let (mut socket, response) = tungstenite::connect(request).unwrap();
        assert_eq!(response.headers()["Sec-WebSocket-Protocol"], protocol);
        let MaybeTlsStream::Plain(stream) = socket.get_mut() else {
            panic!("local plain socket")
        };
        stream.set_nodelay(true).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut peer = Self {
            socket,
            encoding,
            input: Codec::new(encoding, NETWORK_INPUT_LIMITS).unwrap(),
            output: Codec::new(encoding, NETWORK_OUTPUT_LIMITS).unwrap(),
            next_delivery: 0,
            session: String::new(),
        };
        peer.send(&ClientFrame::Hello(ClientHello {
            compatibility: arena_wire::compatibility(),
            client_id: CONTROLLER.into(),
            role: Role::Controller,
            expected_session_id: session,
        }));
        let ServerPayload::Hello(hello) = peer.receive().frame else {
            panic!("expected Hello")
        };
        assert_eq!(hello.compatibility, arena_wire::compatibility());
        assert_eq!(hello.execution, arena_wire::execution());
        assert_eq!(hello.role, Role::Controller);
        assert!(!hello.status.read_only);
        peer.session = hello.session_id;
        let ServerPayload::Snapshot(snapshot) = peer.receive().frame else {
            panic!("expected Initial Snapshot")
        };
        assert_eq!(snapshot.reason, SnapshotReason::Initial);
        assert_eq!(snapshot.batch.tick, snapshot.status.playback_mode.tick);
        (peer, snapshot.batch.bundles)
    }
    fn send(&mut self, frame: &ClientFrame) {
        let bytes = self.input.encode_client(frame).unwrap();
        let message = match self.encoding {
            Encoding::Json => Message::Text(String::from_utf8(bytes).unwrap().into()),
            Encoding::MessagePack => Message::Binary(bytes.into()),
        };
        self.socket.send(message).unwrap();
    }
    fn receive(&mut self) -> ServerFrame {
        let message = self.socket.read().unwrap();
        let bytes = match (self.encoding, message) {
            (Encoding::Json, Message::Text(text)) => text.as_bytes().to_vec(),
            (Encoding::MessagePack, Message::Binary(bytes)) => bytes.to_vec(),
            (_, other) => panic!("wrong frame encoding: {other:?}"),
        };
        let frame = self.output.decode_server(&bytes).unwrap();
        assert_eq!(
            frame.delivery_sequence, self.next_delivery,
            "socket delivery is independent of game tick"
        );
        self.next_delivery = self.next_delivery.checked_add(1).unwrap();
        frame
    }
    fn admitted(&mut self, tick: u64) {
        // The server processes preceding input frames synchronously before Ping.
        // Pong is a transport barrier, never a fabricated gameplay receipt.
        let nonce = tick.to_be_bytes().to_vec();
        self.socket
            .send(Message::Ping(nonce.clone().into()))
            .unwrap();
        match self.socket.read().unwrap() {
            Message::Pong(actual) => assert_eq!(actual.as_ref(), nonce),
            other => panic!("unexpected output before the owner tick: {other:?}"),
        }
    }
    fn close(mut self) -> String {
        self.socket.close(None).unwrap();
        loop {
            match self.socket.read() {
                Ok(Message::Close(_)) => {}
                Err(tungstenite::Error::ConnectionClosed | tungstenite::Error::AlreadyClosed) => {
                    break
                }
                other => panic!("unexpected close response: {other:?}"),
            }
        }
        self.session
    }
}

struct Network {
    host: ArenaHost,
    peer: Option<Peer>,
    url: String,
    encoding: Encoding,
    server: tokio::task::JoinHandle<()>,
    connections: usize,
}
impl Network {
    fn new(io: &Runtime, setup: &Setup, encoding: Encoding) -> Self {
        let host = ArenaHost::new(setup.runtime(), HostOptions::default()).unwrap();
        let router = host.router();
        let listener = io
            .block_on(tokio::net::TcpListener::bind("127.0.0.1:0"))
            .unwrap();
        let url = format!("ws://{}/ws/arena", listener.local_addr().unwrap());
        let server = io.spawn(async move { axum::serve(listener, router).await.unwrap() });
        let (peer, initial) = Peer::connect(&url, encoding, None);
        assert_bundles(
            &initial,
            &wire(&host.inspect().unwrap()),
            "initial socket projection",
        );
        Self {
            host,
            peer: Some(peer),
            url,
            encoding,
            server,
            connections: 1,
        }
    }
    fn reconnect(&mut self) {
        let session = self.peer.take().unwrap().close();
        let (peer, initial) = Peer::connect(&self.url, self.encoding, Some(session));
        assert_bundles(
            &initial,
            &wire(&self.host.inspect().unwrap()),
            "reconnect projection does not tick",
        );
        self.peer = Some(peer);
        self.connections += 1;
    }
    fn tick(&mut self, tick: u64) -> Vec<WireBundle> {
        let returned = wire(&self.host.tick().unwrap());
        let ServerPayload::Output(output) = self.peer.as_mut().unwrap().receive().frame else {
            panic!("expected one complete Output")
        };
        assert_eq!(output.batch.tick, tick as i64);
        assert_eq!(output.batch.tick, output.status.playback_mode.tick);
        assert!(!output.status.read_only);
        assert_bundles(
            &output.batch.bundles,
            &returned,
            "socket batch equals owner result",
        );
        returned
    }
}
impl Drop for Network {
    fn drop(&mut self) {
        self.peer.take();
        self.host.begin_shutdown();
        self.server.abort();
    }
}

fn wire(bundles: &[OscBundle]) -> Vec<WireBundle> {
    bundles
        .iter()
        .map(|bundle| WireBundle::try_from(bundle).unwrap())
        .collect()
}
fn assert_bundles(actual: &[WireBundle], expected: &[WireBundle], label: &str) {
    // Canonical JSON bytes also distinguish f32 negative zero, unlike PartialEq.
    assert_eq!(
        serde_json::to_vec(actual).unwrap(),
        serde_json::to_vec(expected).unwrap(),
        "{label}"
    );
}
fn projection(bundles: &[WireBundle], address: &str) -> Value {
    let message = bundles
        .iter()
        .flat_map(|bundle| &bundle.messages)
        .find(|message| message.address == address)
        .unwrap();
    let [kitu_transport::wire::WireArg::Str(value)] = message.args.as_slice() else {
        panic!("JSON projection")
    };
    serde_json::from_str(value).unwrap()
}
fn receipts(bundles: &[WireBundle]) -> Vec<Value> {
    bundles
        .iter()
        .flat_map(|bundle| &bundle.messages)
        .filter(|message| message.address == "/ui/arena/command")
        .map(|message| {
            let kitu_transport::wire::WireArg::Str(value) = &message.args[0] else {
                panic!("receipt")
            };
            serde_json::from_str(value).unwrap()
        })
        .collect()
}
fn input(address: &str, args: Vec<OscArg>, id: u64) -> InputFrame {
    InputFrame {
        metadata: Some(InputMetadata {
            source: CONTROLLER.into(),
            message_id: id,
            schema_version: 1,
        }),
        bundle: WireBundle::try_from(&OscBundle {
            messages: vec![OscMessage {
                address: address.into(),
                args,
            }],
        })
        .unwrap(),
    }
}
fn hex(bytes: impl AsRef<[u8]>) -> String {
    use std::fmt::Write;
    let mut value = String::with_capacity(bytes.as_ref().len() * 2);
    for byte in bytes.as_ref() {
        write!(&mut value, "{byte:02x}").unwrap();
    }
    value
}

fn hash_part(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_le_bytes());
    hasher.update(bytes);
}

struct Harness {
    json: Network,
    msgpack: Network,
    native: Native,
    tick: u64,
    sequence: u64,
    inputs_hash: Sha256,
    committed_hash: Sha256,
    last_input_tick: u64,
    next_input_order: u32,
    output_hash: Sha256,
    state_hash: Sha256,
    receipt_count: usize,
    setup: Setup,
}
impl Harness {
    fn new(io: &Runtime, setup: Setup) -> Self {
        let native = Native::new(&setup);
        let json = Network::new(io, &setup, Encoding::Json);
        let msgpack = Network::new(io, &setup, Encoding::MessagePack);
        let metadata = projection(
            &native.read(kitu_application_inspect_host_json),
            "/host/arena/status",
        );
        assert_eq!(
            metadata["compatibility"],
            serde_json::to_value(arena_wire::compatibility()).unwrap()
        );
        assert_eq!(
            metadata["execution"],
            serde_json::to_value(arena_wire::execution()).unwrap()
        );
        let initial = native.read(kitu_application_inspect_json);
        assert_bundles(
            &initial,
            &wire(&json.host.inspect().unwrap()),
            "initial native/JSON state",
        );
        assert_bundles(
            &initial,
            &wire(&msgpack.host.inspect().unwrap()),
            "initial native/MessagePack state",
        );
        Self {
            json,
            msgpack,
            native,
            tick: 0,
            sequence: 0,
            inputs_hash: Sha256::new(),
            committed_hash: Sha256::new(),
            last_input_tick: 0,
            next_input_order: 0,
            output_hash: Sha256::new(),
            state_hash: Sha256::new(),
            receipt_count: 0,
            setup,
        }
    }
    fn advance(&mut self, inputs: &[InputFrame], disconnect: bool) -> Vec<WireBundle> {
        if disconnect {
            self.json.reconnect();
            self.msgpack.reconnect();
        }
        for network in [&mut self.json, &mut self.msgpack] {
            let peer = network.peer.as_mut().unwrap();
            for input in inputs {
                peer.send(&ClientFrame::Input(input.clone()));
            }
            peer.admitted(self.tick);
        }
        let json = self.json.tick(self.tick);
        let msgpack = self.msgpack.tick(self.tick);
        assert_bundles(
            &msgpack,
            &json,
            "complete JSON/MessagePack output, order, and scalar widths",
        );
        if disconnect {
            // Derive the exact native equivalent from the real host's receipt,
            // rather than assuming a connection id or injecting it over WS.
            let emitted = receipts(&json);
            let receipt = emitted
                .iter()
                .find(|r| r["source"] == "host:controller")
                .expect("actual socket release receipt");
            assert_eq!(receipt["accepted"], true);
            let mut control = input(
                "/input/arena/disconnect",
                vec![],
                receipt["id"].as_u64().unwrap(),
            );
            control.metadata.as_mut().unwrap().source = receipt["source"].as_str().unwrap().into();
            self.submit_native(&control);
        }
        for input in inputs {
            self.submit_native(input);
        }
        let native = self.native.tick();
        assert_bundles(
            &native,
            &json,
            "complete socket/C ABI output, receipts and events",
        );
        let inspected = self.native.read(kitu_application_inspect_json);
        assert_bundles(
            &inspected,
            &wire(&self.json.host.inspect().unwrap()),
            "complete native/JSON inspection",
        );
        assert_bundles(
            &inspected,
            &wire(&self.msgpack.host.inspect().unwrap()),
            "complete native/MessagePack inspection",
        );
        let state = projection(&inspected, "/ui/arena/state");
        let cue = projection(&inspected, "/render/arena/presentation");
        assert_eq!(state["tick"], self.tick);
        assert_eq!(state["tick"], cue["tick"]);
        assert_eq!(state["simulationSteps"], cue["simulationStep"]);
        hash_part(&mut self.output_hash, &serde_json::to_vec(&json).unwrap());
        hash_part(
            &mut self.state_hash,
            &serde_json::to_vec(&inspected).unwrap(),
        );
        self.receipt_count += receipts(&json).len();
        self.tick += 1;
        json
    }
    fn submit_native(&mut self, input: &InputFrame) {
        self.native.submit(input, self.sequence);
        hash_part(&mut self.inputs_hash, &serde_json::to_vec(input).unwrap());
        if self.last_input_tick != self.tick {
            self.last_input_tick = self.tick;
            self.next_input_order = 0;
        }
        hash_part(
            &mut self.committed_hash,
            &serde_json::to_vec(&(self.tick, self.sequence, self.next_input_order, input)).unwrap(),
        );
        self.next_input_order += 1;
        self.sequence += 1;
    }
    fn verify_recorded_inputs(&self, name: &str) -> String {
        let mut recordings = Vec::new();
        for network in [&self.json, &self.msgpack] {
            let url = network
                .url
                .replace("ws://", "http://")
                .replace("/ws/arena", "/arena/recording/export");
            let bytes = ureq::get(&url)
                .call()
                .unwrap()
                .body_mut()
                .with_config()
                .limit(64 * 1024 * 1024)
                .read_to_vec()
                .unwrap();
            let recording = kitu_tsq1::recording::Recording::decode(&bytes).unwrap();
            assert_eq!(recording.entries.len() as u64, self.sequence);
            assert_eq!(recording.manifest["ticks"], self.tick);
            let mut committed = Sha256::new();
            for entry in &recording.entries {
                let input = InputFrame {
                    metadata: serde_json::from_value(entry.metadata["identity"].clone()).unwrap(),
                    bundle: WireBundle::try_from(&entry.bundle).unwrap(),
                };
                hash_part(
                    &mut committed,
                    &serde_json::to_vec(&(
                        entry.tick,
                        entry.metadata["sequence"].as_u64().unwrap(),
                        entry.order,
                        input,
                    ))
                    .unwrap(),
                );
            }
            assert_eq!(committed.finalize(), self.committed_hash.clone().finalize(), "actual socket admission preserves exact input bits, ids, tick and order submitted to native");
            recordings.push(bytes);
        }
        assert_eq!(
            recordings[0], recordings[1],
            "actual JSON and MessagePack hosts save identical TSQ1 bytes"
        );
        if let Some(directory) = std::env::var_os("KITU_WIRE_EVIDENCE_DIR") {
            std::fs::create_dir_all(&directory).unwrap();
            std::fs::write(
                Path::new(&directory).join(format!("{name}.tsq")),
                &recordings[0],
            )
            .unwrap();
        }
        hex(Sha256::digest(&recordings[0]))
    }
    fn evidence(&self, name: &str, extra: Value) {
        let recording_hash = self.verify_recorded_inputs(name);
        if let Some(directory) = std::env::var_os("KITU_WIRE_EVIDENCE_DIR") {
            std::fs::create_dir_all(&directory).unwrap();
            let proof = json!({
                "version":1, "scenario":name, "matched":true,
                "paths":["actual-json-websocket","actual-msgpack-websocket","exported-c-abi"],
                "clock":"explicit owner ticks; I/O runtime has no scheduler",
                "execution":arena_wire::execution(), "compatibility":arena_wire::compatibility(),
                "ticks":self.tick,"inputs":self.sequence,"receipts":self.receipt_count,
                "jsonConnections":self.json.connections,"msgpackConnections":self.msgpack.connections,
                "inputSha256":hex(self.inputs_hash.clone().finalize()),
                "committedInputSha256":hex(self.committed_hash.clone().finalize()),
                "identicalJsonMessagePackRecordingSha256":recording_hash,
                "outputSha256":hex(self.output_hash.clone().finalize()),
                "projectionSha256":hex(self.state_hash.clone().finalize()),
                "hashFraming":"SHA256 of little-endian u64 length followed by canonical JSON per input/tick",
                "initialContentHash":self.setup.content.hash,"initialScriptHash":self.setup.script.hash,
                "initialTimelineHash":self.setup.timeline.hash,"checks":extra,
            });
            std::fs::write(
                Path::new(&directory).join(format!("{name}.json")),
                serde_json::to_vec_pretty(&proof).unwrap(),
            )
            .unwrap();
        }
    }
}

#[derive(Default)]
struct ControllerIds {
    original: BTreeMap<(String, u64), u64>,
    next: u64,
}
impl ControllerIds {
    fn fresh(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
    fn translate(&mut self, source: &InputMetadata) -> u64 {
        let key = (source.source.clone(), source.message_id);
        if let Some(id) = self.original.get(&key) {
            return *id;
        }
        let id = self.fresh();
        self.original.insert(key, id);
        id
    }
    fn group(&mut self, runtime: &DemoRuntime) -> (Vec<InputFrame>, bool) {
        let mut disconnect = false;
        let mut group = Vec::new();
        for record in runtime.committed_input_records() {
            if record.bundle.messages[0].address == "/input/arena/disconnect" {
                assert!(
                    group.is_empty(),
                    "the fixture disconnect precedes the frame"
                );
                disconnect = true;
                continue;
            }
            let id = self.translate(record.metadata.as_ref().unwrap());
            group.push(InputFrame {
                metadata: Some(InputMetadata {
                    source: CONTROLLER.into(),
                    message_id: id,
                    schema_version: 1,
                }),
                bundle: WireBundle::try_from(&record.bundle).unwrap(),
            });
        }
        (group, disconnect)
    }
}

fn io_runtime() -> Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
}
fn reference_directory(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../kitu-integration-runner/scenarios/arena/reference")
        .join(name)
}
fn reference_receipts(outputs: &[OscBundle]) -> Vec<Value> {
    // The fixture's three synthetic producers become one real controller. Only
    // identity fields differ from the oracle; every result/tick/order stays checked.
    receipts(&wire(outputs))
        .into_iter()
        .map(strip_identity)
        .collect()
}
fn strip_identity(mut receipt: Value) -> Value {
    for field in ["source", "id", "sequence"] {
        receipt.as_object_mut().unwrap().remove(field);
    }
    receipt
}
fn frozen_scenario(name: &str, expected_ticks: u64, expected_inputs: u64) {
    let io = io_runtime();
    let mut harness = Harness::new(&io, Setup::stock());
    let mut ids = ControllerIds::default();
    let mut death = false;
    let mut retry = false;
    let counts = support::replay_reference_from_directory(
        &reference_directory(name),
        name,
        usize::MAX,
        |_| {},
        |runtime, expected| {
            let (inputs, disconnect) = ids.group(runtime);
            let actual = harness.advance(&inputs, disconnect);
            let state = projection(&actual, "/ui/arena/state");
            support::compare(
                &support::projection(runtime),
                &state,
                "frozen state through sockets and native",
            );
            assert_eq!(
                receipts(&actual)
                    .into_iter()
                    .map(strip_identity)
                    .collect::<Vec<_>>(),
                reference_receipts(expected)
            );
            if name == "stock-eleven-death-retry" && state["tick"] == 5526 {
                assert_eq!(state["floor"], 11);
                assert_eq!(state["phase"], 5);
                assert_eq!(state["inventory"]["health"], 0);
                death = true;
            }
            if name == "stock-eleven-death-retry" && state["tick"] == 5527 {
                assert_eq!(state["floor"], 0);
                assert_eq!(state["phase"], 1);
                assert_eq!(state["inventory"]["health"], 100);
                retry = true;
            }
        },
    );
    assert_eq!(
        (harness.tick, harness.sequence),
        (expected_ticks, expected_inputs)
    );
    if name == "stock-eleven-death-retry" {
        assert!(death && retry);
    }
    harness.evidence(name, json!({"frozenCheckpoints":counts.0,"frozenCommandOutcomes":counts.1,"allTickStatesCompared":true,"death11F":death,"retry":retry,"disconnectReconnect":name=="preparation"}));
}

#[test]
fn preparation_has_identical_json_msgpack_and_c_abi_outputs_with_real_reconnect() {
    frozen_scenario("preparation", 28, 40);
}
#[test]
fn stock_eleven_floor_death_and_retry_match_json_msgpack_and_c_abi() {
    frozen_scenario("stock-eleven-death-retry", 5528, 5581);
}

#[test]
fn edited_initial_rhai_and_tsq_cues_pause_and_advance_identically_across_transports() {
    let io = io_runtime();
    let mut harness = Harness::new(&io, Setup::edited());
    let mut ids = ControllerIds::default();
    let mut paused = false;
    let mut floor_seen = false;
    let mut telegraph_steps = BTreeSet::new();
    support::replay_reference_from_directory(
        &reference_directory("stock-eleven-death-retry"),
        "stock-eleven-death-retry",
        1800,
        |_| {},
        |runtime, _| {
            let (inputs, disconnect) = ids.group(runtime);
            assert!(!disconnect);
            let output = harness.advance(&inputs, false);
            let state = projection(&output, "/ui/arena/state");
            let cue = projection(&output, "/render/arena/presentation");
            if cue["floor"]["offsetTick"] == 12 {
                assert_eq!(cue["floor"]["opacity"], 0.85);
                floor_seen = true;
            }
            if let Some(boss) = cue["bosses"].as_array().unwrap().first() {
                assert_eq!(boss["radius"], 4.5);
                telegraph_steps.insert(cue["simulationStep"].as_u64().unwrap());
                if boss["offsetTick"] == 0 {
                    let enemy = state["enemies"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .find(|enemy| enemy["Id"] == boss["entityId"])
                        .unwrap();
                    assert_eq!(enemy["PhaseRemaining"], 1.6);
                }
                if !paused && boss["offsetTick"] == 24 {
                    let pause = input("/input/arena/pause", vec![], ids.fresh());
                    for index in 0..30 {
                        let output = harness.advance(
                            if index == 0 {
                                std::slice::from_ref(&pause)
                            } else {
                                &[]
                            },
                            false,
                        );
                        let frozen = projection(&output, "/render/arena/presentation");
                        assert_eq!(frozen["simulationStep"], cue["simulationStep"]);
                        assert_eq!(frozen["bosses"], cue["bosses"]);
                        assert_eq!(frozen["floor"], cue["floor"]);
                    }
                    // Resume joins the next original input group, preserving the
                    // fixture's next simulation input without an extra game step.
                    let resume = input("/input/arena/resume", vec![], ids.fresh());
                    for network in [&mut harness.json, &mut harness.msgpack] {
                        network
                            .peer
                            .as_mut()
                            .unwrap()
                            .send(&ClientFrame::Input(resume.clone()));
                    }
                    harness.submit_native(&resume);
                    paused = true;
                }
            }
        },
    );
    assert!(paused && floor_seen);
    assert_eq!(
        telegraph_steps.len(),
        96,
        "edited 1.6-second Rhai telegraph"
    );
    assert_eq!(harness.tick, 1830);
    harness.evidence("edited-rhai-timeline", json!({"bossRadius":4.5,"floorPeakOpacity":0.85,"telegraphSimulationSteps":96,"pausedOwnerTicks":30,"frozenCueDuringPause":true}));
}

#[test]
fn wide_ids_duplicate_receipts_and_valid_arena_types_are_exact() {
    let io = io_runtime();
    let mut harness = Harness::new(&io, Setup::stock());
    let first = (1u64 << 53) + 1;
    harness.advance(&[input("/input/arena/start", vec![], first)], false);
    harness.advance(
        &[input(
            "/input/arena/frame",
            vec![
                OscArg::Float(-0.0),
                OscArg::Float(0.0),
                OscArg::Bool(true),
                OscArg::Float(0.0),
                OscArg::Float(1.0),
                OscArg::Bool(false),
                OscArg::Bool(false),
            ],
            first + 1,
        )],
        false,
    );
    let invalid_target = harness.advance(
        &[input(
            "/input/arena/use",
            vec![OscArg::Int(i32::MAX)],
            first + 2,
        )],
        false,
    );
    assert_eq!(receipts(&invalid_target)[0]["code"], "invalid_target");
    let pause = input("/input/arena/pause", vec![], u64::MAX);
    let output = harness.advance(
        &[
            pause.clone(),
            pause,
            input("/input/arena/resume", vec![], first + 3),
        ],
        false,
    );
    let results = receipts(&output);
    assert_eq!(results[0]["id"].as_u64(), Some(u64::MAX));
    assert_eq!(results[0]["accepted"], true);
    assert_eq!(results[1]["duplicate"], true);
    assert_eq!(results[1]["accepted"], true);
    assert_eq!(results[2]["code"], "id_conflict");
    assert_eq!(projection(&output, "/ui/arena/state")["overlay"], "pause");
    harness.evidence("wide-identities", json!({"firstInputId":first,"maximumInputId":u64::MAX,"duplicateKeepsOriginalReceipt":true,"olderIdRejected":true,"negativeZeroAndTypedArgumentsCompared":true}));
}
