//! Real loopback regressions for bounded response batches and mutation ownership.
use super::InternalWebSocketRelay;
use futures_util::{SinkExt, StreamExt};
use kitu_osc_ir::{OscArg, OscMessage};
use kitu_transport::{decode_kep_envelope, encode_kep_envelope, encode_osc_packet, KepEnvelope};
use std::{
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio::{net::TcpListener, task::JoinHandle, time::timeout};
use tokio_tungstenite::{accept_async, tungstenite::Message};

const FRAME_LIMIT: usize = 128;
const BYTE_LIMIT: usize = 1024 * 1024;

#[derive(Clone)]
enum Replies {
    Continuous,
    Burst(Vec<Vec<u8>>),
    DelayedFirst,
}

struct Backend {
    url: String,
    requests: Arc<Mutex<Vec<u64>>>,
    connections: Arc<AtomicUsize>,
    task: JoinHandle<()>,
}

impl Backend {
    async fn start(replies: Replies) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let requests = Arc::new(Mutex::new(Vec::new()));
        let connections = Arc::new(AtomicUsize::new(0));
        let observed_requests = requests.clone();
        let observed_connections = connections.clone();
        let task = tokio::spawn(async move {
            // The relay has one connection at a time. A reset closes this socket
            // before accepting an explicitly requested replacement connection.
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                observed_connections.fetch_add(1, Ordering::SeqCst);
                let mut socket = accept_async(stream).await.unwrap();
                let mut interval = tokio::time::interval(Duration::from_millis(16));
                interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                let mut latest_request = None;
                let mut tick = 0;
                loop {
                    tokio::select! {
                        incoming = socket.next() => {
                            let Some(Ok(Message::Binary(bytes))) = incoming else { break };
                            let envelope = decode_kep_envelope(&bytes).unwrap();
                            let id = envelope.correlation_id.unwrap();
                            observed_requests.lock().unwrap().push(id);
                            latest_request = Some(id);
                            if matches!(replies, Replies::Continuous) {
                                continue;
                            }
                            if id == 1 && matches!(replies, Replies::DelayedFirst) {
                                tokio::time::sleep(Duration::from_millis(350)).await;
                            }
                            let frames = match &replies {
                                Replies::Burst(frames) if id == 1 => frames.clone(),
                                _ => vec![response(id, 0)],
                            };
                            let mut failed = false;
                            for bytes in frames {
                                if socket.send(Message::Binary(bytes.into())).await.is_err() {
                                    failed = true;
                                    break;
                                }
                            }
                            if failed { break; }
                        }
                        _ = interval.tick(), if latest_request.is_some() && matches!(replies, Replies::Continuous) => {
                            if socket.send(Message::Binary(response(latest_request.unwrap(), tick).into())).await.is_err() {
                                break;
                            }
                            tick += 1;
                        }
                    }
                }
            }
        });
        Self {
            url,
            requests,
            connections,
            task,
        }
    }

    fn received(&self) -> Vec<u64> {
        self.requests.lock().unwrap().clone()
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.task.abort();
    }
}

fn request(id: u64) -> Vec<u8> {
    let mut message = OscMessage::new("/mutation");
    message.args.push(OscArg::Int(id as i32));
    let mut envelope = KepEnvelope::osc(encode_osc_packet(&message).unwrap());
    envelope.correlation_id = Some(id);
    encode_kep_envelope(&envelope).unwrap()
}

fn response(request: u64, tick: u64) -> Vec<u8> {
    encode_kep_envelope(&KepEnvelope::json(
        serde_json::to_vec(&serde_json::json!({"request": request, "tick": tick})).unwrap(),
    ))
    .unwrap()
}

fn sized_response(bytes: usize) -> Vec<u8> {
    // A valid JSON string with a bin32 KEP payload makes the encoded boundary
    // exact. The cap applies to whole encoded responses, including KEP metadata.
    let mut payload = vec![b'x'; bytes];
    payload[0] = b'"';
    *payload.last_mut().unwrap() = b'"';
    let overhead = encode_kep_envelope(&KepEnvelope::json(payload.clone()))
        .unwrap()
        .len()
        - payload.len();
    payload.truncate(bytes - overhead);
    *payload.last_mut().unwrap() = b'"';
    let encoded = encode_kep_envelope(&KepEnvelope::json(payload)).unwrap();
    assert_eq!(encoded.len(), bytes);
    encoded
}

fn json_response(bytes: &[u8]) -> serde_json::Value {
    serde_json::from_slice(&decode_kep_envelope(bytes).unwrap().payload).unwrap()
}

