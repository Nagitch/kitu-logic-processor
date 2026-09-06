//! Versioned Arena JSON/MessagePack connections sharing the host's owner clock.
//!
//! Clients negotiate a fixed subprotocol and compatible Hello before receiving
//! projections or owning gameplay input. Lag closes the connection; reconnecting
//! captures a new coherent snapshot and retains the client's producer identity.
use super::*;
use axum::extract::ws::CloseFrame;
use axum::http::{header::SEC_WEBSOCKET_PROTOCOL, HeaderMap};
use axum::response::Response;
use kitu_transport::application::{
    check_compatibility, ClientFrame, ClientHello, Codec, Compatibility, Encoding, ErrorCode,
    ErrorFrame, ExecutionStatus, ExecutionVersion, InputFrame, OutputBatchRef, OutputFrameRef,
    Role, ServerFrameRef, ServerHello, ServerPayloadRef, SnapshotFrameRef, SnapshotReason,
    WireLimits, NETWORK_INPUT_LIMITS, NETWORK_OUTPUT_LIMITS,
};
use std::time::Duration;

/// Fixed text-frame subprotocol for Arena wire version one.
pub const JSON_PROTOCOL: &str = "kitu-arena-json-v1";
/// Fixed binary-frame subprotocol for Arena wire version one.
pub const MESSAGEPACK_PROTOCOL: &str = "kitu-arena-msgpack-v1";
/// Maximum queued network commands, including the controller's eventual disconnect.
pub const MAX_PENDING_INPUTS: usize = 256;
/// Maximum encoded network input bytes awaiting an owner tick.
pub const MAX_PENDING_INPUT_BYTES: usize = 1024 * 1024;
pub(super) const BATCH_CAPACITY: usize = 16;
const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(5);
const SEND_TIMEOUT: Duration = Duration::from_secs(3);

/// Returns the capabilities required by the dedicated Arena connection.
///
/// This identity also appears in native host inspection; it does not rename the
/// enclosing demo-game Runtime or weaken detached replay version validation.
pub fn compatibility() -> Compatibility {
    Compatibility {
        app_id: "endless-arena".into(),
        wire_version: 1,
        schema_version: arena::SCHEMA_VERSION,
        presentation_version: arena::presentation::CONTRACT_VERSION,
        tick_rate: 60,
        features: ["output-batches", "presentation", "replay", "typed-osc"]
            .map(str::to_owned)
            .to_vec(),
    }
}

/// Returns the same execution identity used to verify saved Arena recordings.
pub fn execution() -> ExecutionVersion {
    let version = crate::replay::ExecutionVersion::current();
    ExecutionVersion {
        package: version.package,
        source_hash: version.source_hash,
        target: version.target,
    }
}

fn status(game: &GameState) -> ExecutionStatus {
    ExecutionStatus {
        playback_mode: playback::mode(game),
        read_only: game.ensure_live_input().is_err(),
    }
}

/// One complete publication captured while the owner still holds the game lock.
pub(super) struct Publication {
    watermark: u64,
    status: ExecutionStatus,
    bundles: Vec<OscBundle>,
    reason: Option<SnapshotReason>,
}

pub(super) fn publish(state: &AppState, game: &mut GameState, output: &[OscBundle]) {
    let replace = std::mem::take(&mut game.wire_snapshot_pending);
    if state.arena_events.receiver_count() == 0 {
        return;
    }
    let publication = Publication {
        watermark: game.publication_id,
        status: status(game),
        bundles: if replace {
            game.application_projection()
        } else {
            output.to_vec()
        },
        reason: replace.then_some(SnapshotReason::Seek),
    };
    // Synchronous ring insertion under the owner lock preserves the subscription
    // boundary. Encoding, socket writes and all persistence remain outside it.
    let _ = state.arena_events.send(Arc::new(publication));
}

