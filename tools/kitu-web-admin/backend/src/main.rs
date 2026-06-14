use std::{
    env,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use anyhow::{Context, Result};
use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        State,
    },
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use kitu_osc_ir::{OscArg, OscMessage};
use kitu_runtime::{build_runtime, Runtime};
use kitu_transport::LocalChannel;
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
    runtime: Runtime<LocalChannel>,
    next_log_id: u64,
    logs: Vec<DebugLogEntry>,
}

impl Default for GameState {
    fn default() -> Self {
        Self {
            runtime: build_runtime(LocalChannel::connected()),
            next_log_id: 1,
            logs: Vec::new(),
        }
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
        inner: Arc::new(Mutex::new(GameState::default())),
        events,
    };

    let app = Router::new()
        .route("/health", get(health))
        .route("/state", get(state_snapshot))
        .route("/logs", get(logs_snapshot))
        .route("/ws", get(ws_upgrade))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let bind = env::var("KITU_WEB_ADMIN_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_string());
    let addr: SocketAddr = bind
        .parse()
        .with_context(|| format!("invalid KITU_WEB_ADMIN_BIND address: {bind}"))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    info!("kitu web admin demo backend listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "service": "kitu-web-admin-demo-backend"
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
    let mut guard = state
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut outgoing_events = Vec::new();

    let osc_message = client_message.to_osc_message();
    guard.push_log(
        LogLevel::Info,
        format!("admin -> backend {}", osc_message.to_debug_string()?),
        Some(osc_message.address.clone()),
    );

    match osc_message.address.as_str() {
        "/admin/world/spawn" => {
            let object = spawn_object(&mut guard, &osc_message)?;
            outgoing_events.push(ServerEvent::Log {
                entry: guard.push_log(
                    LogLevel::Info,
                    format!("spawned {} {}", object.kind, object.id),
                    Some(osc_message.address.clone()),
                ),
            });
        }
        "/admin/world/move" => {
            let moved = move_object(&mut guard, &osc_message)?;
            outgoing_events.push(ServerEvent::Log {
                entry: guard.push_log(
                    LogLevel::Info,
                    format!(
                        "moved {} to ({:.1}, {:.1}, {:.1})",
                        moved.id, moved.x, moved.y, moved.z
                    ),
                    Some(osc_message.address.clone()),
                ),
            });
        }
        "/admin/world/reset" => {
            guard.runtime.reset_world_objects();
            outgoing_events.push(ServerEvent::Log {
                entry: guard.push_log(
                    LogLevel::Warn,
                    "cleared world objects",
                    Some(osc_message.address.clone()),
                ),
            });
        }
        other => {
            return Err(anyhow::anyhow!("unsupported OSC admin address: {other}"));
        }
    }

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

    drop(guard);

    for event in outgoing_events {
        let _ = state.events.send(event);
    }

    Ok(())
}

fn spawn_object(state: &mut GameState, message: &OscMessage) -> Result<WorldObject> {
    let kind = string_arg(message, 0).unwrap_or("marker").to_string();
    let x = numeric_arg(message, 1).unwrap_or(0.0);
    let y = numeric_arg(message, 2).unwrap_or(0.0);
    let z = numeric_arg(message, 3).unwrap_or(0.0);
    let object = state
        .runtime
        .spawn_world_object(kind, x, y, z)
        .context("spawn runtime world object")?;
    Ok(WorldObject::from_runtime(&object))
}

fn move_object(state: &mut GameState, message: &OscMessage) -> Result<WorldObject> {
    let id = string_arg(message, 0)
        .ok_or_else(|| anyhow::anyhow!("/admin/world/move expects object id"))?;
    let x =
        numeric_arg(message, 1).ok_or_else(|| anyhow::anyhow!("/admin/world/move expects x"))?;
    let y =
        numeric_arg(message, 2).ok_or_else(|| anyhow::anyhow!("/admin/world/move expects y"))?;
    let z =
        numeric_arg(message, 3).ok_or_else(|| anyhow::anyhow!("/admin/world/move expects z"))?;

    let moved = state
        .runtime
        .move_world_object(id, x, y, z)
        .with_context(|| format!("move runtime world object: {id}"))?;
    Ok(WorldObject::from_runtime(&moved))
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

impl GameState {
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

    #[test]
    fn client_message_converts_to_osc_ir() {
        let message = ClientOscMessage {
            address: "/admin/world/spawn".to_string(),
            args: vec![JsonOscArg::Str("enemy".to_string()), JsonOscArg::Float(3.0)],
        };

        let osc = message.to_osc_message();
        assert_eq!(osc.address, "/admin/world/spawn");
        assert_eq!(osc.args[0], OscArg::Str("enemy".to_string()));
        assert_eq!(osc.args[1], OscArg::Float(3.0));
    }

    #[test]
    fn spawn_and_move_update_world_objects() {
        let mut state = GameState::default();
        let mut spawn = OscMessage::new("/admin/world/spawn");
        spawn.push_arg(OscArg::Str("enemy".to_string()));
        spawn.push_arg(OscArg::Float(1.0));
        spawn.push_arg(OscArg::Float(0.0));
        spawn.push_arg(OscArg::Float(2.0));

        let object = spawn_object(&mut state, &spawn).unwrap();
        assert_eq!(object.id, "obj-1");
        assert_eq!(state.runtime.inspect_world_state().objects.len(), 1);

        let mut move_message = OscMessage::new("/admin/world/move");
        move_message.push_arg(OscArg::Str(object.id));
        move_message.push_arg(OscArg::Float(4.0));
        move_message.push_arg(OscArg::Float(0.5));
        move_message.push_arg(OscArg::Float(6.0));

        let moved = move_object(&mut state, &move_message).unwrap();
        assert_eq!(moved.x, 4.0);
        assert_eq!(moved.y, 0.5);
        assert_eq!(moved.z, 6.0);

        let snapshot = state.snapshot();
        assert_eq!(snapshot.objects, vec![moved]);
    }
}
