//! Interactive playback uses verified Runtime execution and cached last-valid projections.
use super::*;
use crate::replay::{ExecutionVersion, Session};
use kitu_osc_ir::OscBundle;

type Completion = tokio::sync::oneshot::Sender<Result<()>>;
pub(super) enum Control {
    Action(String, Completion),
    BeginSeek(u64),
    Prepared(u64, Result<Box<Playback>>, Completion),
}

fn pending_seek(game: &GameState) -> bool {
    game.controls
        .iter()
        .any(|control| matches!(control, Control::BeginSeek(_) | Control::Prepared(_, _, _)))
}

pub(super) fn apply_controls(game: &mut GameState) {
    while let Some(control) = game.controls.pop_front() {
        match control {
            Control::Action(action, complete) => {
                let _ = complete.send(operate(game, &action));
            }
            Control::BeginSeek(generation) => {
                if game.playback_generation == generation {
                    if let Some(playback) = game.playback.as_mut() {
                        playback.playing = false;
                        playback.steps = 0;
                        playback.seeking = true;
                    }
                }
            }
            Control::Prepared(generation, prepared, complete) => {
                let result = if game.playback_generation != generation {
                    Err(anyhow::anyhow!("replay changed while seeking"))
                } else {
                    match prepared {
                        Ok(playback) => {
                            game.playback = Some(*playback);
                            Ok(())
                        }
                        Err(error) => {
                            if let Some(playback) = game.playback.as_mut() {
                                playback.seeking = false;
                                playback.error = Some(format!("{error:#}"));
                            }
                            Err(error)
                        }
                    }
                };
                let _ = complete.send(result);
            }
        }
    }
}