pub(super) async fn upgrade(
    headers: HeaderMap,
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    let protocols = headers
        .get_all(SEC_WEBSOCKET_PROTOCOL)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(','))
        .map(str::trim)
        .collect::<Vec<_>>();
    let (protocol, encoding) = match protocols.as_slice() {
        [JSON_PROTOCOL] => (JSON_PROTOCOL, Encoding::Json),
        [MESSAGEPACK_PROTOCOL] => (MESSAGEPACK_PROTOCOL, Encoding::MessagePack),
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                "offer exactly one supported Arena subprotocol",
            )
                .into_response()
        }
    };
    if state.work.is_closing() {
        return (StatusCode::SERVICE_UNAVAILABLE, "host is shutting down").into_response();
    }
    ws.protocols([protocol])
        .max_message_size(NETWORK_INPUT_LIMITS.max_bytes)
        .max_frame_size(NETWORK_INPUT_LIMITS.max_bytes)
        .on_upgrade(move |socket| lifetime(socket, state, encoding))
        .into_response()
}

async fn lifetime(socket: WebSocket, state: AppState, encoding: Encoding) {
    let Ok(_work) = state.work.enter() else {
        return;
    };
    let mut closing = state.work.subscribe();
    if *closing.borrow() {
        return;
    }
    // Cancellation covers the entire socket future, including blocked writes.
    // The controller lease drops before host-owned work can finish shutdown.
    tokio::select! {
        _ = closing.changed() => {},
        _ = connection(socket, state.clone(), encoding) => {},
    }
}

struct Lease {
    state: AppState,
    connection_id: u64,
}
impl Drop for Lease {
    fn drop(&mut self) {
        release_arena_controller(&self.state, self.connection_id);
    }
}

struct Handshake {
    hello: ServerHello,
    publication: Publication,
    receiver: broadcast::Receiver<Arc<Publication>>,
    lease: Lease,
    client_id: String,
}

fn refusal(
    code: ErrorCode,
    message: impl Into<String>,
    fatal: bool,
    id: Option<u64>,
) -> ErrorFrame {
    let message = message.into();
    let mut end = message.len().min(1024);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    ErrorFrame {
        code,
        message: message[..end].to_owned(),
        fatal,
        input_id: id,
    }
}

fn handshake(state: &AppState, client: ClientHello) -> std::result::Result<Handshake, ErrorFrame> {
    check_compatibility(&compatibility(), &client.compatibility)
        .map_err(|error| refusal(ErrorCode::Incompatible, error.to_string(), true, None))?;
    if client.client_id.is_empty()
        || client.client_id.len() > 128
        || client.client_id.contains('\0')
        || client.client_id.starts_with("host:")
    {
        return Err(refusal(
            ErrorCode::Protocol,
            "invalid or reserved client identity",
            true,
            None,
        ));
    }
    let mut game = state
        .inner
        .lock()
        .map_err(|_| refusal(ErrorCode::Internal, "state lock poisoned", true, None))?;
    if client
        .expected_session_id
        .as_ref()
        .is_some_and(|id| id != &game.runtime_id)
    {
        return Err(refusal(
            ErrorCode::SessionChanged,
            "Arena runtime session changed; synchronize before sending input",
            true,
            None,
        ));
    }
    if client.role == Role::Controller {
        if state.options.external_controller || game.controller.is_some() {
            return Err(refusal(
                ErrorCode::ControllerBusy,
                "Arena already has an active controller",
                true,
                None,
            ));
        }
        if game.wire_pending_inputs >= MAX_PENDING_INPUTS {
            return Err(refusal(
                ErrorCode::QueueFull,
                "wait for an owner tick before reconnecting a controller",
                true,
                None,
            ));
        }
    }
    let id = game.next_connection_id;
    game.next_connection_id = id.checked_add(1).ok_or_else(|| {
        refusal(
            ErrorCode::Internal,
            "connection identity exhausted",
            true,
            None,
        )
    })?;
    // Subscribe and capture projection/status under the same lock used to publish
    // ticks. No owner update can be lost between these operations.
    let receiver = state.arena_events.subscribe();
    let status = status(&game);
    let publication = Publication {
        watermark: game.publication_id,
        status: status.clone(),
        bundles: game.application_projection(),
        reason: Some(SnapshotReason::Initial),
    };
    let hello = ServerHello {
        compatibility: compatibility(),
        execution: execution(),
        session_id: game.runtime_id.clone(),
        role: client.role,
        limits: WireLimits {
            max_input_bytes: NETWORK_INPUT_LIMITS.max_bytes as u32,
            max_output_bytes: NETWORK_OUTPUT_LIMITS.max_bytes as u32,
        },
        status,
    };
    if client.role == Role::Controller {
        game.controller = Some((id, client.client_id.clone()));
    }
    Ok(Handshake {
        hello,
        publication,
        receiver,
        lease: Lease {
            state: state.clone(),
            connection_id: id,
        },
        client_id: client.client_id,
    })
}

