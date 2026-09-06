//! Shared Arena host state, with one explicit owner of the simulation clock.
//!
//! HTTP handlers admit work and inspect projections. A standalone server or an
//! embedded caller owns [`ArenaHost::tick`]; constructing a router starts no clock.

use std::{
    collections::HashMap,
    env,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};

use crate::{arena, build_arena_runtime, DemoRuntime};
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
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use kitu_runtime::InputMetadata;
use kitu_transport::{
    decode_kep_envelope, decode_osc_packet, encode_kep_envelope, KepEnvelope, KEP_PAYLOAD_OSC,
};
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tracing::{error, info};

const DEFAULT_BIND: &str = "127.0.0.1:8787";
const KEP_ROUTE_SERVER_EVENT: &str = "/server/event";

mod content;
mod playback;
mod recording;
mod script;
mod shell;
mod work;

/// Per-instance storage, connection ownership and background I/O configuration.
#[derive(Clone)]
pub struct HostOptions {
    /// Reserve gameplay control for the embedding caller; websocket clients observe.
    pub external_controller: bool,
    /// Editable TMD, SQLite database or source plan; loaded only on explicit validation.
    pub content_path: PathBuf,
    /// Editable Rhai source loaded on explicit validation; `None` uses the bundled script.
    pub script_path: Option<PathBuf>,
    /// Destination for immutable run manifests.
    pub run_directory: PathBuf,
    /// Destination for TSQ1 recordings owned by this host.
    pub recording_directory: PathBuf,
    /// Whether successful live starts asynchronously persist run manifests.
    pub persist_runs: bool,
    /// Explicit runtime used by native-owner ticks to schedule I/O.
    pub io_runtime: Option<tokio::runtime::Handle>,
    /// Actual listening endpoint, including an assigned ephemeral port when used.
    pub bridge_endpoint: Option<String>,
}

impl Default for HostOptions {
    fn default() -> Self {
        Self {
            external_controller: false,
            content_path: "apps/demo-game/content/arena.tmd".into(),
            script_path: None,
            run_directory: "apps/demo-game/.arena/runs".into(),
            recording_directory: "apps/demo-game/.arena/recordings".into(),
            persist_runs: false,
            io_runtime: None,
            bridge_endpoint: None,
        }
    }
}

/// Exclusive clock owner for one Runtime, recorder, playback state and command history.
pub struct ArenaHost {
    state: AppState,
}