#[tokio::test]
async fn continuous_sixty_hz_updates_finish_and_a_second_request_progresses() {
    let backend = Backend::start(Replies::Continuous).await;
    let mut relay = InternalWebSocketRelay::new(backend.url.clone());
    for id in 1..=2 {
        // A quiet-gap timeout restarts every16ms and never completes this call.
        // The generous2s bound isolates that defect from a busy CI scheduler.
        let frames = timeout(
            Duration::from_secs(2),
            relay.relay_kep_envelope(request(id)),
        )
        .await
        .expect("continuous responses must not renew the batch deadline")
        .unwrap();
        assert!(!frames.is_empty());
        assert!(frames.len() <= FRAME_LIMIT);
        assert!(frames.iter().map(Vec::len).sum::<usize>() <= BYTE_LIMIT);
        assert!(frames
            .iter()
            .any(|bytes| json_response(bytes)["request"] == id));
    }
    assert_eq!(
        backend.received(),
        vec![1, 2],
        "each mutation is sent exactly once"
    );
    assert_eq!(backend.connections.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn a_burst_returns_only_the_first_128_response_frames() {
    let all = (0..256).map(|tick| response(1, tick)).collect::<Vec<_>>();
    let backend = Backend::start(Replies::Burst(all.clone())).await;
    let mut relay = InternalWebSocketRelay::new(backend.url.clone());
    let frames = timeout(Duration::from_secs(2), relay.relay_kep_envelope(request(1)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frames, all[..FRAME_LIMIT]);
    assert_eq!(backend.received(), vec![1]);
}

#[tokio::test]
async fn exactly_one_mebibyte_of_encoded_responses_is_accepted() {
    let half = sized_response(BYTE_LIMIT / 2);
    let backend = Backend::start(Replies::Burst(vec![half.clone(), half.clone()])).await;
    let mut relay = InternalWebSocketRelay::new(backend.url.clone());
    let frames = timeout(Duration::from_secs(2), relay.relay_kep_envelope(request(1)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(frames, vec![half.clone(), half]);
    assert_eq!(frames.iter().map(Vec::len).sum::<usize>(), BYTE_LIMIT);
    assert_eq!(backend.received(), vec![1]);
}

#[tokio::test]
async fn first_or_aggregate_byte_overflow_resets_without_resending_a_mutation() {
    for frames in [
        vec![sized_response(BYTE_LIMIT + 1)],
        vec![
            sized_response(BYTE_LIMIT / 2),
            sized_response(BYTE_LIMIT / 2 + 1),
        ],
    ] {
        let backend = Backend::start(Replies::Burst(frames)).await;
        let mut relay = InternalWebSocketRelay::new(backend.url.clone());
        timeout(Duration::from_secs(2), relay.relay_kep_envelope(request(1)))
            .await
            .unwrap()
            .expect_err("response byte overflow must fail the exchange");
        assert!(
            relay.writer.is_none() && relay.reader.is_none(),
            "overflow resets both socket halves"
        );
        tokio::time::sleep(Duration::from_millis(250)).await;
        assert_eq!(
            backend.received(),
            vec![1],
            "overflow must never retry a mutation"
        );
        assert_eq!(
            backend.connections.load(Ordering::SeqCst),
            1,
            "no automatic reconnect/resend"
        );
        let next = timeout(Duration::from_secs(2), relay.relay_kep_envelope(request(2)))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(next, vec![response(2, 0)]);
        assert_eq!(backend.received(), vec![1, 2]);
        assert_eq!(backend.connections.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn first_response_keeps_its_separate_two_second_wait() {
    let backend = Backend::start(Replies::DelayedFirst).await;
    let mut relay = InternalWebSocketRelay::new(backend.url.clone());
    let frames = timeout(Duration::from_secs(2), relay.relay_kep_envelope(request(1)))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        frames,
        vec![response(1, 0)],
        "the200ms drain deadline starts after the first response"
    );
    assert_eq!(backend.received(), vec![1]);
}

#[tokio::test]
#[ignore = "requires KITU_GATEWAY_TEST_WS_URL pointing to an isolated running admin host /ws endpoint"]
async fn actual_admin_host_broadcasts_allow_two_consecutive_mutations() {
    let url = std::env::var("KITU_GATEWAY_TEST_WS_URL")
        .expect("set KITU_GATEWAY_TEST_WS_URL for this explicit real-host test");
    let mut relay = InternalWebSocketRelay::new(url);
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    for id in 1..=2 {
        let mut message = OscMessage::new("/admin/world/spawn");
        message.push_arg(OscArg::Str(format!("bounded-relay-{unique}-{id}")));
        message.push_arg(OscArg::Float(id as f32));
        message.push_arg(OscArg::Float(2.0));
        message.push_arg(OscArg::Float(3.0));
        let mut envelope = KepEnvelope::osc(encode_osc_packet(&message).unwrap());
        envelope.route = Some("/room/main".into());
        envelope.correlation_id = Some(id);
        let frames = timeout(
            Duration::from_secs(2),
            relay.relay_kep_envelope(encode_kep_envelope(&envelope).unwrap()),
        )
        .await
        .expect("the live host's tick broadcasts must not hold the relay indefinitely")
        .unwrap();
        assert!(!frames.is_empty());
        assert!(frames.len() <= FRAME_LIMIT);
        assert!(frames.iter().map(Vec::len).sum::<usize>() <= BYTE_LIMIT);
        for frame in &frames {
            let decoded = decode_kep_envelope(frame).unwrap();
            assert_eq!(decoded.payload_type, kitu_transport::KEP_PAYLOAD_JSON);
            let value: serde_json::Value = serde_json::from_slice(&decoded.payload).unwrap();
            assert!(value.get("type").is_some());
        }
        eprintln!(
            "actual admin host request {id}: {} bounded JSON responses",
            frames.len()
        );
    }
    if let Some(writer) = relay.writer.as_mut() {
        writer.close().await.unwrap();
    }
    relay.reset();
}