fn input(
    state: &AppState,
    connection_id: u64,
    client_id: &str,
    role: &Role,
    input: InputFrame,
    encoded_bytes: usize,
) -> std::result::Result<(), ErrorFrame> {
    let input_id = input.metadata.as_ref().map(|metadata| metadata.message_id);
    let error = |code, message| refusal(code, message, false, input_id);
    if *role != Role::Controller {
        return Err(error(
            ErrorCode::ReadOnly,
            "observer connections cannot submit gameplay input".into(),
        ));
    }
    let metadata = input.metadata.ok_or_else(|| {
        error(
            ErrorCode::InvalidInput,
            "Arena input requires metadata".into(),
        )
    })?;
    if metadata.source != client_id
        || metadata.schema_version != arena::SCHEMA_VERSION
        || metadata.message_id == 0
    {
        return Err(error(
            ErrorCode::InvalidInput,
            "input metadata does not match the negotiated producer and schema".into(),
        ));
    }
    let bundle = OscBundle::try_from(input.bundle)
        .map_err(|e| error(ErrorCode::InvalidInput, e.to_string()))?;
    if bundle.messages.len() != 1 || bundle.messages[0].address == "/input/arena/disconnect" {
        return Err(error(
            ErrorCode::InvalidInput,
            "Arena requires one command; disconnect is host-owned".into(),
        ));
    }
    arena::validate_input(&bundle.messages[0], &metadata)
        .map_err(|e| error(ErrorCode::InvalidInput, e.to_string()))?;
    let mut game = state
        .inner
        .lock()
        .map_err(|_| error(ErrorCode::Internal, "state lock poisoned".into()))?;
    game.ensure_live_input()
        .map_err(|e| error(ErrorCode::ReadOnly, e.to_string()))?;
    if game.controller.as_ref() != Some(&(connection_id, client_id.to_owned())) {
        return Err(error(
            ErrorCode::ControllerBusy,
            "this connection does not own Arena control".into(),
        ));
    }
    // Reserve one slot for a disconnect even when the input queue is saturated.
    if game.wire_pending_inputs >= MAX_PENDING_INPUTS - 1
        || encoded_bytes > MAX_PENDING_INPUT_BYTES.saturating_sub(game.wire_pending_bytes)
    {
        return Err(error(
            ErrorCode::QueueFull,
            "Arena network input queue is full; wait for the owner tick".into(),
        ));
    }
    game.runtime
        .try_enqueue_input(bundle, Some(metadata))
        .map_err(|e| error(ErrorCode::InvalidInput, e.to_string()))?;
    game.wire_pending_inputs += 1;
    game.wire_pending_bytes += encoded_bytes;
    Ok(())
}