impl ArenaHost {
    /// Wraps an already configured, unstarted Arena without replacing its content.
    ///
    /// # Examples
    /// ```
    /// use kitu_demo_game::{build_arena_runtime, host::{ArenaHost, HostOptions}};
    /// let mut host = ArenaHost::new(build_arena_runtime()?, HostOptions::default())?;
    /// assert!(!host.inspect()?.is_empty());
    /// host.tick()?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn new(runtime: DemoRuntime, options: HostOptions) -> Result<Self> {
        anyhow::ensure!(
            !options.persist_runs || options.io_runtime.is_some(),
            "run persistence requires an explicit I/O runtime handle"
        );
        let (events, _) = broadcast::channel(256);
        Ok(Self {
            state: AppState {
                inner: Arc::new(Mutex::new(GameState::from_runtime(runtime)?)),
                events,
                content: Arc::new(content::Service::new(
                    options.content_path.clone(),
                    options.run_directory.clone(),
                )),
                script: Arc::new(script::Service::new(options.script_path.clone())),
                shell: Arc::new(shell::Service::default()),
                work: Arc::new(work::Work::default()),
                playback_operations: Arc::new(tokio::sync::Mutex::new(())),
                verification_slots: Arc::new(tokio::sync::Semaphore::new(1)),
                options: Arc::new(options),
            },
        })
    }

    /// Admits a native owner's ordinary input without executing it or changing clocks.
    /// Metadata and application validation use the same Runtime queue as HTTP Shell.
    pub fn submit(&mut self, bundle: OscBundle, metadata: Option<InputMetadata>) -> Result<u64> {
        self.state.work.check()?;
        // Native owners can supply detached management source. Compile/probe it
        // before taking the host clock lock, then use ordinary queue validation.
        let script = arena::prepare_script_input(&bundle, metadata.as_ref())?;
        let mut game = self
            .state
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        game.ensure_live_input()?;
        if let Some(script) = script {
            arena::pin_prepared_script(&mut game.runtime, script)?;
        }
        game.runtime
            .try_enqueue_input(bundle, metadata)
            .map_err(Into::into)
    }

    /// Advances the authoritative host once, retaining original OSC bundle boundaries.
    /// Broadcast and persistence work is dispatched after releasing the game lock.
    pub fn tick(&mut self) -> Result<Vec<OscBundle>> {
        self.state.work.check()?;
        let tick = advance_tick(&self.state)?;
        for event in tick.run_events {
            content::save_run_event(&self.state, &event);
        }
        for event in tick.events {
            let _ = self.state.events.send(event);
        }
        Ok(tick.output)
    }

    /// Returns detached game projections, including the last verified playback state.
    pub fn inspect(&self) -> Result<Vec<OscBundle>> {
        let game = self
            .state
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        Ok(game.application_projection())
    }

    /// Returns coherent host-only session/playback information, excluded from recordings.
    pub fn inspect_host(&self) -> Result<Vec<OscBundle>> {
        let game = self
            .state
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let value = serde_json::json!({
            "sessionId": game.runtime_id, "schemaVersion": arena::SCHEMA_VERSION,
            "playbackMode": playback::mode(&game), "readOnly": game.ensure_live_input().is_err(),
            "bridgeEndpoint": self.state.options.bridge_endpoint,
            "closing": self.state.work.is_closing(),
        });
        let mut message = OscMessage::new("/host/arena/status");
        message.push_arg(OscArg::Str(serde_json::to_string(&value)?));
        Ok(vec![OscBundle {
            messages: vec![message],
        }])
    }

    /// Creates an HTTP/WS router observing this instance; this never starts a timer.
    pub fn router(&self) -> Router {
        router(self.state.clone())
    }

    /// Refuses new work and asks observers, operator tasks and CPU work to stop.
    pub fn begin_shutdown(&self) {
        self.state.work.close();
    }

    /// Waits for all host-owned background jobs and observer sockets to release ownership.
    /// Call [`Self::begin_shutdown`] first, then stop/join the embedding I/O runtime.
    pub async fn wait_shutdown(&self) {
        self.state.work.wait().await;
    }
}

/// Runs the standalone 60 Hz scheduler and HTTP server configured by existing environment variables.
pub async fn serve_from_environment() -> Result<()> {
    let bind = env::var("KITU_DEMO_GAME_BIND")
        .or_else(|_| env::var("KITU_WEB_ADMIN_BIND"))
        .unwrap_or_else(|_| DEFAULT_BIND.to_string());
    let addr: SocketAddr = bind
        .parse()
        .with_context(|| format!("invalid bind address: {bind}"))?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let options = HostOptions {
        script_path: env::var_os("KITU_ARENA_SCRIPT").map(PathBuf::from),
        content_path: env::var_os("KITU_ARENA_CONTENT")
            .or_else(|| env::var_os("KITU_ARENA_TMD"))
            .map(PathBuf::from)
            .unwrap_or_else(|| HostOptions::default().content_path),
        run_directory: env::var_os("KITU_ARENA_RUN_DIRECTORY")
            .map(PathBuf::from)
            .unwrap_or_else(|| HostOptions::default().run_directory),
        recording_directory: env::var_os("KITU_ARENA_RECORDING_DIRECTORY")
            .map(PathBuf::from)
            .unwrap_or_else(|| HostOptions::default().recording_directory),
        persist_runs: true,
        io_runtime: Some(tokio::runtime::Handle::current()),
        bridge_endpoint: Some(format!("http://{}", listener.local_addr()?)),
        ..HostOptions::default()
    };
    let mut host = ArenaHost::new(build_arena_runtime()?, options)?;
    let app = host.router();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs_f64(1.0 / 60.0));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Burst);
        loop {
            interval.tick().await;
            if let Err(error) = host.tick() {
                broadcast_error(&host.state, format!("runtime tick failed: {error:#}"));
            }
        }
    });
    info!("kitu demo game admin host listening on http://{addr}");
    axum::serve(listener, app).await?;
    Ok(())
}

