//! Live operator commands: shared grammar, bounded idempotency and ordinary tick admission.
use super::*;
use kitu_shell::{CommandRequest, CommandResponse, HostCatalog, COMMAND_VERSION};
use serde_json::{json, Value};

const MAX_COMMANDS: usize = 512;
#[derive(Default)]
pub(super) struct Service {
    book: Mutex<Book>,
}
#[derive(Default)]
struct Book {
    entries: HashMap<(String, u64), Entry>,
    high_water: HashMap<String, u64>,
}
struct Entry {
    args: Vec<String>,
    result: tokio::sync::watch::Receiver<Option<CommandResponse>>,
}

pub(super) async fn catalog(State(state): State<AppState>) -> Result<Json<HostCatalog>, ApiError> {
    let session_id = state
        .inner
        .lock()
        .map_err(|_| ApiError::state_poisoned())?
        .runtime_id
        .clone();
    Ok(Json(HostCatalog {
        version: COMMAND_VERSION,
        session_id,
        commands: kitu_shell::command_catalog(),
    }))
}
pub(super) async fn execute(
    State(state): State<AppState>,
    Json(request): Json<CommandRequest>,
) -> Json<CommandResponse> {
    let id = request.id;
    Json(
        handle(state, request)
            .await
            .unwrap_or_else(|e| CommandResponse {
                id,
                ok: false,
                data: Value::Null,
                error: Some(format!("{e:#}")),
            }),
    )
}
async fn handle(state: AppState, request: CommandRequest) -> Result<CommandResponse> {
    state.work.check()?;
    anyhow::ensure!(
        request.version == COMMAND_VERSION,
        "incompatible Shell command version"
    );
    anyhow::ensure!(
        request.id > 0
            && !request.client_id.is_empty()
            && request.client_id.len() <= 64
            && request
                .client_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "client ID must be 1..64 ASCII letters/digits/-/_ and command ID must be positive"
    );
    anyhow::ensure!(
        request.session_id
            == state
                .inner
                .lock()
                .map_err(|_| anyhow::anyhow!("state lock poisoned"))?
                .runtime_id,
        "host session changed; reconnect before sending commands"
    );
    kitu_shell::resolve_command(&request.args).map_err(anyhow::Error::msg)?;
    let mut receiver = {
        let mut book = state
            .shell
            .book
            .lock()
            .map_err(|_| anyhow::anyhow!("Shell lock poisoned"))?;
        let key = (request.client_id.clone(), request.id);
        if let Some(entry) = book.entries.get(&key) {
            anyhow::ensure!(
                entry.args == request.args,
                "command ID conflicts with an earlier payload"
            );
            entry.result.clone()
        } else {
            anyhow::ensure!(
                book.entries.len() < MAX_COMMANDS,
                "Shell capacity of 512 commands reached; start a fresh host session"
            );
            anyhow::ensure!(
                request.id > *book.high_water.get(&request.client_id).unwrap_or(&0),
                "stale command ID"
            );
            let (sender, receiver) = tokio::sync::watch::channel(None);
            book.high_water
                .insert(request.client_id.clone(), request.id);
            book.entries.insert(
                key,
                Entry {
                    args: request.args.clone(),
                    result: receiver.clone(),
                },
            );
            let worker = state.clone();
            state.spawn(async move {
                let source = format!("shell:{}:{}", request.client_id, request.id);
                let result = run(&worker, &request.args, &source).await;
                let response = match result {
                    Ok(data) => {
                        let mut refusal = data
                            .get("receipt")
                            .filter(|r| r["accepted"] == false)
                            .map(|r| r["code"].as_str().unwrap_or("command rejected").to_owned());
                        if request.args == ["content", "validate"] {
                            if let Some(diagnostics) =
                                data["diagnostics"].as_array().filter(|d| !d.is_empty())
                            {
                                refusal = Some(
                                    diagnostics
                                        .iter()
                                        .filter_map(Value::as_str)
                                        .collect::<Vec<_>>()
                                        .join("; "),
                                );
                            }
                        }
                        CommandResponse {
                            id: request.id,
                            ok: refusal.is_none(),
                            data,
                            error: refusal,
                        }
                    }
                    Err(error) => CommandResponse {
                        id: request.id,
                        ok: false,
                        data: Value::Null,
                        error: Some(format!("{error:#}")),
                    },
                };
                // Work and cached results survive an HTTP client disconnect.
                let _ = sender.send(Some(response));
            })?;
            receiver
        }
    };
    loop {
        if let Some(response) = receiver.borrow().clone() {
            return Ok(response);
        }
        receiver
            .changed()
            .await
            .context("command worker stopped before returning a result")?;
    }
}
fn exactly(args: &[String], n: usize, usage: &str) -> Result<()> {
    anyhow::ensure!(args.len() == n, "expected {usage}");
    Ok(())
}
fn value<T: Serialize>(json: Json<T>) -> Result<Value> {
    Ok(serde_json::to_value(json.0)?)
}
fn application(game: &GameState) -> Result<Value> {
    let projection = game.runtime.inspect_application();
    let OscArg::Str(json) = &projection[0].messages[0].args[0] else {
        anyhow::bail!("missing application projection")
    };
    Ok(serde_json::from_str(json)?)
}
async fn run(state: &AppState, args: &[String], source: &str) -> Result<Value> {
    let (spec, rest) = kitu_shell::resolve_command(args).map_err(anyhow::Error::msg)?;
    match spec.prefix.join(" ").as_str() {
        "help" => {
            exactly(rest, 0, &spec.usage)?;
            value(catalog(State(state.clone())).await.map_err(|e| e.0)?)
        }
        "inspect" => {
            exactly(rest, 1, &spec.usage)?;
            match rest[0].as_str() {
                "application" | "replay" => value(
                    playback::status(State(state.clone()))
                        .await
                        .map_err(|e| e.0)?,
                ),
                "world" => Ok(serde_json::to_value(snapshot(state)?)?),
                "content" => value(
                    content::inspect(State(state.clone()))
                        .await
                        .map_err(|e| e.0)?,
                ),
                "recording" => value(
                    recording::status(State(state.clone()))
                        .await
                        .map_err(|e| e.0)?,
                ),
                _ => anyhow::bail!("unknown inspection target"),
            }
        }
        "osc send" => {
            anyhow::ensure!(!rest.is_empty(), "expected {}", spec.usage);
            submit(
                state,
                kitu_shell::parse_osc(&rest[0], &rest[1..]).map_err(anyhow::Error::msg)?,
                source,
                1,
            )
            .await
        }
        "app action list" => {
            exactly(rest, 0, &spec.usage)?;
            value(
                app_action_catalog(State(state.clone()))
                    .await
                    .map_err(|e| e.0)?,
            )
        }
        "app action describe" => {
            exactly(rest, 1, &spec.usage)?;
            value(
                app_action_definition(Path(rest[0].clone()), State(state.clone()))
                    .await
                    .map_err(|e| e.0)?,
            )
        }
        "app action run" => {
            anyhow::ensure!(!rest.is_empty(), "expected {}", spec.usage);
            let message = materialize(state, &rest[0], &rest[1..])?;
            submit(state, message, source, 1).await
        }
        "content validate" => {
            exactly(rest, 0, &spec.usage)?;
            value(
                content::validate(State(state.clone()))
                    .await
                    .map_err(|e| e.0)?,
            )
        }
        "content stage" => {
            exactly(rest, 2, &spec.usage)?;
            let response = content::stage_candidate(
                state,
                content::StageRequest {
                    hash: rest[0].clone(),
                    source_sha256: rest[1].clone(),
                },
            )?;
            let result = serde_json::to_value(response)?;
            let sequence = result["sequence"]
                .as_u64()
                .context("missing input sequence")?;
            wait_receipt(state, sequence, true).await
        }
        "replay list" => {
            exactly(rest, 0, &spec.usage)?;
            value(
                recording::list(State(state.clone()))
                    .await
                    .map_err(|e| e.0)?,
            )
        }
        "replay save" => {
            exactly(rest, 0, &spec.usage)?;
            value(
                recording::save(State(state.clone()))
                    .await
                    .map_err(|e| e.0)?,
            )
        }
        "replay verify" => {
            exactly(rest, 1, &spec.usage)?;
            value(
                recording::verify(State(state.clone()), Path(rest[0].clone()))
                    .await
                    .map_err(|e| e.0)?,
            )
        }
        "replay load" => {
            exactly(rest, 1, &spec.usage)?;
            let _ = playback::load(
                State(state.clone()),
                Json(playback::LoadRequest {
                    id: rest[0].clone(),
                }),
            )
            .await
            .map_err(|e| e.0)?;
            wait_activation(state, &rest[0]).await?;
            value(
                playback::status(State(state.clone()))
                    .await
                    .map_err(|e| e.0)?,
            )
        }
        "replay seek" => {
            exactly(rest, 1, &spec.usage)?;
            value(
                playback::seek(
                    State(state.clone()),
                    Json(playback::SeekRequest {
                        tick: rest[0].parse().context("tick must be an integer")?,
                    }),
                )
                .await
                .map_err(|e| e.0)?,
            )
        }
        "replay play" | "replay pause" | "replay step" | "replay stop" | "replay live" => {
            exactly(rest, 0, &spec.usage)?;
            let before = {
                let game = state
                    .inner
                    .lock()
                    .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
                playback::mode(&game)
            };
            let _ = playback::command(
                State(state.clone()),
                Json(playback::CommandRequest {
                    action: spec.prefix[1].clone(),
                }),
            )
            .await
            .map_err(|e| e.0)?;
            if spec.prefix[1] == "step" {
                tokio::time::timeout(std::time::Duration::from_secs(3), async {
                    loop {
                        let mode = {
                            let game = state
                                .inner
                                .lock()
                                .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
                            playback::mode(&game)
                        };
                        anyhow::ensure!(
                            mode.active && mode.recording_id == before.recording_id,
                            "replay changed before step completed"
                        );
                        if mode.tick > before.tick {
                            break Ok::<_, anyhow::Error>(());
                        }
                        if let Some(error) = mode.error {
                            anyhow::bail!(error);
                        }
                        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                    }
                })
                .await
                .context("step timed out; inspect replay before continuing")??;
            }
            value(
                playback::status(State(state.clone()))
                    .await
                    .map_err(|e| e.0)?,
            )
        }
        "scenario list" => {
            exactly(rest, 0, &spec.usage)?;
            Ok(serde_json::from_str(include_str!(
                "../../content/arena-scenarios.json"
            ))?)
        }
        "scenario run" => {
            exactly(rest, 1, &spec.usage)?;
            run_scenario(state, &rest[0], source).await
        }
        _ => anyhow::bail!("command has no host implementation"),
    }
}
fn materialize(state: &AppState, id: &str, args: &[String]) -> Result<OscMessage> {
    let game = state
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let catalog = game.runtime.app_action_catalog();
    let action = catalog.action(id).context("unknown app action")?;
    let mut inputs = HashMap::new();
    for raw in args {
        let (name, raw) = raw
            .split_once('=')
            .context("action arguments use name=value")?;
        let input = action
            .inputs
            .iter()
            .find(|s| s.name == name)
            .with_context(|| format!("unknown action input {name}"))?;
        anyhow::ensure!(!inputs.contains_key(name), "duplicate action input {name}");
        inputs.insert(
            name.into(),
            ActionValue::parse_cli_value(name, raw, input.value_type)?,
        );
    }
    Ok(catalog.materialize_message(id, &inputs)?)
}
async fn submit(state: &AppState, message: OscMessage, source: &str, id: u64) -> Result<Value> {
    state.work.check()?;
    anyhow::ensure!(
        message.address != "/input/arena/disconnect",
        "disconnect is owned by the controller connection"
    );
    let arena_input = message.address.starts_with("/input/arena/");
    anyhow::ensure!(
        arena_input
            || message.address == "/input/move"
            || message.address.starts_with("/admin/world/"),
        "this host has no input handler for {}",
        message.address
    );
    let expects_receipt =
        message.address != "/input/move" && message.address != "/input/arena/frame";
    let sequence = {
        let mut game = state
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        game.ensure_live_input()?;
        let mut bundle = kitu_osc_ir::OscBundle::new();
        bundle.push(message);
        game.runtime.try_enqueue_input(
            bundle,
            Some(InputMetadata {
                source: source.into(),
                message_id: id,
                schema_version: arena::SCHEMA_VERSION,
            }),
        )?
    };
    wait_receipt(state, sequence, expects_receipt).await
}
async fn wait_receipt(state: &AppState, sequence: u64, expects_receipt: bool) -> Result<Value> {
    tokio::time::timeout(std::time::Duration::from_secs(3),async {
        loop {
            state.work.check()?;
            {
                let game=state.inner.lock().map_err(|_|anyhow::anyhow!("state lock poisoned"))?;
                if let Some(receipt)=game.live_receipts.get(&sequence) {return Ok(json!({"receipt":receipt,"state":application(&game)?}));}
                if !expects_receipt {
                    if let Some(input)=game.runtime.committed_input_records().into_iter().find(|i|i.sequence==sequence){
                        return Ok(json!({"receipt":{"sequence":input.sequence,"accepted":true,"code":"applied","tick":game.runtime.current_tick().get()-1},"state":application(&game)?}));
                    }
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }).await.context("command outcome timed out; inspect state (command may have applied); retry only the same client/request ID")?
}
/// Capture live outcomes independently of display/replay switching and retain a bounded tail.
pub(super) fn capture_receipts(game: &mut GameState, output: &[kitu_osc_ir::OscBundle]) {
    for message in output.iter().flat_map(|b| &b.messages) {
        let receipt = match (message.address.as_str(), message.args.as_slice()) {
            ("/ui/arena/command", [OscArg::Str(json)]) => serde_json::from_str::<Value>(json).ok(),
            (
                "/ui/kitu/command",
                [OscArg::Int64(sequence), OscArg::Int64(tick), OscArg::Bool(accepted), OscArg::Str(detail)],
            ) => Some(
                json!({"sequence":sequence,"tick":tick,"accepted":accepted,"code":if *accepted{"ok"}else{detail},"detail":detail}),
            ),
            _ => None,
        };
        if let Some(receipt) = receipt {
            if let Some(sequence) = receipt["sequence"].as_u64() {
                game.live_receipts.insert(sequence, receipt);
            }
        }
    }
    // Frames also need a durable application acknowledgment: a fast subsequent
    // tick may replace Runtime's committed batch before a command waiter wakes.
    let tick = game.runtime.current_tick().get() - 1;
    for input in game.runtime.committed_input_records() {
        if input
            .bundle
            .messages
            .iter()
            .any(|m| m.address == "/input/arena/frame" || m.address == "/input/move")
        {
            game.live_receipts.entry(input.sequence).or_insert_with(
                || json!({"sequence":input.sequence,"tick":tick,"accepted":true,"code":"applied"}),
            );
        }
    }
    while game.live_receipts.len() > 1024 {
        game.live_receipts.pop_first();
    }
}
async fn wait_activation(state: &AppState, expected_id: &str) -> Result<()> {
    state.work.check()?;
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            state.work.check()?;
            {
                let game = state
                    .inner
                    .lock()
                    .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
                if game.pending_playback.is_none() {
                    let mode = playback::mode(&game);
                    anyhow::ensure!(
                        mode.active && mode.recording_id.as_deref() == Some(expected_id),
                        "replay load was superseded before activation"
                    );
                    return Ok::<_, anyhow::Error>(());
                }
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .context("replay activation timed out")?
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Scenario {
    id: String,
    description: String,
    steps: Vec<ScenarioStep>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ScenarioStep {
    action: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    wait_ticks: u16,
}
async fn run_scenario(state: &AppState, id: &str, source: &str) -> Result<Value> {
    let scenarios: Vec<Scenario> =
        serde_json::from_str(include_str!("../../content/arena-scenarios.json"))?;
    let scenario = scenarios
        .into_iter()
        .find(|s| s.id == id)
        .context("unknown scenario")?;
    anyhow::ensure!(
        scenario.steps.len() <= 32
            && scenario
                .steps
                .iter()
                .map(|s| u32::from(s.wait_ticks))
                .sum::<u32>()
                <= 600,
        "scenario exceeds 32 steps / 600 waiting ticks"
    );
    let mut steps = Vec::new();
    for (index, step) in scenario.steps.iter().enumerate() {
        let result = submit(
            state,
            materialize(state, &step.action, &step.args)?,
            source,
            index as u64 + 1,
        )
        .await?;
        let rejected = result["receipt"]["accepted"] == false;
        steps.push(result.clone());
        if rejected {
            return Ok(json!({"id":scenario.id,"steps":steps,"receipt":result["receipt"]}));
        }
        if step.wait_ticks > 0 {
            let target = state
                .inner
                .lock()
                .map_err(|_| anyhow::anyhow!("state lock poisoned"))?
                .runtime
                .current_tick()
                .get()
                + u64::from(step.wait_ticks);
            tokio::time::timeout(std::time::Duration::from_secs(15), async {
                loop {
                    state.work.check()?;
                    let tick = {
                        let game = state
                            .inner
                            .lock()
                            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
                        game.ensure_live_input()?;
                        game.runtime.current_tick().get()
                    };
                    if tick >= target {
                        return Ok::<_, anyhow::Error>(());
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
                }
            })
            .await
            .context("scenario wait timed out")??;
        }
    }
    let game = state
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    Ok(
        json!({"id":scenario.id,"description":scenario.description,"steps":steps,"state":application(&game)?}),
    )
}

/// HTTP action forms use the same admission and applied-result path as Shell.
pub(super) async fn action(state: &AppState, message: OscMessage) -> Result<Value> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    submit(state, message, &format!("admin:app-action:{id}"), 1).await
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct LineRequest {
    version: u32,
    session_id: String,
    client_id: String,
    id: u64,
    line: String,
}
pub(super) async fn line(
    State(state): State<AppState>,
    Json(request): Json<LineRequest>,
) -> Json<CommandResponse> {
    match kitu_shell::parse_line(&request.line) {
        Ok(args) => {
            execute(
                State(state),
                Json(CommandRequest {
                    version: request.version,
                    session_id: request.session_id,
                    client_id: request.client_id,
                    id: request.id,
                    args,
                }),
            )
            .await
        }
        Err(error) => Json(CommandResponse {
            id: request.id,
            ok: false,
            data: Value::Null,
            error: Some(error),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn request(state: &AppState, id: u64, line: &str) -> CommandRequest {
        CommandRequest {
            version: COMMAND_VERSION,
            session_id: state.inner.lock().unwrap().runtime_id.clone(),
            client_id: "test-shell".into(),
            id,
            args: kitu_shell::parse_line(line).unwrap(),
        }
    }
    fn clock(state: AppState) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                advance_runtime_tick(&state).unwrap();
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        })
    }
    #[tokio::test]
    async fn live_commands_apply_once_report_refusals_and_replay_exactly() {
        let state = super::super::tests::test_state();
        let ticker = clock(state.clone());
        let start = request(&state, 1, "app action run arena.start");
        let result = handle(state.clone(), start.clone()).await.unwrap();
        assert!(result.ok, "{:?}", result.error);
        assert_eq!(result.data["state"]["phase"], 1);
        let again = handle(state.clone(), start).await.unwrap();
        assert_eq!(
            serde_json::to_value(result).unwrap(),
            serde_json::to_value(again).unwrap()
        );
        assert!(handle(
            state.clone(),
            request(&state, 1, "app action run arena.menu")
        )
        .await
        .is_err());
        let refusal = handle(
            state.clone(),
            request(&state, 2, "app action run arena.start"),
        )
        .await
        .unwrap();
        assert!(!refusal.ok);
        assert_eq!(refusal.data["receipt"]["accepted"], false);
        let spawn = request(
            &state,
            3,
            "app action run spawn-object kind=marker x=1 y=2 z=3",
        );
        assert!(handle(state.clone(), spawn.clone()).await.unwrap().ok);
        assert!(handle(state.clone(), spawn).await.unwrap().ok);
        assert_eq!(
            state
                .inner
                .lock()
                .unwrap()
                .runtime
                .inspect_world_state()
                .objects
                .len(),
            1
        );
        let moved = handle(
            state.clone(),
            request(
                &state,
                4,
                "app action run move-object id=missing x=0 y=0 z=0",
            ),
        )
        .await
        .unwrap();
        assert!(!moved.ok);
        assert!(moved.error.is_some());
        let unknown = handle(
            state.clone(),
            request(&state, 5, "osc send /unsupported i:1"),
        )
        .await
        .unwrap();
        assert!(!unknown.ok);
        let scenario = handle(
            state.clone(),
            request(&state, 6, "scenario run preparation-smoke"),
        )
        .await
        .unwrap();
        assert!(scenario.ok, "{:?}", scenario.error);
        assert_eq!(scenario.data["steps"].as_array().unwrap().len(), 4);
        assert_eq!(scenario.data["state"]["phase"], 1);
        assert_eq!(scenario.data["state"]["overlay"], "none");
        ticker.abort();
        let recorder = state.inner.lock().unwrap().recorder.clone();
        let session = crate::replay::Session::decode(&recorder.encode().unwrap()).unwrap();
        let replay = session.verify().unwrap();
        assert_eq!(replay.runs, 2);
        let mut replay_runtime = session.runtime().unwrap();
        for _ in 0..recorder.ticks() {
            session.tick(&mut replay_runtime).unwrap();
        }
        assert_eq!(
            replay_runtime.inspect_world_state(),
            state.inner.lock().unwrap().runtime.inspect_world_state()
        );
    }
    #[tokio::test]
    async fn line_and_argument_clients_share_quoting_identity_and_rejections() {
        let state = super::super::tests::test_state();
        let ticker = clock(state.clone());
        let req = request(&state, 1, "osc send /input/move 's:two words' f:1 f:-0.0");
        let Json(first) = line(
            State(state.clone()),
            Json(LineRequest {
                version: req.version,
                session_id: req.session_id.clone(),
                client_id: req.client_id.clone(),
                id: req.id,
                line: "osc send /input/move 's:two words' f:1 f:-0.0".into(),
            }),
        )
        .await;
        let second = handle(state.clone(), req.clone()).await.unwrap();
        assert!(first.ok);
        assert_eq!(
            serde_json::to_value(first).unwrap(),
            serde_json::to_value(second).unwrap()
        );
        assert_eq!(
            state
                .inner
                .lock()
                .unwrap()
                .runtime
                .inspect_world_state()
                .objects[0]
                .id,
            "two words"
        );
        let mut stale = req;
        stale.session_id = "previous-host".into();
        assert!(handle(state.clone(), stale).await.is_err());
        let session_id = state.inner.lock().unwrap().runtime_id.clone();
        let Json(invalid) = line(
            State(state.clone()),
            Json(LineRequest {
                version: 1,
                session_id,
                client_id: "test-shell".into(),
                id: 2,
                line: "'unfinished".into(),
            }),
        )
        .await;
        assert!(!invalid.ok);
        ticker.abort();
    }
}