struct Sender {
    codec: Codec,
    encoding: Encoding,
    next: u64,
}
impl Sender {
    async fn send(&mut self, socket: &mut WebSocket, frame: ServerPayloadRef<'_>) -> Result<()> {
        let next = self
            .next
            .checked_add(1)
            .context("delivery sequence exhausted")?;
        let bytes = self.codec.encode_server_ref(&ServerFrameRef {
            delivery_sequence: self.next,
            frame,
        })?;
        let message = match self.encoding {
            Encoding::Json => Message::Text(String::from_utf8(bytes)?.into()),
            Encoding::MessagePack => Message::Binary(bytes.into()),
        };
        tokio::time::timeout(SEND_TIMEOUT, socket.send(message))
            .await
            .context("Arena client write timed out")??;
        self.next = next;
        Ok(())
    }
    async fn publication(
        &mut self,
        socket: &mut WebSocket,
        publication: &Publication,
    ) -> Result<()> {
        let batch = OutputBatchRef {
            tick: publication.status.playback_mode.tick,
            bundles: &publication.bundles,
        };
        let frame = if let Some(reason) = &publication.reason {
            ServerPayloadRef::Snapshot(SnapshotFrameRef {
                reason: *reason,
                batch,
                status: &publication.status,
            })
        } else {
            ServerPayloadRef::Output(OutputFrameRef {
                batch,
                status: &publication.status,
            })
        };
        self.send(socket, frame).await
    }
    async fn error(&mut self, socket: &mut WebSocket, error: &ErrorFrame, close_code: u16) {
        let _ = self.send(socket, ServerPayloadRef::Error(error)).await;
        if error.fatal {
            let _ = tokio::time::timeout(
                SEND_TIMEOUT,
                socket.send(Message::Close(Some(CloseFrame {
                    code: close_code,
                    reason: "Arena connection requires synchronization".into(),
                }))),
            )
            .await;
        }
    }
}

fn application_message(message: Message, encoding: Encoding) -> Result<Vec<u8>> {
    match (encoding, message) {
        (Encoding::Json, Message::Text(text)) => Ok(text.as_bytes().to_vec()),
        (Encoding::MessagePack, Message::Binary(bytes)) => Ok(bytes.to_vec()),
        _ => anyhow::bail!("WebSocket frame encoding does not match the negotiated subprotocol"),
    }
}

async fn acknowledge_peer_close<F: std::future::Future>(
    lease: Option<&Lease>,
    flush: F,
) -> std::result::Result<F::Output, tokio::time::error::Elapsed> {
    if let Some(lease) = lease {
        // The peer may reconnect as soon as this reply reaches it. Ownership
        // must already be free before even polling the acknowledgment write.
        release_arena_controller(&lease.state, lease.connection_id);
    }
    tokio::time::timeout(SEND_TIMEOUT, flush).await
}

async fn next_application(
    socket: &mut WebSocket,
    encoding: Encoding,
    lease: Option<&Lease>,
) -> Result<Option<Vec<u8>>> {
    loop {
        match socket.recv().await {
            None => return Ok(None),
            Some(Ok(Message::Close(_))) => {
                // The next read flushes tungstenite's queued close reply. Release
                // control first so the reply is an ownership handoff boundary.
                // Lease::drop remains idempotent for this path and handles all
                // abrupt disconnects, protocol errors and host shutdowns.
                let _ = acknowledge_peer_close(lease, socket.recv()).await;
                return Ok(None);
            }
            Some(Ok(Message::Ping(bytes))) => {
                tokio::time::timeout(SEND_TIMEOUT, socket.send(Message::Pong(bytes))).await??;
            }
            Some(Ok(Message::Pong(_))) => {}
            Some(Ok(message)) => return application_message(message, encoding).map(Some),
            Some(Err(_)) => {
                anyhow::bail!("invalid WebSocket message or message exceeds the input byte limit")
            }
        }
    }
}