#[derive(Clone)]
struct AppState {
    inner: Arc<Mutex<GameState>>,
    events: broadcast::Sender<ServerEvent>,
    content: Arc<content::Service>,
    script: Arc<script::Service>,
    shell: Arc<shell::Service>,
    work: Arc<work::Work>,
    playback_operations: Arc<tokio::sync::Mutex<()>>,
    verification_slots: Arc<tokio::sync::Semaphore>,
    options: Arc<HostOptions>,
}

struct GameState {
    runtime: DemoRuntime,
    recorder: crate::replay::Recorder,
    recording_error: Option<String>,
    playback: Option<playback::Playback>,
    pending_playback: Option<playback::Playback>,
    playback_generation: u64,
    run_events: Vec<ServerEvent>,
    live_receipts: std::collections::BTreeMap<u64, serde_json::Value>,
    next_log_id: u64,
    logs: Vec<DebugLogEntry>,
    runtime_id: String,
    next_connection_id: u64,
    controller: Option<(u64, String)>,
    controls: std::collections::VecDeque<playback::Control>,
}

impl GameState {
    fn from_runtime(runtime: DemoRuntime) -> Result<Self> {
        anyhow::ensure!(
            runtime.current_tick().get() == 0,
            "host requires an unstarted Runtime"
        );
        let recorder = crate::replay::Recorder::new(&runtime)?;
        Ok(Self {
            runtime,
            recorder,
            recording_error: None,
            playback: None,
            pending_playback: None,
            playback_generation: 0,
            run_events: Vec::new(),
            live_receipts: std::collections::BTreeMap::new(),
            next_log_id: 1,
            logs: Vec::new(),
            runtime_id: format!(
                "{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)?
                    .as_nanos()
            ),
            next_connection_id: 1,
            controller: None,
            controls: std::collections::VecDeque::new(),
        })
    }

