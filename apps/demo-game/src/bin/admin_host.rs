use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use kitu_app_actions::{ActionValue, AppActionCatalog, AppActionDefinition};
use kitu_demo_game::{build_demo_runtime, DemoRuntime};
use kitu_osc_ir::{OscArg, OscMessage};
use kitu_transport::{
    decode_kep_envelope, decode_osc_packet, encode_kep_envelope, KepEnvelope, KEP_PAYLOAD_OSC,
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

const DEFAULT_BIND: &str = "127.0.0.1:8787";
const KEP_ROUTE_SERVER_EVENT: &str = "/server/event";

#[derive(Clone)]
struct AppState {
    inner: Arc<Mutex<GameState>>,
    events: broadcast::Sender<ServerEvent>,
}

struct GameState {
    runtime: DemoRuntime,
    next_log_id: u64,
    logs: Vec<DebugLogEntry>,
}

impl GameState {
    fn new() -> Result<Self> {
        Ok(Self {
            runtime: build_demo_runtime()?,
            next_log_id: 1,
            logs: Vec::new(),
        })
    }

    fn snapshot(&self) -> WorldSnapshot {
        let runtime_snapshot = self.runtime.inspect_world_state();
        WorldSnapshot {
            tick: self.runtime.current_tick().get(),
            objects: runtime_snapshot
                .objects
                .iter()
                .map(WorldObject::from_runtime)
                .collect(),
        }
    }

    fn push_log(
        &mut self,
        level: LogLevel,
        message: impl Into<String>,
        osc_address: Option<String>,
    ) -> DebugLogEntry {
        let entry = DebugLogEntry {
            id: self.next_log_id,
            level,
            message: message.into(),
            osc_address,
            tick: self.runtime.current_tick().get(),
        };
        self.next_log_id += 1;
        self.logs.push(entry.clone());
        if self.logs.len() > 500 {
            self.logs.remove(0);
        }
        entry
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct WorldObject {
    id: String,
    kind: String,
    x: f32,
    y: f32,
    z: f32,
    color: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct DebugLogEntry {
    id: u64,
    level: LogLevel,
    message: String,
    osc_address: Option<String>,
    tick: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum LogLevel {
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct WorldSnapshot {
    tick: u64,
    objects: Vec<WorldObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
struct ClientOscMessage {
    address: String,
    #[serde(default)]
    args: Vec<JsonOscArg>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value", rename_all = "lowercase")]
enum JsonOscArg {
    Int(i32),
    Int64(i64),
    Float(f32),
    Str(String),
    Bool(bool),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ActionRunRequest {
    #[serde(default)]
    inputs: HashMap<String, ActionValue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionRunResponse {
    action_id: String,
    osc: ClientOscMessage,
    snapshot: WorldSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum ServerEvent {
    Connected {
        protocol: &'static str,
        tick: u64,
    },
    State {
        snapshot: WorldSnapshot,
    },
    Log {
        entry: DebugLogEntry,
    },
    Osc {
        address: String,
        args: Vec<JsonOscArg>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WsOutputMode {
    Json,
    Kep,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let (events, _) = broadcast::channel(256);
    let state = AppState {
        inner: Arc::new(Mutex::new(GameState::new()?)),
        events,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/state", get(state_snapshot))
        .route("/logs", get(logs_snapshot))
        .route("/app-actions", get(app_action_catalog))
        .route("/app-actions/{id}", get(app_action_definition))
        .route("/app-actions/{id}/run", post(run_app_action))
        .route("/ws", get(ws_upgrade))
        .route("/ws/runtime", get(runtime_ws_upgrade))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let bind = env::var("KITU_DEMO_GAME_BIND")
        .or_else(|_| env::var("KITU_WEB_ADMIN_BIND"))
        .unwrap_or_else(|_| DEFAULT_BIND.to_string());
    let addr: SocketAddr = bind
        .parse()
        .with_context(|| format!("invalid bind address: {bind}"))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("kitu demo game admin host listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "kitu-demo-game-admin-host"
    }))
}

async fn state_snapshot(State(state): State<AppState>) -> Result<Json<WorldSnapshot>, ApiError> {
    Ok(Json(snapshot(&state)?))
}

async fn logs_snapshot(
    State(state): State<AppState>,
) -> Result<Json<Vec<DebugLogEntry>>, ApiError> {
    let guard = state.inner.lock().map_err(|_| ApiError::state_poisoned())?;
    Ok(Json(guard.logs.clone()))
}

async fn app_action_catalog(
    State(state): State<AppState>,
) -> Result<Json<AppActionCatalog>, ApiError> {
    let guard = state.inner.lock().map_err(|_| ApiError::state_poisoned())?;
    Ok(Json(guard.runtime.app_action_catalog().clone()))
}

async fn app_action_definition(
    Path(action_id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<AppActionDefinition>, ApiError> {
    let guard = state.inner.lock().map_err(|_| ApiError::state_poisoned())?;
    let action = guard
        .runtime
        .app_action_catalog()
        .action(&action_id)
        .cloned()
        .ok_or_else(|| ApiError::bad_request(format!("unknown app action: {action_id}")))?;
    Ok(Json(action))
}

async fn run_app_action(
    Path(action_id): Path<String>,
    State(state): State<AppState>,
    Json(request): Json<ActionRunRequest>,
) -> Result<Json<ActionRunResponse>, ApiError> {
    Ok(Json(run_app_action_request(
        &state,
        action_id,
        request.inputs,
    )?))
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| ws_loop(socket, state))
}

async fn runtime_ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| runtime_ws_loop(socket, state))
}

async fn ws_loop(mut socket: WebSocket, state: AppState) {
    if let Err(err) = send_initial_state(&mut socket, &state).await {
        error!("failed to send initial state: {err}");
        return;
    }

    let mut receiver = state.events.subscribe();
    let mut output_mode = WsOutputMode::Json;

    loop {
        tokio::select! {
            maybe_message = socket.recv() => {
                match maybe_message {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientOscMessage>(&text) {
                            Ok(message) => {
                                if let Err(err) = handle_client_osc(&state, message) {
                                    broadcast_error(&state, err.to_string());
                                }
                            }
                            Err(err) => broadcast_error(&state, format!("invalid client message: {err}")),
                        }
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        output_mode = WsOutputMode::Kep;
                        match decode_kep_osc_message(&bytes) {
                            Ok(message) => {
                                if let Err(err) = handle_client_osc_message(&state, message) {
                                    broadcast_error(&state, err.to_string());
                                }
                            }
                            Err(err) => broadcast_error(&state, format!("invalid client KEP message: {err:#}")),
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        error!("websocket receive error: {err}");
                        break;
                    }
                }
            }
            event = receiver.recv() => {
                match event {
                    Ok(event) => {
                        if let Err(err) = send_event_with_mode(&mut socket, &event, output_mode).await {
                            error!("websocket send error: {err}");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        if let Ok(snapshot) = snapshot(&state) {
                            let _ = send_event_with_mode(&mut socket, &ServerEvent::State { snapshot }, output_mode).await;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn runtime_ws_loop(mut socket: WebSocket, state: AppState) {
    if let Err(err) = send_initial_runtime_state(&mut socket, &state).await {
        error!("failed to send initial runtime state: {err}");
        return;
    }

    let mut receiver = state.events.subscribe();
    let mut output_mode = WsOutputMode::Json;

    loop {
        tokio::select! {
            maybe_message = socket.recv() => {
                match maybe_message {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<ClientOscMessage>(&text) {
                            Ok(message) => {
                                if let Err(err) = handle_runtime_osc(&state, message) {
                                    broadcast_error(&state, err.to_string());
                                }
                            }
                            Err(err) => broadcast_error(&state, format!("invalid runtime client message: {err}")),
                        }
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        output_mode = WsOutputMode::Kep;
                        match decode_kep_osc_message(&bytes) {
                            Ok(message) => {
                                if let Err(err) = handle_runtime_osc_message(&state, message) {
                                    broadcast_error(&state, err.to_string());
                                }
                            }
                            Err(err) => broadcast_error(&state, format!("invalid runtime KEP message: {err:#}")),
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(err)) => {
                        error!("runtime websocket receive error: {err}");
                        break;
                    }
                }
            }
            event = receiver.recv() => {
                match event {
                    Ok(event) => {
                        if let Err(err) = send_event_with_mode(&mut socket, &event, output_mode).await {
                            error!("runtime websocket send error: {err}");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        if let Ok(snapshot) = snapshot(&state) {
                            let _ = send_event_with_mode(&mut socket, &ServerEvent::State { snapshot }, output_mode).await;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
}

async fn send_initial_state(socket: &mut WebSocket, state: &AppState) -> Result<()> {
    let (tick, logs) = {
        let guard = state
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        (guard.runtime.current_tick().get(), guard.logs.clone())
    };

    send_event(
        socket,
        &ServerEvent::Connected {
            protocol: "osc-ir-json-v1",
            tick,
        },
    )
    .await?;
    send_event(
        socket,
        &ServerEvent::State {
            snapshot: snapshot(state)?,
        },
    )
    .await?;

    for entry in logs {
        send_event(socket, &ServerEvent::Log { entry }).await?;
    }

    Ok(())
}

async fn send_initial_runtime_state(socket: &mut WebSocket, state: &AppState) -> Result<()> {
    let tick = {
        let guard = state
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        guard.runtime.current_tick().get()
    };

    send_event(
        socket,
        &ServerEvent::Connected {
            protocol: "kitu-runtime-osc-ir-json-v1",
            tick,
        },
    )
    .await?;
    send_event(
        socket,
        &ServerEvent::State {
            snapshot: snapshot(state)?,
        },
    )
    .await?;

    Ok(())
}

async fn send_event(socket: &mut WebSocket, event: &ServerEvent) -> Result<()> {
    socket
        .send(Message::Text(serde_json::to_string(event)?.into()))
        .await
        .context("send websocket event")
}

async fn send_event_with_mode(
    socket: &mut WebSocket,
    event: &ServerEvent,
    mode: WsOutputMode,
) -> Result<()> {
    match mode {
        WsOutputMode::Json => send_event(socket, event).await,
        WsOutputMode::Kep => {
            let bytes = encode_server_event_envelope(event)?;
            socket
                .send(Message::Binary(bytes.into()))
                .await
                .context("send websocket KEP event")
        }
    }
}

fn handle_client_osc(state: &AppState, client_message: ClientOscMessage) -> Result<()> {
    handle_client_osc_message(state, client_message.to_osc_message())
}

fn handle_client_osc_message(state: &AppState, osc_message: OscMessage) -> Result<()> {
    if let Some((action_id, inputs)) = action_request_from_osc_message(&osc_message)? {
        run_app_action_request(state, action_id, inputs)?;
        return Ok(());
    }

    let events = run_runtime_osc_request(state, osc_message)?;
    for event in events {
        let _ = state.events.send(event);
    }
    Ok(())
}

fn handle_runtime_osc(state: &AppState, client_message: ClientOscMessage) -> Result<()> {
    handle_runtime_osc_message(state, client_message.to_osc_message())
}

fn handle_runtime_osc_message(state: &AppState, osc_message: OscMessage) -> Result<()> {
    let events = run_runtime_osc_request(state, osc_message)?;
    for event in events {
        let _ = state.events.send(event);
    }
    Ok(())
}

fn run_runtime_osc_request(state: &AppState, osc_message: OscMessage) -> Result<Vec<ServerEvent>> {
    let mut bundle = kitu_osc_ir::OscBundle::new();
    bundle.push(osc_message.clone());

    let mut guard = state
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut outgoing_events = Vec::new();

    guard.runtime.enqueue_input(bundle);
    outgoing_events.push(ServerEvent::Log {
        entry: guard.push_log(
            LogLevel::Info,
            format!("runtime input {}", osc_message.to_debug_string()?),
            Some(osc_message.address),
        ),
    });

    guard.runtime.tick_once().context("tick Kitu runtime")?;
    for bundle in guard.runtime.drain_output_buffer() {
        for message in bundle.messages {
            outgoing_events.push(ServerEvent::Osc {
                address: message.address.clone(),
                args: message.args.into_iter().map(JsonOscArg::from).collect(),
            });
        }
    }

    outgoing_events.push(ServerEvent::State {
        snapshot: guard.snapshot(),
    });

    Ok(outgoing_events)
}

fn decode_kep_osc_message(bytes: &[u8]) -> Result<OscMessage> {
    let envelope = decode_kep_envelope(bytes).context("decode KEP envelope")?;
    anyhow::ensure!(
        envelope.payload_type == KEP_PAYLOAD_OSC,
        "unsupported KEP payload type: {}",
        envelope.payload_type
    );
    decode_osc_packet(&envelope.payload).context("decode KEP OSC payload")
}

fn encode_server_event_envelope(event: &ServerEvent) -> Result<Vec<u8>> {
    let mut envelope =
        KepEnvelope::json(serde_json::to_vec(event).context("encode server event JSON")?);
    envelope.route = Some(KEP_ROUTE_SERVER_EVENT.to_string());
    envelope.flags = Some(0);
    encode_kep_envelope(&envelope).context("encode server event KEP envelope")
}

fn run_app_action_request(
    state: &AppState,
    action_id: String,
    inputs: HashMap<String, ActionValue>,
) -> Result<ActionRunResponse> {
    let mut guard = state
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut outgoing_events = Vec::new();

    let outcome = guard
        .runtime
        .run_app_action(&action_id, &inputs)
        .with_context(|| format!("run app action `{action_id}`"))?;
    let osc_message = outcome.message;
    outgoing_events.push(ServerEvent::Log {
        entry: guard.push_log(
            LogLevel::Info,
            format!(
                "app action {} -> {}",
                outcome.action_id,
                osc_message.to_debug_string()?
            ),
            Some(osc_message.address.clone()),
        ),
    });

    guard.push_log(
        LogLevel::Info,
        format!("runtime accepted action {}", outcome.action_id),
        Some(osc_message.address.clone()),
    );

    guard.runtime.tick_once().context("tick Kitu runtime")?;
    for bundle in guard.runtime.drain_output_buffer() {
        for message in bundle.messages {
            outgoing_events.push(ServerEvent::Osc {
                address: message.address.clone(),
                args: message.args.into_iter().map(JsonOscArg::from).collect(),
            });
        }
    }

    let snapshot = guard.snapshot();
    outgoing_events.push(ServerEvent::State {
        snapshot: snapshot.clone(),
    });
    let response = ActionRunResponse {
        action_id: outcome.action_id,
        osc: ClientOscMessage::from(osc_message),
        snapshot,
    };

    drop(guard);

    for event in outgoing_events {
        let _ = state.events.send(event);
    }

    Ok(response)
}

fn action_request_from_osc_message(
    message: &OscMessage,
) -> Result<Option<(String, HashMap<String, ActionValue>)>> {
    match message.address.as_str() {
        "/admin/world/spawn" => Ok(Some((
            "spawn-object".to_string(),
            HashMap::from([
                (
                    "kind".to_string(),
                    ActionValue::String(string_arg(message, 0).unwrap_or("marker").to_string()),
                ),
                (
                    "x".to_string(),
                    ActionValue::Float(numeric_arg(message, 1).unwrap_or(0.0)),
                ),
                (
                    "y".to_string(),
                    ActionValue::Float(numeric_arg(message, 2).unwrap_or(0.0)),
                ),
                (
                    "z".to_string(),
                    ActionValue::Float(numeric_arg(message, 3).unwrap_or(0.0)),
                ),
            ]),
        ))),
        "/admin/world/move" => {
            let id = string_arg(message, 0)
                .ok_or_else(|| anyhow::anyhow!("/admin/world/move expects object id"))?;
            let x = numeric_arg(message, 1)
                .ok_or_else(|| anyhow::anyhow!("/admin/world/move expects x"))?;
            let y = numeric_arg(message, 2)
                .ok_or_else(|| anyhow::anyhow!("/admin/world/move expects y"))?;
            let z = numeric_arg(message, 3)
                .ok_or_else(|| anyhow::anyhow!("/admin/world/move expects z"))?;
            Ok(Some((
                "move-object".to_string(),
                HashMap::from([
                    ("id".to_string(), ActionValue::String(id.to_string())),
                    ("x".to_string(), ActionValue::Float(x)),
                    ("y".to_string(), ActionValue::Float(y)),
                    ("z".to_string(), ActionValue::Float(z)),
                ]),
            )))
        }
        "/admin/world/reset" => Ok(Some(("reset-world".to_string(), HashMap::new()))),
        _ => Ok(None),
    }
}

fn numeric_arg(message: &OscMessage, index: usize) -> Option<f32> {
    match message.args.get(index) {
        Some(OscArg::Float(value)) => Some(*value),
        Some(OscArg::Int(value)) => Some(*value as f32),
        Some(OscArg::Int64(value)) => Some(*value as f32),
        _ => None,
    }
}

fn string_arg(message: &OscMessage, index: usize) -> Option<&str> {
    match message.args.get(index) {
        Some(OscArg::Str(value)) if !value.is_empty() => Some(value),
        _ => None,
    }
}

fn color_for_kind(kind: &str) -> &'static str {
    match kind {
        "player" => "#38bdf8",
        "spawn-point" => "#2dd4bf",
        "enemy" => "#fb7185",
        "treasure" => "#facc15",
        "trigger" => "#a78bfa",
        _ => "#60a5fa",
    }
}

fn snapshot(state: &AppState) -> Result<WorldSnapshot> {
    let guard = state
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    Ok(guard.snapshot())
}

fn broadcast_error(state: &AppState, message: String) {
    let _ = state.events.send(ServerEvent::Error { message });
}

impl WorldObject {
    fn from_runtime(object: &kitu_runtime::WorldObject) -> Self {
        Self {
            id: object.id.clone(),
            kind: object.kind.clone(),
            x: object.transform.x,
            y: object.transform.y,
            z: object.transform.z,
            color: color_for_kind(&object.kind).to_string(),
        }
    }
}

impl ClientOscMessage {
    fn to_osc_message(&self) -> OscMessage {
        let mut message = OscMessage::new(self.address.clone());
        for arg in &self.args {
            message.push_arg(arg.clone().into());
        }
        message
    }
}

impl From<OscMessage> for ClientOscMessage {
    fn from(value: OscMessage) -> Self {
        Self {
            address: value.address,
            args: value.args.into_iter().map(JsonOscArg::from).collect(),
        }
    }
}

impl From<JsonOscArg> for OscArg {
    fn from(value: JsonOscArg) -> Self {
        match value {
            JsonOscArg::Int(value) => OscArg::Int(value),
            JsonOscArg::Int64(value) => OscArg::Int64(value),
            JsonOscArg::Float(value) => OscArg::Float(value),
            JsonOscArg::Str(value) => OscArg::Str(value),
            JsonOscArg::Bool(value) => OscArg::Bool(value),
        }
    }
}

impl From<OscArg> for JsonOscArg {
    fn from(value: OscArg) -> Self {
        match value {
            OscArg::Int(value) => JsonOscArg::Int(value),
            OscArg::Int64(value) => JsonOscArg::Int64(value),
            OscArg::Float(value) => JsonOscArg::Float(value),
            OscArg::Str(value) => JsonOscArg::Str(value),
            OscArg::Bool(value) => JsonOscArg::Bool(value),
        }
    }
}

#[derive(Debug)]
struct ApiError(anyhow::Error);

impl ApiError {
    fn state_poisoned() -> Self {
        Self(anyhow::anyhow!("state lock poisoned"))
    }

    fn bad_request(message: impl Into<String>) -> Self {
        Self(anyhow::anyhow!(message.into()))
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": self.0.to_string() })),
        )
            .into_response()
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(value: anyhow::Error) -> Self {
        Self(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> AppState {
        let (events, _) = broadcast::channel(16);
        AppState {
            inner: Arc::new(Mutex::new(GameState::new().unwrap())),
            events,
        }
    }

    #[test]
    fn runtime_osc_request_executes_player_move_slice() {
        let state = test_state();
        let request = ClientOscMessage {
            address: "/input/move".to_string(),
            args: vec![
                JsonOscArg::Str("player:local".to_string()),
                JsonOscArg::Float(1.25),
                JsonOscArg::Float(-0.5),
            ],
        };

        let events = run_runtime_osc_request(&state, request.to_osc_message()).unwrap();
        let render = events
            .iter()
            .find_map(|event| match event {
                ServerEvent::Osc { address, args } if address == "/render/player/transform" => {
                    Some(args)
                }
                _ => None,
            })
            .expect("expected render transform event");

        assert_eq!(render[0], JsonOscArg::Str("player:local".to_string()));
        assert_eq!(render[1], JsonOscArg::Int64(0));
        assert_eq!(render[2], JsonOscArg::Float(1.25));
        assert_eq!(render[3], JsonOscArg::Float(-0.5));
        assert_eq!(render[4], JsonOscArg::Float(0.0));

        let snapshot = events
            .iter()
            .find_map(|event| match event {
                ServerEvent::State { snapshot } => Some(snapshot),
                _ => None,
            })
            .expect("expected state event");
        assert!(snapshot.objects.iter().any(|object| {
            object.id == "player:local"
                && object.kind == "player"
                && object.x == 1.25
                && object.y == 0.0
                && object.z == -0.5
        }));
    }

    #[test]
    fn app_action_spawn_broadcasts_world_state_for_unity_clients() {
        let state = test_state();
        let response = run_app_action_request(
            &state,
            "spawn-object".to_string(),
            HashMap::from([
                ("kind".to_string(), ActionValue::String("enemy".to_string())),
                ("x".to_string(), ActionValue::Float(2.0)),
                ("y".to_string(), ActionValue::Float(0.5)),
                ("z".to_string(), ActionValue::Float(-3.0)),
            ]),
        )
        .unwrap();

        assert_eq!(response.snapshot.objects.len(), 1);
        assert_eq!(response.snapshot.objects[0].kind, "enemy");
        assert_eq!(response.snapshot.objects[0].x, 2.0);
        assert_eq!(response.snapshot.objects[0].y, 0.5);
        assert_eq!(response.snapshot.objects[0].z, -3.0);

        let mut receiver = state.events.subscribe();
        let events = run_app_action_request(
            &state,
            "spawn-object".to_string(),
            HashMap::from([
                (
                    "kind".to_string(),
                    ActionValue::String("treasure".to_string()),
                ),
                ("x".to_string(), ActionValue::Float(4.0)),
                ("y".to_string(), ActionValue::Float(0.0)),
                ("z".to_string(), ActionValue::Float(1.5)),
            ]),
        )
        .unwrap();
        assert_eq!(events.snapshot.objects.len(), 2);

        let mut saw_state = false;
        while let Ok(event) = receiver.try_recv() {
            if let ServerEvent::State { snapshot } = event {
                saw_state = true;
                assert_eq!(snapshot.objects.len(), 2);
                assert!(snapshot
                    .objects
                    .iter()
                    .any(|object| object.kind == "treasure"));
            }
        }

        assert!(saw_state, "expected state broadcast after spawn action");
    }

    #[test]
    fn admin_websocket_accepts_project_action_osc() {
        let state = test_state();
        let mut receiver = state.events.subscribe();
        let mut message = OscMessage::new("/game/enemy/spawn");
        message.push_arg(OscArg::Str("slime".to_string()));
        message.push_arg(OscArg::Float(1.0));
        message.push_arg(OscArg::Float(2.0));

        handle_client_osc_message(&state, message).unwrap();

        let mut saw_state = false;
        while let Ok(event) = receiver.try_recv() {
            if let ServerEvent::State { .. } = event {
                saw_state = true;
            }
        }

        assert!(saw_state, "expected state broadcast after project OSC");
    }

    #[test]
    fn kep_binary_decodes_to_osc_message() {
        let mut message = OscMessage::new("/admin/world/spawn");
        message.push_arg(OscArg::Str("marker".to_string()));
        message.push_arg(OscArg::Float(1.0));
        message.push_arg(OscArg::Float(2.0));
        message.push_arg(OscArg::Float(3.0));

        let osc_packet = kitu_transport::encode_osc_packet(&message).unwrap();
        let bytes =
            kitu_transport::encode_kep_envelope(&kitu_transport::KepEnvelope::osc(osc_packet))
                .unwrap();

        let decoded = decode_kep_osc_message(&bytes).unwrap();

        assert_eq!(decoded, message);
    }

    #[test]
    fn server_event_encodes_to_json_kep_envelope() {
        let bytes = encode_server_event_envelope(&ServerEvent::Error {
            message: "test error".to_string(),
        })
        .unwrap();

        let envelope = decode_kep_envelope(&bytes).unwrap();
        assert_eq!(envelope.payload_type, kitu_transport::KEP_PAYLOAD_JSON);
        assert_eq!(envelope.route.as_deref(), Some(KEP_ROUTE_SERVER_EVENT));

        let event: serde_json::Value = serde_json::from_slice(&envelope.payload).unwrap();
        assert_eq!(event["type"], "error");
        assert_eq!(event["message"], "test error");
    }
}