async fn connection(mut socket: WebSocket, state: AppState, encoding: Encoding) {
    let Ok(input_codec) = Codec::new(encoding, NETWORK_INPUT_LIMITS) else {
        return;
    };
    let Ok(output_codec) = Codec::new(encoding, NETWORK_OUTPUT_LIMITS) else {
        return;
    };
    let mut sender = Sender {
        codec: output_codec,
        encoding,
        next: 0,
    };
    let first = tokio::time::timeout(
        HANDSHAKE_TIMEOUT,
        next_application(&mut socket, encoding, None),
    )
    .await;
    let hello = match first {
        Ok(Ok(Some(bytes))) => match input_codec.decode_client(&bytes) {
            Ok(ClientFrame::Hello(hello)) => hello,
            Ok(_) => {
                sender
                    .error(
                        &mut socket,
                        &refusal(
                            ErrorCode::Protocol,
                            "Hello must be the first application message",
                            true,
                            None,
                        ),
                        1008,
                    )
                    .await;
                return;
            }
            Err(error) => {
                sender
                    .error(
                        &mut socket,
                        &refusal(ErrorCode::Protocol, error.to_string(), true, None),
                        1008,
                    )
                    .await;
                return;
            }
        },
        Ok(Ok(None)) => return,
        error => {
            let message = match error {
                Err(_) => "Arena Hello timed out".to_owned(),
                Ok(Err(error)) => error.to_string(),
                _ => unreachable!(),
            };
            let close_code = if message.contains("input byte limit") {
                1009
            } else {
                1008
            };
            sender
                .error(
                    &mut socket,
                    &refusal(ErrorCode::Protocol, message, true, None),
                    close_code,
                )
                .await;
            return;
        }
    };
    let Handshake {
        hello,
        publication,
        mut receiver,
        lease,
        client_id,
    } = match handshake(&state, hello) {
        Ok(handshake) => handshake,
        Err(error) => {
            sender.error(&mut socket, &error, 1008).await;
            return;
        }
    };
    if let Err(error) = async {
        sender
            .send(&mut socket, ServerPayloadRef::Hello(&hello))
            .await?;
        sender.publication(&mut socket, &publication).await
    }
    .await
    {
        sender
            .error(
                &mut socket,
                &refusal(ErrorCode::Internal, error.to_string(), true, None),
                1009,
            )
            .await;
        return;
    }
    let watermark = publication.watermark;
    loop {
        tokio::select! {
            incoming = next_application(&mut socket, encoding, Some(&lease)) => {
                let bytes = match incoming {
                    Ok(Some(bytes)) => bytes,
                    Ok(None) => break,
                    Err(error) => {
                        let close_code = if error.to_string().contains("input byte limit") { 1009 } else { 1008 };
                        sender.error(&mut socket, &refusal(ErrorCode::Protocol, error.to_string(), true, None), close_code).await;
                        break;
                    }
                };
                let frame = match input_codec.decode_client(&bytes) {
                    Ok(frame) => frame,
                    Err(error) => {
                        sender.error(&mut socket, &refusal(ErrorCode::Protocol, error.to_string(), true, None), 1008).await;
                        break;
                    }
                };
                let result = match frame {
                    ClientFrame::Hello(_) => Err(refusal(ErrorCode::Protocol, "Hello is only valid once", true, None)),
                    ClientFrame::Input(frame) => input(&state, lease.connection_id, &client_id, &hello.role, frame, bytes.len()),
                };
                if let Err(error) = result {
                    sender.error(&mut socket, &error, 1008).await;
                    if error.fatal { break; }
                }
            },
            event = receiver.recv() => {
                match event {
                    Ok(event) if event.watermark <= watermark => continue,
                    Ok(event) => {
                        if let Err(error) = sender.publication(&mut socket, &event).await {
                            sender.error(&mut socket, &refusal(ErrorCode::Internal, error.to_string(), true, None), 1009).await;
                            break;
                        }
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        sender.error(&mut socket, &refusal(ErrorCode::Protocol, "Arena output lagged; reconnect for a complete synchronization snapshot", true, None), 1008).await;
                        break;
                    },
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn close_acknowledgment_starts_after_release_and_old_drop_preserves_replacement() {
        let state = super::super::tests::test_state();
        let controller = || ClientHello {
            compatibility: compatibility(),
            client_id: "reconnecting-controller".into(),
            role: Role::Controller,
            expected_session_id: None,
        };
        let original = handshake(&state, controller()).unwrap();
        let original_id = original.lease.connection_id;
        let replacement = acknowledge_peer_close(Some(&original.lease), async {
            // This is the first poll of the acknowledgment future, not a delay
            // after socket receipt. No scheduler timing can conceal a held lease.
            assert!(state.inner.lock().unwrap().controller.is_none());
            handshake(&state, controller()).unwrap()
        })
        .await
        .unwrap();
        assert_ne!(replacement.lease.connection_id, original_id);
        drop(original);
        let game = state.inner.lock().unwrap();
        assert_eq!(
            game.controller.as_ref(),
            Some(&(
                replacement.lease.connection_id,
                "reconnecting-controller".to_owned(),
            ))
        );
        assert_eq!(
            game.wire_pending_inputs, 1,
            "old cleanup must not queue a second disconnect"
        );
    }
}