    fn snapshot(&self) -> WorldSnapshot {
        if let Some(playback) = &self.playback {
            return playback.world.clone();
        }
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

    fn ensure_live_input(&self) -> Result<()> {
        anyhow::ensure!(
            self.playback.is_none() && self.pending_playback.is_none(),
            "replay is read-only; return to live mode before sending game inputs"
        );
        Ok(())
    }

    fn observed_runtime(&self) -> &DemoRuntime {
        self.playback
            .as_ref()
            .map_or(&self.runtime, |playback| &playback.runtime)
    }

    fn application_projection(&self) -> Vec<kitu_osc_ir::OscBundle> {
        self.playback.as_ref().map_or_else(
            || self.runtime.inspect_application(),
            |playback| playback.projection.clone(),
        )
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

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArenaClientEnvelope {
    schema_version: u32,
    session_id: String,
    client_id: String,
    message_id: u64,
    #[serde(flatten)]
    message: ClientOscMessage,
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
    Replay {
        mode: playback::Mode,
    },
    ArenaSession {
        id: String,
        #[serde(rename = "schemaVersion")]
        schema_version: u32,
    },
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

fn router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/shell/catalog", get(shell::catalog))
        .route("/shell/execute", post(shell::execute))
        .route("/shell/line", post(shell::line))
        .route("/state", get(state_snapshot))
        .route("/logs", get(logs_snapshot))
        .route("/arena/content", get(content::inspect))
        .route("/arena/content/validate", post(content::validate))
        .route("/arena/content/stage", post(content::stage))
        .route("/arena/script", get(script::inspect))
        .route("/arena/script/validate", post(script::validate))
        .route("/arena/script/stage", post(script::stage))
        .route("/arena/recording", get(recording::status))
        .route("/arena/recording/export", get(recording::export))
        .route("/arena/recording/save", post(recording::save))
        .route("/arena/recordings", get(recording::list))
        .route("/arena/recordings/import", post(recording::import))
        .route("/arena/recordings/{id}", get(recording::download))
        .route("/arena/recordings/{id}/verify", post(recording::verify))
        .route("/arena/playback", get(playback::status))
        .route("/arena/playback/load", post(playback::load))
        .route("/arena/playback/command", post(playback::command))
        .route("/arena/playback/seek", post(playback::seek))
        .layer(axum::extract::DefaultBodyLimit::max(
            kitu_tsq1::recording::MAX_BYTES,
        ))
        .route("/app-actions", get(app_action_catalog))
        .route("/app-actions/{id}", get(app_action_definition))
        .route("/app-actions/{id}/run", post(run_app_action))
        .route("/ws", get(ws_upgrade))
        .route("/ws/runtime", get(runtime_ws_upgrade))
        .layer(CorsLayer::permissive())
        .with_state(state)
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
    let message = {
        let game = state.inner.lock().map_err(|_| ApiError::state_poisoned())?;
        game.runtime
            .app_action_catalog()
            .materialize_message(&action_id, &request.inputs)
            .map_err(anyhow::Error::from)?
    };
    let result = shell::action(&state, message.clone()).await?;
    if result["receipt"]["accepted"] == false {
        return Err(ApiError::bad_request(
            result["receipt"]["code"]
                .as_str()
                .unwrap_or("action rejected"),
        ));
    }
    Ok(Json(ActionRunResponse {
        action_id,
        osc: ClientOscMessage::from(message),
        snapshot: snapshot(&state)?,
    }))
}

async fn ws_upgrade(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| observer_lifetime(socket, state, false))
}

async fn runtime_ws_upgrade(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| observer_lifetime(socket, state, true))
}

async fn observer_lifetime(socket: WebSocket, state: AppState, runtime: bool) {
    let Ok(_work) = state.work.enter() else {
        return;
    };
    let mut shutdown = state.work.subscribe();
    if *shutdown.borrow() {
        return;
    }
    // Cancel even a socket send blocked by a non-reading observer. Dropping the
    // complete socket future releases its handle before native library unload.
    tokio::select! {
        _ = shutdown.changed() => {},
        _ = async { if runtime { runtime_ws_loop(socket, state).await } else { ws_loop(socket, state).await } } => {},
    }
}

async fn ws_loop(mut socket: WebSocket, state: AppState) {
    let Ok(_work) = state.work.enter() else {
        return;
    };
    let mut shutdown = state.work.subscribe();
    if *shutdown.borrow() {
        return;
    }
    let initial = tokio::select! {
        result = send_initial_state(&mut socket, &state) => result,
        _ = shutdown.changed() => return,
    };
    if let Err(err) = initial {
        error!("failed to send initial state: {err}");
        return;
    }

    let mut receiver = state.events.subscribe();
    let mut output_mode = WsOutputMode::Json;

    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
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
    let Ok(_work) = state.work.enter() else {
        return;
    };
    let mut shutdown = state.work.subscribe();
    if *shutdown.borrow() {
        return;
    }
    let connection_id = {
        let Ok(mut guard) = state.inner.lock() else {
            return;
        };
        let id = guard.next_connection_id;
        guard.next_connection_id += 1;
        id
    };
    let initial = tokio::select! {
        result = send_initial_runtime_state(&mut socket, &state, WsOutputMode::Json) => result,
        _ = shutdown.changed() => return,
    };
    if let Err(err) = initial {
        error!("failed to send initial runtime state: {err}");
        return;
    }
    let mut receiver = state.events.subscribe();
    let mut output_mode = WsOutputMode::Json;
    loop {
        tokio::select! {
            _ = shutdown.changed() => break,
            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        let result = match serde_json::from_str::<ClientOscMessage>(&text) {
                            Ok(message) if message.address.starts_with("/input/arena/") => {
                                match serde_json::from_str::<ArenaClientEnvelope>(&text) {
                                    Ok(envelope) => enqueue_arena_request(&state, connection_id, envelope),
                                    Err(error) => Err(error.into()),
                                }
                            }
                            Ok(message) => handle_runtime_osc(&state, message),
                            Err(error) => Err(error.into()),
                        };
                        if let Err(error) = result {
                            let _ = send_event(&mut socket, &ServerEvent::Error { message: error.to_string() }).await;
                        }
                    }
                    Some(Ok(Message::Binary(bytes))) => {
                        output_mode = WsOutputMode::Kep;
                        let result = decode_kep_osc_message(&bytes).and_then(|message| {
                            anyhow::ensure!(!message.address.starts_with("/input/arena/"), "Arena requires its versioned JSON envelope in this stage");
                            handle_runtime_osc_message(&state, message)
                        });
                        if let Err(error) = result { broadcast_error(&state, error.to_string()); }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Err(error)) => { error!("runtime websocket receive error: {error}"); break; }
                    Some(Ok(_)) => {}
                }
            }
            event = receiver.recv() => {
                match event {
                    Ok(event) => {
                        if send_event_with_mode(&mut socket, &event, output_mode).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        // A complete projection replaces dropped transient presentation updates.
                        if send_initial_runtime_state(&mut socket, &state, output_mode).await.is_err() { break; }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    release_arena_controller(&state, connection_id);
}

fn enqueue_arena_request(
    state: &AppState,
    connection_id: u64,
    envelope: ArenaClientEnvelope,
) -> Result<()> {
    state.work.check()?;
    anyhow::ensure!(
        !state.options.external_controller,
        "embedded Runtime websocket connections are observers; native input owns control"
    );
    let message = envelope.message.to_osc_message();
    anyhow::ensure!(
        message.address != "/input/arena/disconnect",
        "disconnect is a host-originated control"
    );
    anyhow::ensure!(
        !envelope.client_id.starts_with("host:"),
        "reserved producer identity"
    );
    let metadata = InputMetadata {
        source: envelope.client_id.clone(),
        message_id: envelope.message_id,
        schema_version: envelope.schema_version,
    };
    arena::validate_input(&message, &metadata)?;
    let mut guard = state
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    anyhow::ensure!(
        envelope.session_id == guard.runtime_id,
        "Arena runtime session changed; synchronize before sending input"
    );
    guard.ensure_live_input()?;
    if let Some((owner, source)) = &guard.controller {
        anyhow::ensure!(
            *owner == connection_id && source == &envelope.client_id,
            "Arena already has an active controller"
        );
    } else {
        guard.controller = Some((connection_id, envelope.client_id));
    }
    let mut bundle = kitu_osc_ir::OscBundle::new();
    bundle.push(message);
    guard.runtime.try_enqueue_input(bundle, Some(metadata))?;
    Ok(())
}

fn release_arena_controller(state: &AppState, connection_id: u64) {
    let Ok(mut guard) = state.inner.lock() else {
        return;
    };
    if guard.controller.as_ref().map(|(id, _)| *id) != Some(connection_id) {
        return;
    }
    guard.controller = None;
    let mut bundle = kitu_osc_ir::OscBundle::new();
    bundle.push(OscMessage::new("/input/arena/disconnect"));
    guard.runtime.enqueue_tagged_input(
        bundle,
        InputMetadata {
            source: "host:controller".into(),
            message_id: connection_id,
            schema_version: arena::SCHEMA_VERSION,
        },
    );
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

async fn send_initial_runtime_state(
    socket: &mut WebSocket,
    state: &AppState,
    mode: WsOutputMode,
) -> Result<()> {
    // Take one coherent snapshot before any socket await: a concurrent seek or
    // live/replay switch must not mix mode, tick and projection from different runs.
    let initial = {
        let game = state
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        let mut events = vec![
            ServerEvent::Connected {
                protocol: "kitu-runtime-osc-ir-json-v1",
                tick: game.snapshot().tick,
            },
            ServerEvent::ArenaSession {
                id: game.runtime_id.clone(),
                schema_version: arena::SCHEMA_VERSION,
            },
        ];
        events.extend(playback::events(&game, game.application_projection()));
        events
    };
    for event in initial {
        send_event_with_mode(socket, &event, mode).await?;
    }
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
    state.work.check()?;
    anyhow::ensure!(
        !state.options.external_controller,
        "embedded websocket is read-only; use the HTTP Shell for operator commands"
    );
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
    state.work.check()?;
    anyhow::ensure!(
        !state.options.external_controller,
        "embedded websocket is read-only; use the HTTP Shell for operator commands"
    );
    let events = run_runtime_osc_request(state, osc_message)?;
    for event in events {
        let _ = state.events.send(event);
    }
    Ok(())
}

fn run_runtime_osc_request(state: &AppState, osc_message: OscMessage) -> Result<Vec<ServerEvent>> {
    state.work.check()?;
    anyhow::ensure!(
        !osc_message.address.starts_with("/input/arena/"),
        "Arena input requires the versioned /ws/runtime connection"
    );
    let mut bundle = kitu_osc_ir::OscBundle::new();
    bundle.push(osc_message.clone());

    let mut guard = state
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let mut outgoing_events = Vec::new();

    guard.ensure_live_input()?;
    guard.runtime.try_enqueue_input(bundle, None)?;
    outgoing_events.push(ServerEvent::Log {
        entry: guard.push_log(
            LogLevel::Info,
            format!("runtime input {}", osc_message.to_debug_string()?),
            Some(osc_message.address),
        ),
    });

    Ok(outgoing_events)
}

struct TickResult {
    output: Vec<OscBundle>,
    events: Vec<ServerEvent>,
    run_events: Vec<ServerEvent>,
}

fn advance_tick(state: &AppState) -> Result<TickResult> {
    let mut guard = state
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let control = playback::apply_controls(&mut guard);
    let output = if let Some(ready) = guard.pending_playback.take() {
        if guard.playback.is_none() {
            advance_live_tick(&mut guard)?;
        }
        guard.playback = Some(ready);
        guard.playback.as_mut().unwrap().take_outputs()
    } else if let Some(playback) = guard.playback.as_mut() {
        playback.advance();
        playback.take_outputs()
    } else {
        advance_live_tick(&mut guard)?
    };
    playback::complete_control(&guard, control);
    let events = playback::events(&guard, output.clone());
    let run_events = std::mem::take(&mut guard.run_events);
    Ok(TickResult {
        output,
        events,
        run_events,
    })
}

#[cfg(test)]
fn advance_runtime_tick(state: &AppState) -> Result<Vec<ServerEvent>> {
    let result = advance_tick(state)?;
    // Legacy unit assertions inspect the detached persistence queue explicitly.
    state
        .inner
        .lock()
        .unwrap()
        .run_events
        .extend(result.run_events);
    Ok(result.events)
}

fn advance_live_tick(guard: &mut GameState) -> Result<Vec<kitu_osc_ir::OscBundle>> {
    guard.runtime.tick_once().context("tick Kitu runtime")?;
    let outputs = guard.runtime.drain_output_buffer();
    if guard.recording_error.is_none() {
        let GameState {
            runtime, recorder, ..
        } = &mut *guard;
        if let Err(error) = recorder.capture(runtime, &outputs) {
            guard.recording_error = Some(format!("{error:#}"));
        }
    }
    shell::capture_receipts(guard, &outputs);
    // Persist live starts even when this tick activates replay; historical run
    // events are presentation output and must never overwrite live manifests.
    for message in outputs
        .iter()
        .flat_map(|bundle| &bundle.messages)
        .filter(|message| message.address == "/game/arena/run")
    {
        guard.run_events.push(ServerEvent::Osc {
            address: message.address.clone(),
            args: message
                .args
                .clone()
                .into_iter()
                .map(JsonOscArg::from)
                .collect(),
        });
    }
    Ok(outputs)
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

    guard.ensure_live_input()?;
    let message = guard
        .runtime
        .app_action_catalog()
        .materialize_message(&action_id, &inputs)?;
    anyhow::ensure!(
        !message.address.starts_with("/input/arena/"),
        "Arena actions require the versioned Shell or HTTP action endpoint"
    );
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
            Json(serde_json::json!({ "error": format!("{:#}", self.0) })),
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

    pub(super) fn test_state() -> AppState {
        ArenaHost::new(build_arena_runtime().unwrap(), HostOptions::default())
            .unwrap()
            .state
    }

    pub(super) fn arena_request(state: &AppState, id: u64, address: &str) -> ArenaClientEnvelope {
        ArenaClientEnvelope {
            schema_version: arena::SCHEMA_VERSION,
            session_id: state.inner.lock().unwrap().runtime_id.clone(),
            client_id: "test-controller".into(),
            message_id: id,
            message: ClientOscMessage {
                address: address.into(),
                args: vec![],
            },
        }
    }

    fn arena_projection(state: &AppState) -> arena::ArenaState {
        let guard = state.inner.lock().unwrap();
        let projection = guard.runtime.inspect_application();
        let OscArg::Str(json) = &projection[0].messages[0].args[0] else {
            panic!("state JSON");
        };
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn arena_admission_never_advances_time_and_disconnect_requires_resume() {
        let state = test_state();
        for _ in 0..20 {
            enqueue_arena_request(&state, 1, arena_request(&state, 1, "/input/arena/start"))
                .unwrap();
        }
        assert_eq!(arena_projection(&state).simulation_steps, 0);
        for _ in 0..60 {
            advance_runtime_tick(&state).unwrap();
        }
        assert_eq!(arena_projection(&state).simulation_steps, 60);
        let before = arena_projection(&state).elapsed;
        release_arena_controller(&state, 99); // An observer cannot pause the controller.
        assert!(state.inner.lock().unwrap().controller.is_some());
        release_arena_controller(&state, 1);
        advance_runtime_tick(&state).unwrap();
        assert_eq!(arena_projection(&state).overlay, "pause");
        assert_eq!(arena_projection(&state).elapsed, before);
        // Reconnect can synchronize and issue commands, but only resume runs time.
        enqueue_arena_request(&state, 2, arena_request(&state, 2, "/input/arena/pause")).unwrap();
        advance_runtime_tick(&state).unwrap();
        assert_eq!(arena_projection(&state).elapsed, before);
        enqueue_arena_request(&state, 2, arena_request(&state, 3, "/input/arena/resume")).unwrap();
        advance_runtime_tick(&state).unwrap();
        assert_eq!(arena_projection(&state).simulation_steps, 61);
    }

    #[test]
    fn invalid_or_competing_arena_envelopes_do_not_claim_or_poison_the_queue() {
        let state = test_state();
        let mut request = arena_request(&state, 1, "/input/arena/start");
        request.session_id = "stale runtime".into();
        assert!(enqueue_arena_request(&state, 1, request).is_err());
        assert!(state.inner.lock().unwrap().controller.is_none());
        assert!(
            enqueue_arena_request(&state, 1, arena_request(&state, 1, "/input/arena/take"))
                .is_err()
        );
        assert!(
            enqueue_arena_request(&state, 1, arena_request(&state, 1, "/input/arena/unknown"))
                .is_err()
        );
        assert!(state.inner.lock().unwrap().controller.is_none());
        enqueue_arena_request(&state, 1, arena_request(&state, 1, "/input/arena/start")).unwrap();
        assert!(
            enqueue_arena_request(&state, 2, arena_request(&state, 2, "/input/arena/menu"))
                .is_err()
        );
        let mut malformed = arena_request(&state, 3, "/input/arena/frame");
        malformed.message.args.push(JsonOscArg::Float(f32::NAN));
        assert!(enqueue_arena_request(&state, 1, malformed).is_err());
        assert!(handle_client_osc_message(&state, OscMessage::new("/input/arena/menu")).is_err());
        advance_runtime_tick(&state).unwrap();
        assert_eq!(arena_projection(&state).phase, 1);
        assert_eq!(arena_projection(&state).simulation_steps, 1);
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

        let admitted = run_runtime_osc_request(&state, request.to_osc_message()).unwrap();
        assert!(admitted
            .iter()
            .all(|event| matches!(event, ServerEvent::Log { .. })));
        assert_eq!(state.inner.lock().unwrap().runtime.current_tick().get(), 0);
        let events = advance_runtime_tick(&state).unwrap();
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
        for event in advance_runtime_tick(&state).unwrap() {
            let _ = state.events.send(event);
        }

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

    #[test]
    fn embedded_owner_preserves_output_bundles_and_observers_cannot_pause_it() {
        let mut host = ArenaHost::new(
            build_arena_runtime().unwrap(),
            HostOptions {
                external_controller: true,
                bridge_endpoint: Some("http://127.0.0.1:9000".into()),
                ..HostOptions::default()
            },
        )
        .unwrap();
        let mut direct = build_arena_runtime().unwrap();
        let _router = host.router();
        let before = host.inspect().unwrap();
        let bundle = OscBundle {
            messages: vec![OscMessage::new("/input/arena/start")],
        };
        let identity = InputMetadata {
            source: "native".into(),
            message_id: 1,
            schema_version: 1,
        };
        assert_eq!(
            host.submit(bundle.clone(), Some(identity.clone())).unwrap(),
            direct.try_enqueue_input(bundle, Some(identity)).unwrap()
        );
        assert_eq!(
            host.inspect().unwrap(),
            before,
            "router construction and admission have no clock"
        );
        direct.tick_once().unwrap();
        assert_eq!(host.tick().unwrap(), direct.drain_output_buffer());
        assert_eq!(host.inspect().unwrap(), direct.inspect_application());
        assert_eq!(host.state.inner.lock().unwrap().recorder.ticks(), 1);
        assert!(enqueue_arena_request(
            &host.state,
            1,
            arena_request(&host.state, 1, "/input/arena/menu")
        )
        .is_err());
        assert!(
            handle_client_osc_message(&host.state, OscMessage::new("/admin/world/reset")).is_err()
        );
        release_arena_controller(&host.state, 1);
        direct.tick_once().unwrap();
        assert_eq!(host.tick().unwrap(), direct.drain_output_buffer());
        let metadata = host.inspect_host().unwrap();
        let [OscArg::Str(json)] = metadata[0].messages[0].args.as_slice() else {
            panic!("host metadata");
        };
        let status: serde_json::Value = serde_json::from_str(json).unwrap();
        assert_eq!(
            status["sessionId"],
            host.state.inner.lock().unwrap().runtime_id
        );
        assert_eq!(status["playbackMode"]["active"], false);
        assert_eq!(status["readOnly"], false);
        assert_eq!(status["bridgeEndpoint"], "http://127.0.0.1:9000");
        host.begin_shutdown();
        assert!(host.tick().is_err());
        assert!(host.submit(OscBundle::new(), None).is_err());
    }

    #[test]
    fn native_tick_persists_on_explicit_io_runtime_outside_any_tokio_context() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .unwrap();
        let directory = std::env::temp_dir().join(format!(
            "arena-native-host-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let mut host = ArenaHost::new(
            build_arena_runtime().unwrap(),
            HostOptions {
                external_controller: true,
                persist_runs: true,
                io_runtime: Some(runtime.handle().clone()),
                run_directory: directory.clone(),
                ..HostOptions::default()
            },
        )
        .unwrap();
        assert!(tokio::runtime::Handle::try_current().is_err());
        host.submit(
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
        host.tick().unwrap();
        let session = host.state.inner.lock().unwrap().runtime_id.clone();
        host.begin_shutdown();
        runtime.block_on(host.wait_shutdown());
        let manifest: serde_json::Value =
            serde_json::from_slice(&std::fs::read(directory.join(session).join("1.json")).unwrap())
                .unwrap();
        assert_eq!(manifest["start"]["run"], 1);
        std::fs::remove_dir_all(directory).unwrap();
    }

    #[tokio::test]
    async fn shutdown_cancels_and_joins_worker_ownership_and_storage_can_be_disabled() {
        let host = ArenaHost::new(
            build_arena_runtime().unwrap(),
            HostOptions {
                content_path: PathBuf::new(),
                recording_directory: PathBuf::new(),
                ..HostOptions::default()
            },
        )
        .unwrap();
        let work = host.state.work.clone();
        let (started, ready) = tokio::sync::oneshot::channel();
        let worker = host
            .state
            .spawn_blocking(move || {
                started.send(()).unwrap();
                loop {
                    work.check()?;
                    std::thread::yield_now();
                }
                #[allow(unreachable_code)]
                Ok::<(), anyhow::Error>(())
            })
            .unwrap();
        ready.await.unwrap();
        let error = recording::save(State(host.state.clone()))
            .await
            .err()
            .unwrap();
        assert!(error.0.to_string().contains("storage is disabled"));
        let error = recording::list(State(host.state.clone()))
            .await
            .err()
            .unwrap();
        assert!(error.0.to_string().contains("storage is disabled"));
        host.begin_shutdown();
        tokio::time::timeout(std::time::Duration::from_secs(1), host.wait_shutdown())
            .await
            .unwrap();
        assert!(worker
            .await
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("shutting down"));
        assert!(host.state.spawn_blocking(|| Ok(())).is_err());
    }
}
