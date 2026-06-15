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
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

const DEFAULT_BIND: &str = "127.0.0.1:8787";

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
        .route("/app-actions/:id", get(app_action_definition))
        .route("/app-actions/:id/run", post(run_app_action))
        .route("/ws", get(ws_upgrade))
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

async fn ws_loop(mut socket: WebSocket, state: AppState) {
    if let Err(err) = send_initial_state(&mut socket, &state).await {
        error!("failed to send initial state: {err}");
        return;
    }

    let mut receiver = state.events.subscribe();

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
                        if let Err(err) = send_event(&mut socket, &event).await {
                            error!("websocket send error: {err}");
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        if let Ok(snapshot) = snapshot(&state) {
                            let _ = send_event(&mut socket, &ServerEvent::State { snapshot }).await;
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

async fn send_event(socket: &mut WebSocket, event: &ServerEvent) -> Result<()> {
    socket
        .send(Message::Text(serde_json::to_string(event)?))
        .await
        .context("send websocket event")
}

fn handle_client_osc(state: &AppState, client_message: ClientOscMessage) -> Result<()> {
    let osc_message = client_message.to_osc_message();
    let (action_id, inputs) = action_request_from_osc_message(&osc_message)?;
    run_app_action_request(state, action_id, inputs)?;
    Ok(())
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
) -> Result<(String, HashMap<String, ActionValue>)> {
    match message.address.as_str() {
        "/admin/world/spawn" => Ok((
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
        )),
        "/admin/world/move" => {
            let id = string_arg(message, 0)
                .ok_or_else(|| anyhow::anyhow!("/admin/world/move expects object id"))?;
            let x = numeric_arg(message, 1)
                .ok_or_else(|| anyhow::anyhow!("/admin/world/move expects x"))?;
            let y = numeric_arg(message, 2)
                .ok_or_else(|| anyhow::anyhow!("/admin/world/move expects y"))?;
            let z = numeric_arg(message, 3)
                .ok_or_else(|| anyhow::anyhow!("/admin/world/move expects z"))?;
            Ok((
                "move-object".to_string(),
                HashMap::from([
                    ("id".to_string(), ActionValue::String(id.to_string())),
                    ("x".to_string(), ActionValue::Float(x)),
                    ("y".to_string(), ActionValue::Float(y)),
                    ("z".to_string(), ActionValue::Float(z)),
                ]),
            ))
        }
        "/admin/world/reset" => Ok(("reset-world".to_string(), HashMap::new())),
        other => Err(anyhow::anyhow!("unsupported OSC admin address: {other}")),
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
