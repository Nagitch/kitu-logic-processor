//! Real sockets verify fixed encoding, authority and owner-clock publication.
use futures_util::{SinkExt, StreamExt};
use kitu_demo_game::{
    build_arena_runtime,
    host::{arena_wire, ArenaHost, HostOptions},
    replay::Recorder,
};
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::InputMetadata;
use kitu_transport::application::*;
use serde_json::{json, Value};
use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};
use tokio_tungstenite::{
    connect_async,
    tungstenite::{client::IntoClientRequest, Message},
    MaybeTlsStream, WebSocketStream,
};

type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;
struct Host {
    host: ArenaHost,
    address: std::net::SocketAddr,
    server: tokio::task::JoinHandle<()>,
    directory: PathBuf,
}
impl Host {
    async fn new(external: bool) -> Self {
        // SystemTime can return the same value in concurrent test threads.
        static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);
        let directory = std::env::temp_dir().join(format!(
            "arena-wire-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed),
        ));
        std::fs::create_dir(&directory).unwrap();
        let host = ArenaHost::new(
            build_arena_runtime().unwrap(),
            HostOptions {
                external_controller: external,
                recording_directory: directory.clone(),
                io_runtime: Some(tokio::runtime::Handle::current()),
                ..HostOptions::default()
            },
        )
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let router = host.router();
        let server = tokio::spawn(async move {
            axum::serve(listener, router).await.unwrap();
        });
        Self {
            host,
            address,
            server,
            directory,
        }
    }
    async fn socket(&self, encoding: Encoding) -> Client {
        let mut request = format!("ws://{}/ws/arena", self.address)
            .into_client_request()
            .unwrap();
        let protocol = match encoding {
            Encoding::Json => arena_wire::JSON_PROTOCOL,
            Encoding::MessagePack => arena_wire::MESSAGEPACK_PROTOCOL,
        };
        request
            .headers_mut()
            .insert("sec-websocket-protocol", protocol.parse().unwrap());
        let (socket, response) = connect_async(request).await.unwrap();
        assert_eq!(response.headers()["sec-websocket-protocol"], protocol);
        Client {
            socket,
            encoding,
            sequence: 0,
            pending: VecDeque::new(),
        }
    }
    fn projection(&self) -> Value {
        projection(&self.host.inspect().unwrap(), "/ui/arena/state")
    }
    async fn finish(self) {
        self.host.begin_shutdown();
        tokio::time::timeout(Duration::from_secs(3), self.host.wait_shutdown())
            .await
            .unwrap();
        self.server.abort();
        let _ = self.server.await;
        std::fs::remove_dir_all(self.directory).unwrap();
    }
    async fn complete(&mut self, path: &str, body: Value) -> Value {
        let future = http(self.address, path, body);
        tokio::pin!(future);
        let deadline = tokio::time::sleep(Duration::from_secs(8));
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                response=&mut future => return response,
                _=&mut deadline => panic!("HTTP owner operation timed out"),
                _=tokio::time::sleep(Duration::from_millis(2)) => { self.host.tick().unwrap(); }
            }
        }
    }
}
struct Client {
    socket: Socket,
    encoding: Encoding,
    sequence: u64,
    pending: VecDeque<ServerFrame>,
}
impl Client {
    async fn send(&mut self, frame: ClientFrame) {
        let bytes = Codec::new(self.encoding, NETWORK_INPUT_LIMITS)
            .unwrap()
            .encode_client(&frame)
            .unwrap();
        let message = match self.encoding {
            Encoding::Json => Message::Text(String::from_utf8(bytes).unwrap().into()),
            Encoding::MessagePack => Message::Binary(bytes.into()),
        };
        self.socket.send(message).await.unwrap();
    }
    fn decode(&mut self, message: Message) -> ServerFrame {
        let bytes = match (self.encoding, message) {
            (Encoding::Json, Message::Text(text)) => text.as_bytes().to_vec(),
            (Encoding::MessagePack, Message::Binary(bytes)) => bytes.to_vec(),
            (_, other) => panic!("wrong frame encoding: {other:?}"),
        };
        let frame = Codec::new(self.encoding, NETWORK_OUTPUT_LIMITS)
            .unwrap()
            .decode_server(&bytes)
            .unwrap();
        assert_eq!(frame.delivery_sequence, self.sequence);
        self.sequence += 1;
        frame
    }
    async fn raw(&mut self) -> Message {
        tokio::time::timeout(Duration::from_secs(5), self.socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap()
    }
    async fn next(&mut self) -> ServerFrame {
        if let Some(frame) = self.pending.pop_front() {
            return frame;
        }
        loop {
            let message = self.raw().await;
            if matches!(message, Message::Pong(_)) {
                continue;
            }
            return self.decode(message);
        }
    }
    async fn barrier(&mut self) {
        self.socket
            .send(Message::Ping(vec![9, 3, 7].into()))
            .await
            .unwrap();
        loop {
            let message = self.raw().await;
            if matches!(&message,Message::Pong(bytes) if bytes.as_ref()==[9,3,7]) {
                break;
            }
            if matches!(message, Message::Pong(_)) {
                continue;
            }
            let frame = self.decode(message);
            self.pending.push_back(frame);
        }
    }
    async fn hello(
        &mut self,
        id: &str,
        role: Role,
        expected: Option<String>,
    ) -> (ServerHello, SnapshotFrame) {
        self.send(hello(id, role, expected)).await;
        let ServerPayload::Hello(hello) = self.next().await.frame else {
            panic!("missing hello")
        };
        let ServerPayload::Snapshot(snapshot) = self.next().await.frame else {
            panic!("missing snapshot")
        };
        assert_eq!(snapshot.reason, SnapshotReason::Initial);
        assert_eq!(hello.status, snapshot.status);
        assert_eq!(snapshot.batch.tick, snapshot.status.playback_mode.tick);
        (hello, snapshot)
    }
    async fn error(&mut self, code: ErrorCode, fatal: bool) -> ErrorFrame {
        let ServerPayload::Error(error) = self.next().await.frame else {
            panic!("missing error")
        };
        assert_eq!(error.code, code);
        assert_eq!(error.fatal, fatal);
        error
    }
    async fn output(&mut self) -> OutputFrame {
        let ServerPayload::Output(output) = self.next().await.frame else {
            panic!("missing complete output")
        };
        assert_eq!(output.batch.tick, output.status.playback_mode.tick);
        output
    }
}
fn hello(id: &str, role: Role, expected: Option<String>) -> ClientFrame {
    ClientFrame::Hello(ClientHello {
        compatibility: arena_wire::compatibility(),
        client_id: id.into(),
        role,
        expected_session_id: expected,
    })
}
fn input(id: &str, n: u64, address: &str, args: Vec<OscArg>) -> ClientFrame {
    let bundle = OscBundle {
        messages: vec![OscMessage {
            address: address.into(),
            args,
        }],
    };
    ClientFrame::Input(InputFrame {
        metadata: Some(InputMetadata {
            source: id.into(),
            message_id: n,
            schema_version: 1,
        }),
        bundle: kitu_transport::wire::WireBundle::try_from(&bundle).unwrap(),
    })
}
fn bundles(wire: Vec<kitu_transport::wire::WireBundle>) -> Vec<OscBundle> {
    wire.into_iter().map(|b| b.try_into().unwrap()).collect()
}
fn projection(bundles: &[OscBundle], address: &str) -> Value {
    let message = bundles
        .iter()
        .flat_map(|b| &b.messages)
        .find(|m| m.address == address)
        .unwrap();
    let OscArg::Str(json) = &message.args[0] else {
        panic!("projection string")
    };
    serde_json::from_str(json).unwrap()
}
async fn http(address: std::net::SocketAddr, path: &str, body: Value) -> Value {
    let body = serde_json::to_vec(&body).unwrap();
    let mut stream = TcpStream::connect(address).await.unwrap();
    let header=format!("POST {path} HTTP/1.1\r\nHost: {address}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len());
    stream.write_all(header.as_bytes()).await.unwrap();
    stream.write_all(&body).await.unwrap();
    let mut response = Vec::new();
    stream.read_to_end(&mut response).await.unwrap();
    let start = response.windows(4).position(|w| w == b"\r\n\r\n").unwrap() + 4;
    assert!(
        response.starts_with(b"HTTP/1.1 200"),
        "{}",
        String::from_utf8_lossy(&response)
    );
    serde_json::from_slice(&response[start..]).unwrap()
}

#[tokio::test]
async fn incompatible_hello_has_no_controller_or_input_side_effect() {
    let mut host = Host::new(false).await;
    for encoding in [Encoding::Json, Encoding::MessagePack] {
        for field in 0..6 {
            let mut client = host.socket(encoding).await;
            let ClientFrame::Hello(mut request) = hello("bad", Role::Controller, None) else {
                unreachable!()
            };
            match field {
                0 => request.compatibility.app_id = "another-app".into(),
                1 => request.compatibility.wire_version += 1,
                2 => request.compatibility.schema_version += 1,
                3 => request.compatibility.presentation_version += 1,
                4 => request.compatibility.tick_rate = 30,
                _ => {
                    request.compatibility.features.remove(0);
                }
            }
            client.send(ClientFrame::Hello(request)).await;
            client.error(ErrorCode::Incompatible, true).await;
        }
    }
    assert_eq!(host.projection()["tick"], -1);
    let mut controller = host.socket(Encoding::Json).await;
    let (hello, _) = controller.hello("correct", Role::Controller, None).await;
    assert_eq!(hello.compatibility, arena_wire::compatibility());
    assert_eq!(hello.execution, arena_wire::execution());
    controller
        .send(input("correct", 1, "/input/arena/start", vec![]))
        .await;
    controller.barrier().await;
    let output = host.host.tick().unwrap();
    assert_eq!(projection(&output, "/ui/arena/state")["phase"], 1);
    assert_eq!(bundles(controller.output().await.batch.bundles), output);
    host.finish().await;
}

#[tokio::test]
async fn fixed_protocol_and_hello_order_are_strict() {
    let host = Host::new(false).await;
    for offered in [
        None,
        Some("unknown"),
        Some("kitu-arena-json-v1,kitu-arena-msgpack-v1"),
    ] {
        let mut request = format!("ws://{}/ws/arena", host.address)
            .into_client_request()
            .unwrap();
        if let Some(protocol) = offered {
            request
                .headers_mut()
                .insert("sec-websocket-protocol", protocol.parse().unwrap());
        }
        assert!(connect_async(request).await.is_err());
    }
    for encoding in [Encoding::Json, Encoding::MessagePack] {
        let mut client = host.socket(encoding).await;
        client
            .send(input("before-hello", 1, "/input/arena/start", vec![]))
            .await;
        client.error(ErrorCode::Protocol, true).await;
        let mut client = host.socket(encoding).await;
        let wrong = match encoding {
            Encoding::Json => Message::Binary(vec![0xc0].into()),
            Encoding::MessagePack => Message::Text("{}".into()),
        };
        client.socket.send(wrong).await.unwrap();
        client.error(ErrorCode::Protocol, true).await;
        let mut client = host.socket(encoding).await;
        client.hello("observer", Role::Observer, None).await;
        client.send(hello("observer", Role::Observer, None)).await;
        client.error(ErrorCode::Protocol, true).await;
    }
    let mut stale = host.socket(Encoding::Json).await;
    stale
        .send(hello(
            "controller",
            Role::Controller,
            Some("not-this-session".into()),
        ))
        .await;
    stale.error(ErrorCode::SessionChanged, true).await;
    let mut reserved = host.socket(Encoding::Json).await;
    reserved
        .send(hello("host:arena-timeline-admin", Role::Controller, None))
        .await;
    reserved.error(ErrorCode::Protocol, true).await;
    assert_eq!(host.projection()["tick"], -1);
    host.finish().await;
}

#[tokio::test]
async fn observer_native_owner_and_bound_producer_cannot_be_impersonated() {
    let mut host = Host::new(false).await;
    let mut controller = host.socket(Encoding::MessagePack).await;
    controller.hello("owner", Role::Controller, None).await;
    let mut second = host.socket(Encoding::Json).await;
    second.send(hello("owner", Role::Controller, None)).await;
    second.error(ErrorCode::ControllerBusy, true).await;
    let mut observer = host.socket(Encoding::Json).await;
    observer.hello("watch", Role::Observer, None).await;
    observer
        .send(input("watch", 1, "/input/arena/start", vec![]))
        .await;
    assert_eq!(
        observer.error(ErrorCode::ReadOnly, false).await.input_id,
        Some(1)
    );
    for producer in ["another-client", "host:arena-timeline-admin"] {
        controller
            .send(input(producer, u64::MAX, "/input/arena/start", vec![]))
            .await;
        assert_eq!(
            controller
                .error(ErrorCode::InvalidInput, false)
                .await
                .input_id,
            Some(u64::MAX)
        );
    }
    controller
        .send(input("owner", 1, "/input/arena/disconnect", vec![]))
        .await;
    controller.error(ErrorCode::InvalidInput, false).await;
    controller
        .send(input("owner", 1, "/input/arena/start", vec![]))
        .await;
    controller.barrier().await;
    observer.socket.close(None).await.unwrap();
    tokio::task::yield_now().await;
    let outputs = host.host.tick().unwrap();
    assert_eq!(projection(&outputs, "/ui/arena/state")["overlay"], "none");
    controller.output().await;
    host.finish().await;
    let host = Host::new(true).await;
    let mut controller = host.socket(Encoding::Json).await;
    controller
        .send(hello("network", Role::Controller, None))
        .await;
    controller.error(ErrorCode::ControllerBusy, true).await;
    let mut observer = host.socket(Encoding::MessagePack).await;
    observer.hello("network", Role::Observer, None).await;
    assert_eq!(host.projection()["tick"], -1);
    host.finish().await;
}

#[tokio::test]
async fn queue_pressure_does_not_create_a_hidden_clock_and_disconnect_requires_resume() {
    let mut host = Host::new(false).await;
    let mut client = host.socket(Encoding::Json).await;
    let (hello, _) = client.hello("persistent", Role::Controller, None).await;
    for id in 1..arena_wire::MAX_PENDING_INPUTS as u64 {
        client
            .send(input("persistent", id, "/input/arena/start", vec![]))
            .await;
    }
    client
        .send(input("persistent", 1000, "/input/arena/start", vec![]))
        .await;
    client.barrier().await;
    client.error(ErrorCode::QueueFull, false).await;
    assert_eq!(host.projection()["tick"], -1);
    host.host.tick().unwrap();
    client.output().await;
    assert_eq!(host.projection()["simulationSteps"], 1);
    client.socket.close(None).await.unwrap();
    for _ in 0..5 {
        tokio::task::yield_now().await;
    }
    let mut reconnected = host.socket(Encoding::MessagePack).await;
    reconnected
        .hello("persistent", Role::Controller, Some(hello.session_id))
        .await;
    host.host.tick().unwrap();
    reconnected.output().await;
    assert_eq!(host.projection()["overlay"], "pause");
    reconnected
        .send(input("persistent", 1, "/input/arena/start", vec![]))
        .await;
    reconnected.barrier().await;
    let output = host.host.tick().unwrap();
    reconnected.output().await;
    let receipt = projection(&output, "/ui/arena/command");
    assert_eq!(receipt["duplicate"], true);
    assert_eq!(receipt["id"], 1);
    assert_eq!(host.projection()["overlay"], "pause");
    reconnected
        .send(input("persistent", 1001, "/input/arena/resume", vec![]))
        .await;
    reconnected.barrier().await;
    host.host.tick().unwrap();
    reconnected.output().await;
    assert_eq!(host.projection()["overlay"], "none");
    host.finish().await;
}

#[tokio::test]
async fn lag_closes_only_the_slow_observer_and_reconnect_snapshots_are_coherent() {
    let mut host = Host::new(true).await;
    let mut slow = host.socket(Encoding::Json).await;
    slow.hello("slow", Role::Observer, None).await;
    host.host
        .submit(
            OscBundle {
                messages: vec![OscMessage::new("/input/arena/start")],
            },
            Some(InputMetadata {
                source: "native".into(),
                message_id: 1,
                schema_version: 1,
            }),
        )
        .unwrap();
    // Current-thread Tokio cannot drain the subscription while this owner batch
    // runs, deterministically overflowing the bounded ring through real sockets.
    for _ in 0..40 {
        host.host.tick().unwrap();
    }
    let error = slow.error(ErrorCode::Protocol, true).await;
    assert!(error.message.contains("reconnect"));
    assert_eq!(host.projection()["overlay"], "none");
    let mut next = host.socket(Encoding::MessagePack).await;
    let (_, snapshot) = next.hello("slow", Role::Observer, None).await;
    assert_eq!(snapshot.batch.tick, 39);
    assert_eq!(
        bundles(snapshot.batch.bundles),
        host.host.inspect().unwrap()
    );
    let output = host.host.tick().unwrap();
    let received = next.output().await;
    assert_eq!(received.batch.tick, 40);
    assert_eq!(bundles(received.batch.bundles), output);
    host.finish().await;
}

#[tokio::test]
async fn backward_seek_replaces_full_projection_without_rewinding_delivery_sequence() {
    let mut host = Host::new(false).await;
    let mut runtime = build_arena_runtime().unwrap();
    let mut recorder = Recorder::new(&runtime).unwrap();
    runtime
        .try_enqueue_input(
            OscBundle {
                messages: vec![OscMessage::new("/input/arena/start")],
            },
            Some(InputMetadata {
                source: "saved".into(),
                message_id: 1,
                schema_version: 1,
            }),
        )
        .unwrap();
    for _ in 0..12 {
        runtime.tick_once().unwrap();
        let output = runtime.drain_output_buffer();
        recorder.capture(&runtime, &output).unwrap();
    }
    let bytes = recorder.encode().unwrap();
    let id = hex::encode(sha2::Sha256::digest(&bytes));
    std::fs::write(host.directory.join(format!("{id}.tsq")), bytes).unwrap();
    let mut client = host.socket(Encoding::MessagePack).await;
    client.hello("operator", Role::Controller, None).await;
    host.complete("/arena/playback/load", json!({"id":id}))
        .await;
    host.host.tick().unwrap();
    // Discard prior live publications until the load's replacement boundary.
    loop {
        if let ServerPayload::Snapshot(snapshot) = client.next().await.frame {
            assert_eq!(snapshot.reason, SnapshotReason::Seek);
            assert_eq!(snapshot.batch.tick, -1);
            break;
        }
    }
    host.complete("/arena/playback/seek", json!({"tick":10}))
        .await;
    loop {
        if let ServerPayload::Snapshot(snapshot) = client.next().await.frame {
            assert_eq!(snapshot.batch.tick, 10);
            break;
        }
    }
    client
        .send(input("operator", 99, "/input/arena/start", vec![]))
        .await;
    // A pending paused output may precede this admission error.
    loop {
        if let ServerPayload::Error(error) = client.next().await.frame {
            assert_eq!(error.code, ErrorCode::ReadOnly);
            break;
        }
    }
    let before = client.sequence;
    host.complete("/arena/playback/seek", json!({"tick":2}))
        .await;
    loop {
        if let ServerPayload::Snapshot(snapshot) = client.next().await.frame {
            assert_eq!(snapshot.batch.tick, 2);
            assert_eq!(snapshot.status.playback_mode.tick, 2);
            assert_eq!(
                bundles(snapshot.batch.bundles),
                host.host.inspect().unwrap()
            );
            assert!(client.sequence > before);
            break;
        }
    }
    host.finish().await;
}
use sha2::Digest;

#[tokio::test]
async fn fragmented_frames_keep_one_input_and_snapshot_does_not_lose_racing_publications() {
    use tokio_tungstenite::tungstenite::protocol::frame::{
        coding::{Data, OpCode},
        Frame,
    };
    let mut host = Host::new(false).await;
    let mut client = host.socket(Encoding::Json).await;
    let bytes = Codec::new(Encoding::Json, NETWORK_INPUT_LIMITS)
        .unwrap()
        .encode_client(&hello("fragmented", Role::Controller, None))
        .unwrap();
    let middle = bytes.len() / 2;
    client
        .socket
        .send(Message::Frame(Frame::message(
            bytes[..middle].to_vec(),
            OpCode::Data(Data::Text),
            false,
        )))
        .await
        .unwrap();
    client
        .socket
        .send(Message::Frame(Frame::message(
            bytes[middle..].to_vec(),
            OpCode::Data(Data::Continue),
            true,
        )))
        .await
        .unwrap();
    let ServerPayload::Hello(hello) = client.next().await.frame else {
        panic!("missing Hello")
    };
    // The initial snapshot is already captured, but has not been consumed by
    // this client. Publications during its delivery must follow its watermark.
    let mut expected = Vec::new();
    for _ in 0..8 {
        expected.push(host.host.tick().unwrap());
    }
    let ServerPayload::Snapshot(snapshot) = client.next().await.frame else {
        panic!("missing snapshot")
    };
    assert_eq!(snapshot.status, hello.status);
    assert_eq!(snapshot.batch.tick, -1);
    for (tick, output) in expected.into_iter().enumerate() {
        let actual = client.output().await;
        assert_eq!(actual.batch.tick, tick as i64);
        assert_eq!(bundles(actual.batch.bundles), output);
    }
    let bytes = Codec::new(Encoding::Json, NETWORK_INPUT_LIMITS)
        .unwrap()
        .encode_client(&input("fragmented", 1, "/input/arena/start", vec![]))
        .unwrap();
    let middle = bytes.len() / 2;
    client
        .socket
        .send(Message::Frame(Frame::message(
            bytes[..middle].to_vec(),
            OpCode::Data(Data::Text),
            false,
        )))
        .await
        .unwrap();
    client
        .socket
        .send(Message::Frame(Frame::message(
            bytes[middle..].to_vec(),
            OpCode::Data(Data::Continue),
            true,
        )))
        .await
        .unwrap();
    client.barrier().await;
    let output = host.host.tick().unwrap();
    client.output().await;
    let receipts = output
        .iter()
        .flat_map(|b| &b.messages)
        .filter(|m| m.address == "/ui/arena/command")
        .count();
    assert_eq!(receipts, 1);
    assert_eq!(host.projection()["simulationSteps"], 1);
    host.finish().await;
}

#[tokio::test]
async fn malformed_and_oversized_hello_are_fatal_before_any_input_admission() {
    let host = Host::new(false).await;
    let mut client = host.socket(Encoding::Json).await;
    client
        .socket
        .send(Message::Text(
            "{\"type\":\"hello\",\"payload\":{},\"unexpected\":true}".into(),
        ))
        .await
        .unwrap();
    client.error(ErrorCode::Protocol, true).await;
    let mut client = host.socket(Encoding::MessagePack).await;
    client
        .socket
        .send(Message::Binary(vec![0xc1].into()))
        .await
        .unwrap();
    client.error(ErrorCode::Protocol, true).await;
    let mut client = host.socket(Encoding::Json).await;
    client
        .socket
        .send(Message::Text(
            " ".repeat(NETWORK_INPUT_LIMITS.max_bytes + 1).into(),
        ))
        .await
        .unwrap();
    client.error(ErrorCode::Protocol, true).await;
    // The bounded decoder leaves the oversized payload unread. Depending on
    // the TCP stack this can reset the socket after the fatal application error
    // rather than delivering the best-effort 1009 close frame.
    match tokio::time::timeout(Duration::from_secs(3), client.socket.next())
        .await
        .unwrap()
    {
        Some(Ok(Message::Close(Some(close)))) => assert_eq!(u16::from(close.code), 1009),
        Some(Err(tokio_tungstenite::tungstenite::Error::Io(error))) => {
            assert_eq!(error.kind(), std::io::ErrorKind::ConnectionReset)
        }
        other => panic!("oversized connection did not terminate: {other:?}"),
    }
    assert_eq!(host.projection()["tick"], -1);
    host.finish().await;
}

#[tokio::test]
async fn normal_close_is_acknowledged_before_controller_release_and_reconnect() {
    let mut host = Host::new(false).await;
    for encoding in [Encoding::Json, Encoding::MessagePack] {
        let mut client = host.socket(encoding).await;
        let (hello, _) = client.hello("close-owner", Role::Controller, None).await;
        client.socket.close(None).await.unwrap();
        let close = tokio::time::timeout(Duration::from_secs(3), client.socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(
            matches!(close, Message::Close(_)),
            "expected acknowledged close, received {close:?}"
        );
        let mut replacement = host.socket(encoding).await;
        replacement
            .hello("close-owner", Role::Controller, Some(hello.session_id))
            .await;
        host.host.tick().unwrap();
        replacement.output().await;
        replacement.socket.close(None).await.unwrap();
        let close = tokio::time::timeout(Duration::from_secs(3), replacement.socket.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert!(matches!(close, Message::Close(_)));
    }
    host.finish().await;
}
