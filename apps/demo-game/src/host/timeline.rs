//! Bounded TSQ1 authoring outside the clock lock; immutable clips enter the normal queue.

use arena::presentation::{PreparedTimeline, TimelineSnapshot, TimelineVersion};

use super::*;

pub(super) struct Service {
    directory: Option<PathBuf>,
    validation: tokio::sync::Mutex<()>,
    catalog: Mutex<Catalog>,
}

#[derive(Default)]
struct Catalog {
    candidate: Option<Arc<PreparedTimeline>>,
    diagnostics: Vec<String>,
    next_id: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TimelineStatus {
    path: Option<String>,
    read_only: bool,
    runtime: TimelineSnapshot,
    candidate: Option<TimelineVersion>,
    diagnostics: Vec<String>,
}

impl Service {
    pub(super) fn new(directory: Option<PathBuf>) -> Self {
        Self {
            directory,
            validation: tokio::sync::Mutex::new(()),
            catalog: Mutex::new(Catalog::default()),
        }
    }
}

fn status(state: &AppState) -> Result<TimelineStatus> {
    let (runtime, read_only) = {
        let game = state
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        (
            arena::inspect_timeline(game.observed_runtime())?,
            game.ensure_live_input().is_err(),
        )
    };
    let catalog = state
        .timeline
        .catalog
        .lock()
        .map_err(|_| anyhow::anyhow!("timeline lock poisoned"))?;
    Ok(TimelineStatus {
        path: state
            .timeline
            .directory
            .as_ref()
            .map(|path| path.display().to_string()),
        read_only,
        runtime,
        candidate: catalog
            .candidate
            .as_ref()
            .map(|candidate| candidate.version.clone()),
        diagnostics: catalog.diagnostics.clone(),
    })
}

pub(super) async fn inspect(
    State(state): State<AppState>,
) -> Result<Json<TimelineStatus>, ApiError> {
    status(&state).map(Json).map_err(Into::into)
}

pub(super) async fn validate(
    State(state): State<AppState>,
) -> Result<Json<TimelineStatus>, ApiError> {
    let _validation = state.timeline.validation.lock().await;
    state.work.check()?;
    let directory = state.timeline.directory.clone();
    let work = state.work.clone();
    let result = state
        .spawn_blocking(move || {
            anyhow::ensure!(!work.is_closing(), "host is closing");
            let version = match directory {
                Some(path) => arena::presentation::load_timeline(&path),
                None => arena::presentation::default_timeline(),
            }?;
            anyhow::ensure!(!work.is_closing(), "host is closing");
            arena::presentation::prepare_version(&version)
        })?
        .await;
    {
        let mut catalog = state
            .timeline
            .catalog
            .lock()
            .map_err(|_| ApiError::state_poisoned())?;
        catalog.candidate = None;
        catalog.diagnostics.clear();
        match result {
            Ok(Ok(prepared)) => catalog.candidate = Some(prepared),
            Ok(Err(error)) => catalog.diagnostics.push(diagnostic(&format!("{error:#}"))),
            Err(error) => catalog.diagnostics.push(diagnostic(&format!(
                "timeline validation task failed: {error}"
            ))),
        }
    }
    status(&state).map(Json).map_err(Into::into)
}

fn diagnostic(message: &str) -> String {
    let mut end = message.len().min(1024);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    message[..end].into()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StageRequest {
    pub(super) hash: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct StageResponse {
    sequence: u64,
    hash: String,
}

pub(super) async fn stage(
    State(state): State<AppState>,
    Json(request): Json<StageRequest>,
) -> Result<Json<StageResponse>, (StatusCode, Json<serde_json::Value>)> {
    stage_candidate(&state, request).map(Json).map_err(|error| {
        (
            StatusCode::CONFLICT,
            Json(serde_json::json!({"error": error.to_string()})),
        )
    })
}

pub(super) fn stage_candidate(state: &AppState, request: StageRequest) -> Result<StageResponse> {
    state.work.check()?;
    let mut catalog = state
        .timeline
        .catalog
        .lock()
        .map_err(|_| anyhow::anyhow!("timeline lock poisoned"))?;
    let candidate = catalog
        .candidate
        .as_ref()
        .context("validate valid timelines before applying")?;
    anyhow::ensure!(
        candidate.version.hash == request.hash,
        "candidate changed; inspect and validate the new version before applying"
    );
    let candidate = candidate.clone();
    let mut game = state
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    game.ensure_live_input()?;
    let id = catalog
        .next_id
        .checked_add(1)
        .context("timeline command IDs exhausted")?;
    let sequence = arena::stage_prepared_timeline_from(
        &mut game.runtime,
        candidate,
        id,
        arena::TIMELINE_OPERATOR_SOURCE,
    )?;
    catalog.next_id = id;
    Ok(StageResponse {
        sequence,
        hash: request.hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use kitu_tsq1::presentation::Clip;

    fn write_version(directory: &std::path::Path, radius: f32) {
        let version = arena::presentation::default_timeline().unwrap();
        let mut boss = Clip::decode(&version.clips[0].bytes).unwrap();
        for event in &mut boss.events {
            for message in &mut event.bundle.messages {
                message.args[0] = OscArg::Float(radius);
            }
        }
        std::fs::write(directory.join("boss-telegraph.tsq"), boss.encode().unwrap()).unwrap();
        std::fs::write(
            directory.join("floor-transition.tsq"),
            &version.clips[1].bytes,
        )
        .unwrap();
    }

    #[tokio::test]
    async fn binary_clip_validation_is_bound_to_candidate_and_adopts_only_next_run() {
        let mut state = super::super::tests::test_state();
        let directory = std::env::temp_dir().join(format!(
            "arena-timeline-host-{}",
            state.inner.lock().unwrap().runtime_id
        ));
        std::fs::create_dir_all(&directory).unwrap();
        state.timeline = Arc::new(Service::new(Some(directory.clone())));
        let initial = status(&state).unwrap().runtime.pending;
        enqueue_arena_request(
            &state,
            1,
            super::super::tests::arena_request(&state, 1, "/input/arena/start"),
        )
        .unwrap();
        advance_runtime_tick(&state).unwrap();
        assert_eq!(
            status(&state).unwrap().runtime.active,
            Some(initial.clone())
        );

        write_version(&directory, 4.0);
        let first = validate(State(state.clone()))
            .await
            .unwrap()
            .0
            .candidate
            .unwrap();
        write_version(&directory, 4.5);
        let current = validate(State(state.clone()))
            .await
            .unwrap()
            .0
            .candidate
            .unwrap();
        assert_ne!(first.hash, current.hash);
        assert!(stage_candidate(&state, StageRequest { hash: first.hash }).is_err());
        stage_candidate(
            &state,
            StageRequest {
                hash: current.hash.clone(),
            },
        )
        .unwrap();
        assert_eq!(status(&state).unwrap().runtime.pending, initial);
        advance_runtime_tick(&state).unwrap();
        let staged = status(&state).unwrap();
        assert_eq!(staged.runtime.pending, current);
        assert_eq!(staged.runtime.active, Some(initial.clone()));

        std::fs::write(directory.join("boss-telegraph.tsq"), b"invalid TSQ1").unwrap();
        let invalid = validate(State(state.clone())).await.unwrap().0;
        assert!(invalid.candidate.is_none());
        assert!(!invalid.diagnostics.is_empty());
        assert_eq!(invalid.runtime.pending, current);
        assert_eq!(invalid.runtime.active, Some(initial));
        assert!(stage_candidate(
            &state,
            StageRequest {
                hash: current.hash.clone()
            }
        )
        .is_err());

        enqueue_arena_request(
            &state,
            1,
            super::super::tests::arena_request(&state, 2, "/input/arena/menu"),
        )
        .unwrap();
        advance_runtime_tick(&state).unwrap();
        enqueue_arena_request(
            &state,
            1,
            super::super::tests::arena_request(&state, 3, "/input/arena/start"),
        )
        .unwrap();
        advance_runtime_tick(&state).unwrap();
        assert_eq!(status(&state).unwrap().runtime.active, Some(current));
        std::fs::remove_dir_all(directory).unwrap();
    }
}