async fn wait_control(
    state: &AppState,
    receiver: tokio::sync::oneshot::Receiver<Result<()>>,
) -> Result<()> {
    let mut shutdown = state.work.subscribe();
    state.work.check()?;
    tokio::select! {
        result = tokio::time::timeout(std::time::Duration::from_secs(3), receiver) => result.context("owner tick did not apply playback control")?.context("playback control was dropped")?,
        _ = shutdown.changed() => Err(anyhow::anyhow!("host is shutting down")),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Mode {
    pub active: bool,
    pub recording_id: Option<String>,
    /// Last applied input tick; -1 is the initial state before tick zero.
    pub tick: i64,
    pub total_ticks: u64,
    pub playing: bool,
    pub seeking: bool,
    pub error: Option<String>,
}

pub(super) struct Playback {
    id: String,
    session: Arc<Session>,
    pub runtime: DemoRuntime,
    pub projection: Vec<OscBundle>,
    pub world: WorldSnapshot,
    playing: bool,
    seeking: bool,
    steps: u32,
    error: Option<String>,
    output: Vec<OscBundle>,
}
impl Playback {
    #[cfg(test)]
    fn at(id: String, session: Arc<Session>, tick: i64) -> Result<Self> {
        Self::at_with_cancel(id, session, tick, || Ok(()))
    }
    fn at_with_cancel(
        id: String,
        session: Arc<Session>,
        tick: i64,
        mut check: impl FnMut() -> Result<()>,
    ) -> Result<Self> {
        check()?;
        anyhow::ensure!(
            tick >= -1 && (tick == -1 || tick < session.manifest().ticks as i64),
            "seek tick is outside this recording"
        );
        let mut runtime = session.runtime()?;
        for _ in 0..(tick + 1) as u64 {
            check()?;
            session.tick(&mut runtime)?;
        }
        let projection = runtime.inspect_application();
        let world = world(&runtime);
        Ok(Self {
            id,
            session,
            runtime,
            output: projection.clone(),
            projection,
            world,
            playing: false,
            seeking: false,
            steps: 0,
            error: None,
        })
    }
    pub(super) fn advance(&mut self) {
        if self.seeking || self.error.is_some() || (!self.playing && self.steps == 0) {
            return;
        }
        if self.runtime.current_tick().get() >= self.session.manifest().ticks {
            self.playing = false;
            self.steps = 0;
            return;
        }
        match self.session.tick(&mut self.runtime) {
            Ok(output) => {
                self.output = output;
                self.projection = self.runtime.inspect_application();
                self.world = world(&self.runtime);
                self.steps = self.steps.saturating_sub(1);
                if self.world.tick == self.session.manifest().ticks {
                    self.playing = false;
                    self.steps = 0;
                }
            }
            Err(error) => {
                // Session::tick may have advanced before detecting divergence.
                // Keep showing the last verified state, and require a new seek.
                self.error = Some(format!("{error:#}"));
                self.playing = false;
                self.steps = 0;
            }
        }
    }
    pub(super) fn take_outputs(&mut self) -> Vec<OscBundle> {
        if self.output.is_empty() {
            self.projection.clone()
        } else {
            std::mem::take(&mut self.output)
        }
    }
}
fn world(runtime: &DemoRuntime) -> WorldSnapshot {
    WorldSnapshot {
        tick: runtime.current_tick().get(),
        objects: runtime
            .inspect_world_state()
            .objects
            .iter()
            .map(WorldObject::from_runtime)
            .collect(),
    }
}
pub(super) fn mode(game: &GameState) -> Mode {
    if let Some(p) = game.playback.as_ref() {
        Mode {
            active: true,
            recording_id: Some(p.id.clone()),
            tick: p.world.tick as i64 - 1,
            total_ticks: p.session.manifest().ticks,
            playing: p.playing,
            seeking: p.seeking || game.pending_playback.is_some() || pending_seek(game),
            error: p.error.clone(),
        }
    } else {
        Mode {
            active: false,
            recording_id: None,
            tick: game.runtime.current_tick().get() as i64 - 1,
            total_ticks: 0,
            playing: false,
            seeking: game.pending_playback.is_some(),
            error: None,
        }
    }
}
pub(super) fn events(game: &GameState, outputs: Vec<OscBundle>) -> Vec<ServerEvent> {
    let mut events = vec![ServerEvent::Replay { mode: mode(game) }];
    for message in outputs.into_iter().flat_map(|bundle| bundle.messages) {
        events.push(ServerEvent::Osc {
            address: message.address,
            args: message.args.into_iter().map(JsonOscArg::from).collect(),
        });
    }
    events.push(ServerEvent::State {
        snapshot: game.snapshot(),
    });
    events
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Status {
    mode: Mode,
    state: serde_json::Value,
    live_tick: u64,
    execution: ExecutionVersion,
    content_hash: String,
}
fn inspect(state: &AppState) -> Result<Status> {
    let game = state
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let projection = game.application_projection();
    let OscArg::Str(json) = &projection[0].messages[0].args[0] else {
        anyhow::bail!("missing Arena projection")
    };
    let content = arena::inspect_content(game.observed_runtime())?;
    Ok(Status {
        mode: mode(&game),
        state: serde_json::from_str(json)?,
        live_tick: game.runtime.current_tick().get(),
        execution: ExecutionVersion::current(),
        content_hash: content
            .active
            .as_ref()
            .unwrap_or(&content.pending)
            .hash
            .clone(),
    })
}
pub(super) async fn status(State(state): State<AppState>) -> Result<Json<Status>, ApiError> {
    Ok(Json(inspect(&state)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LoadRequest {
    pub(super) id: String,
}
fn queue_loaded(game: &mut GameState, prepared: Playback) -> Result<()> {
    anyhow::ensure!(
        game.pending_playback.is_none(),
        "replay activation is already queued"
    );
    if game.playback.is_none() {
        let mut bundle = OscBundle::new();
        bundle.push(OscMessage::new("/input/arena/pause"));
        game.runtime.try_enqueue_input(
            bundle,
            Some(InputMetadata {
                source: "host:arena-playback".into(),
                message_id: game.playback_generation,
                schema_version: arena::SCHEMA_VERSION,
            }),
        )?;
    }
    game.pending_playback = Some(prepared);
    Ok(())
}
pub(super) async fn load(
    State(state): State<AppState>,
    Json(request): Json<LoadRequest>,
) -> Result<Json<Status>, ApiError> {
    let operation = state.playback_operations.clone().lock_owned().await;
    state.work.check()?;
    let generation = {
        let mut game = state.inner.lock().map_err(|_| ApiError::state_poisoned())?;
        game.playback_generation += 1;
        game.playback_generation
    };
    let bytes = recording::read(&state, &request.id).await?;
    let work = state.work.clone();
    let prepared = state
        .spawn_blocking(move || {
            let _operation = operation;
            let session = Arc::new(Session::decode(&bytes)?);
            session.verify_with_cancel(|| work.check())?;
            Playback::at_with_cancel(request.id, session, -1, || work.check())
        })?
        .await
        .map_err(anyhow::Error::from)??;
    {
        let mut game = state.inner.lock().map_err(|_| ApiError::state_poisoned())?;
        if game.playback_generation != generation {
            return Err(ApiError::bad_request("replay load was superseded"));
        }
        queue_loaded(&mut game, prepared)?;
    }
    Ok(Json(inspect(&state)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CommandRequest {
    pub(super) action: String,
}
fn operate(game: &mut GameState, action: &str) -> Result<()> {
    if action == "live" {
        game.playback_generation += 1;
        game.pending_playback = None;
        game.playback = None;
        return Ok(());
    }
    anyhow::ensure!(
        game.pending_playback.is_none(),
        "wait for replay activation"
    );
    let p = game.playback.as_mut().context("load a recording first")?;
    anyhow::ensure!(!p.seeking, "seek is in progress");
    match action {
        "pause" => {
            p.playing = false;
            p.steps = 0;
        }
        "play" | "step" => {
            anyhow::ensure!(p.error.is_none(), "seek again after a replay error");
            anyhow::ensure!(
                p.world.tick + u64::from(p.steps) < p.session.manifest().ticks,
                "replay reached the end; seek before continuing"
            );
            if action == "play" {
                p.playing = true;
                p.steps = 0;
            } else {
                p.playing = false;
                p.steps += 1;
            }
        }
        _ => anyhow::bail!("unknown playback action; use play, pause, step, stop or live"),
    }
    Ok(())
}
pub(super) async fn command(
    State(state): State<AppState>,
    Json(request): Json<CommandRequest>,
) -> Result<Json<Status>, ApiError> {
    if request.action == "stop" {
        return seek(State(state), Json(SeekRequest { tick: -1 })).await;
    }
    state.work.check()?;
    let (complete, receiver) = tokio::sync::oneshot::channel();
    {
        let mut game = state.inner.lock().map_err(|_| ApiError::state_poisoned())?;
        if game.controls.len() >= 64 {
            return Err(ApiError::bad_request("playback control queue is full"));
        }
        game.controls
            .push_back(Control::Action(request.action, complete));
    }
    wait_control(&state, receiver).await?;

    Ok(Json(inspect(&state)?))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct SeekRequest {
    pub(super) tick: i64,
}
pub(super) async fn seek(
    State(state): State<AppState>,
    Json(request): Json<SeekRequest>,
) -> Result<Json<Status>, ApiError> {
    let operation = state.playback_operations.clone().lock_owned().await;
    state.work.check()?;
    let (generation, id, session) = {
        let mut game = state.inner.lock().map_err(|_| ApiError::state_poisoned())?;
        if game.pending_playback.is_some() || pending_seek(&game) {
            return Err(ApiError::bad_request("wait for replay activation or seek"));
        }
        let generation = game.playback_generation;
        let playback = game.playback.as_ref().context("load a recording first")?;
        if request.tick < -1
            || (request.tick != -1 && request.tick >= playback.session.manifest().ticks as i64)
        {
            return Err(ApiError::bad_request("seek tick is outside this recording"));
        }
        let id = playback.id.clone();
        let session = playback.session.clone();
        if game.controls.len() >= 64 {
            return Err(ApiError::bad_request("playback control queue is full"));
        }
        game.controls.push_back(Control::BeginSeek(generation));
        (generation, id, session)
    };
    let work = state.work.clone();
    let receiver = spawn_seek(state.clone(), generation, operation, move || {
        Playback::at_with_cancel(id, session, request.tick, || work.check())
    })?
    .await
    .map_err(anyhow::Error::from)??;
    wait_control(&state, receiver).await?;
    Ok(Json(inspect(&state)?))
}

type SeekReceiver = tokio::sync::oneshot::Receiver<Result<()>>;
fn spawn_seek(
    state: AppState,
    generation: u64,
    operation: tokio::sync::OwnedMutexGuard<()>,
    prepare: impl FnOnce() -> Result<Playback> + Send + 'static,
) -> Result<tokio::task::JoinHandle<Result<SeekReceiver>>> {
    let worker = state.clone();
    state.spawn_blocking(move || {
        let _operation = operation;
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(prepare))
            .unwrap_or_else(|_| Err(anyhow::anyhow!("replay seek worker panicked")));
        worker.work.check()?;
        let mut game = worker
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        // Completion commits only in the clock owner's update, even if the HTTP waiter vanished.
        let (complete, receiver) = tokio::sync::oneshot::channel();
        game.controls.push_back(Control::Prepared(
            generation,
            result.map(Box::new),
            complete,
        ));
        Ok(receiver)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay::Recorder;

    async fn with_owner_ticks<T>(
        state: &AppState,
        future: impl std::future::Future<Output = T>,
    ) -> T {
        tokio::pin!(future);
        loop {
            tokio::select! {
                biased;
                result = &mut future => return result,
                _ = tokio::time::sleep(std::time::Duration::from_millis(1)) => { advance_runtime_tick(state).unwrap(); },
            }
        }
    }

    fn session() -> Arc<Session> {
        let mut runtime = build_arena_runtime().unwrap();
        let mut recorder = Recorder::new(&runtime).unwrap();
        let mut start = OscBundle::new();
        start.push(OscMessage::new("/input/arena/start"));
        runtime
            .try_enqueue_input(
                start,
                Some(InputMetadata {
                    source: "replay-test".into(),
                    message_id: 1,
                    schema_version: 1,
                }),
            )
            .unwrap();
        for _ in 0..30 {
            runtime.tick_once().unwrap();
            let output = runtime.drain_output_buffer();
            recorder.capture(&runtime, &output).unwrap();
        }
        let session = Arc::new(Session::decode(&recorder.encode().unwrap()).unwrap());
        session.verify().unwrap();
        session
    }

    #[test]
    fn tick_owner_pauses_live_and_steps_without_accepting_live_mutations() {
        let state = super::super::tests::test_state();
        let saved = session();
        {
            let mut game = state.inner.lock().unwrap();
            game.playback_generation = 1;
            let mut start = OscBundle::new();
            start.push(OscMessage::new("/input/arena/start"));
            game.runtime
                .try_enqueue_input(
                    start,
                    Some(InputMetadata {
                        source: "live".into(),
                        message_id: 1,
                        schema_version: 1,
                    }),
                )
                .unwrap();
            queue_loaded(
                &mut game,
                Playback::at("test".into(), saved.clone(), -1).unwrap(),
            )
            .unwrap();
            assert!(game.ensure_live_input().is_err());
            assert_eq!(game.runtime.current_tick().get(), 0);
            assert!(!mode(&game).active);
            assert!(mode(&game).seeking);
            assert_eq!(mode(&game).tick, game.snapshot().tick as i64 - 1);
        }
        let initial = advance_runtime_tick(&state).unwrap();
        assert!(matches!(&initial[0],ServerEvent::Replay{mode} if mode.active && mode.tick==-1));
        assert_eq!(state.inner.lock().unwrap().runtime.current_tick().get(), 1);
        for _ in 0..4 {
            advance_runtime_tick(&state).unwrap();
        }
        assert_eq!(inspect(&state).unwrap().mode.tick, -1);
        assert_eq!(
            state.inner.lock().unwrap().run_events.len(),
            1,
            "live start queued for persistence during replay activation"
        );
        assert!(
            state
                .inner
                .lock()
                .unwrap()
                .live_receipts
                .values()
                .any(|r| r["source"] == "live" && r["accepted"] == true),
            "command waiters retain live outcomes even when display switches to replay"
        );
        {
            let mut game = state.inner.lock().unwrap();
            operate(&mut game, "step").unwrap();
            assert_eq!(game.playback.as_ref().unwrap().world.tick, 0);
        }
        advance_runtime_tick(&state).unwrap();
        assert_eq!(inspect(&state).unwrap().mode.tick, 0);
        assert_eq!(inspect(&state).unwrap().state["tick"], 0);
        {
            let mut game = state.inner.lock().unwrap();
            operate(&mut game, "play").unwrap();
        }
        for _ in 0..29 {
            advance_runtime_tick(&state).unwrap();
        }
        let completed = inspect(&state).unwrap();
        assert_eq!(completed.mode.tick, 29);
        assert!(!completed.mode.playing);
        assert_eq!(completed.live_tick, 1);
        let seeked = Playback::at("test".into(), saved, 29).unwrap();
        assert_eq!(
            seeked.projection,
            state
                .inner
                .lock()
                .unwrap()
                .playback
                .as_ref()
                .unwrap()
                .projection
        );
        {
            let mut game = state.inner.lock().unwrap();
            assert!(operate(&mut game, "step").is_err());
            operate(&mut game, "live").unwrap();
            assert!(game.ensure_live_input().is_ok());
        }
        advance_runtime_tick(&state).unwrap();
        assert_eq!(inspect(&state).unwrap().live_tick, 2);
        assert_eq!(inspect(&state).unwrap().state["overlay"], "pause");
        assert_eq!(inspect(&state).unwrap().state["phase"], 1);
    }

    #[tokio::test]
    async fn dropped_seek_waiter_finishes_and_live_cancellation_wins() {
        let state = super::super::tests::test_state();
        let saved = session();
        for restore_live in [false, true] {
            let operation = state.playback_operations.clone().lock_owned().await;
            let generation = {
                let mut game = state.inner.lock().unwrap();
                game.playback_generation += 1;
                let mut playback = Playback::at("test".into(), saved.clone(), -1).unwrap();
                playback.seeking = true;
                game.playback = Some(playback);
                game.playback_generation
            };
            let (release, wait) = std::sync::mpsc::channel();
            let saved = saved.clone();
            let worker = spawn_seek(state.clone(), generation, operation, move || {
                wait.recv().unwrap();
                Playback::at("test".into(), saved, 17)
            })
            .unwrap();
            // Dropping a request's JoinHandle detaches its already scheduled
            // worker. Hold it at a deterministic barrier, with seeking set.
            drop(worker);
            assert!(inspect(&state).unwrap().mode.seeking);
            assert!(state.playback_operations.try_lock().is_err());
            if restore_live {
                operate(&mut state.inner.lock().unwrap(), "live").unwrap();
            }
            release.send(()).unwrap();
            let _completed = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                state.playback_operations.lock(),
            )
            .await
            .unwrap();
            advance_runtime_tick(&state).unwrap();
            let status = inspect(&state).unwrap();
            assert!(!status.mode.seeking);
            if restore_live {
                assert!(
                    !status.mode.active,
                    "a detached seek cannot resurrect replay"
                );
            } else {
                assert_eq!(status.mode.tick, 17);
                assert_eq!(status.state["tick"], 17);
                operate(&mut state.inner.lock().unwrap(), "play").unwrap();
            }
        }
    }

    #[tokio::test]
    async fn seek_stop_and_bad_targets_keep_verified_state_and_live_clock() {
        let state = super::super::tests::test_state();
        {
            let mut game = state.inner.lock().unwrap();
            game.playback_generation = 1;
            queue_loaded(
                &mut game,
                Playback::at("test".into(), session(), -1).unwrap(),
            )
            .unwrap();
        }
        advance_runtime_tick(&state).unwrap();
        let Json(at) = with_owner_ticks(
            &state,
            seek(State(state.clone()), Json(SeekRequest { tick: 17 })),
        )
        .await
        .unwrap();
        assert_eq!(at.mode.tick, 17);
        assert_eq!(at.state["tick"], 17);
        assert_eq!(at.live_tick, 1);
        assert!(seek(State(state.clone()), Json(SeekRequest { tick: 30 }))
            .await
            .is_err());
        assert_eq!(inspect(&state).unwrap().mode.tick, 17);
        let Json(stopped) = with_owner_ticks(
            &state,
            command(
                State(state.clone()),
                Json(CommandRequest {
                    action: "stop".into(),
                }),
            ),
        )
        .await
        .unwrap();
        assert_eq!(stopped.mode.tick, -1);
        assert!(!stopped.mode.playing);
        assert_eq!(stopped.live_tick, 1);
    }

    #[tokio::test]
    async fn http_controls_wait_for_the_owner_and_verification_can_cancel_between_ticks() {
        let state = super::super::tests::test_state();
        let saved = session();
        {
            let mut game = state.inner.lock().unwrap();
            game.playback_generation = 1;
            queue_loaded(
                &mut game,
                Playback::at("test".into(), saved.clone(), -1).unwrap(),
            )
            .unwrap();
        }
        advance_runtime_tick(&state).unwrap();
        let request_state = state.clone();
        let request = tokio::spawn(async move {
            command(
                State(request_state),
                Json(CommandRequest {
                    action: "step".into(),
                }),
            )
            .await
        });
        for _ in 0..100 {
            if !state.inner.lock().unwrap().controls.is_empty() {
                break;
            }
            tokio::task::yield_now().await;
        }
        assert!(!state.inner.lock().unwrap().controls.is_empty());
        assert_eq!(inspect(&state).unwrap().mode.tick, -1);
        assert!(!request.is_finished());
        advance_runtime_tick(&state).unwrap();
        assert_eq!(request.await.unwrap().unwrap().0.mode.tick, 0);
        let mut calls = 0;
        let error = saved
            .verify_with_cancel(|| {
                calls += 1;
                anyhow::ensure!(calls < 5, "verification cancelled by test");
                Ok(())
            })
            .unwrap_err();
        assert!(error.to_string().contains("verification cancelled"));
        assert_eq!(calls, 5);
        assert_eq!(saved.verify().unwrap().ticks, 30);
    }
}
