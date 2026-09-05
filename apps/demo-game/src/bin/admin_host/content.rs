//! File evaluation and persistence stay outside the simulation lock and tick.

use std::{io::Read, path::PathBuf};

use arena::config::{ContentSnapshot, ContentVersion};

use super::*;

pub(super) struct Service {
    path: PathBuf,
    run_directory: PathBuf,
    validation: tokio::sync::Mutex<()>,
    catalog: Mutex<Catalog>,
}

#[derive(Default)]
struct Catalog {
    candidate: Option<ContentVersion>,
    diagnostics: Vec<String>,
    next_id: u64,
    saved_run: Option<u64>,
    persistence_error: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ContentStatus {
    path: String,
    runtime: ContentSnapshot,
    candidate: Option<ContentVersion>,
    diagnostics: Vec<String>,
    saved_run: Option<u64>,
    persistence_error: Option<String>,
}

impl Service {
    pub(super) fn from_environment() -> Self {
        Self::new(
            env::var_os("KITU_ARENA_TMD")
                .map(PathBuf::from)
                .unwrap_or_else(|| "apps/demo-game/content/arena.tmd".into()),
            env::var_os("KITU_ARENA_RUN_DIRECTORY")
                .map(PathBuf::from)
                .unwrap_or_else(|| "apps/demo-game/.arena/runs".into()),
        )
    }

    fn new(path: PathBuf, run_directory: PathBuf) -> Self {
        Self {
            path,
            run_directory,
            validation: tokio::sync::Mutex::new(()),
            catalog: Mutex::new(Catalog::default()),
        }
    }
}

pub(super) async fn inspect(
    State(state): State<AppState>,
) -> Result<Json<ContentStatus>, ApiError> {
    status(&state).map(Json).map_err(Into::into)
}

fn status(state: &AppState) -> Result<ContentStatus> {
    let runtime = {
        let game = state
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        arena::inspect_content(&game.runtime)?
    };
    let catalog = state
        .content
        .catalog
        .lock()
        .map_err(|_| anyhow::anyhow!("content lock poisoned"))?;
    Ok(ContentStatus {
        path: state.content.path.display().to_string(),
        runtime,
        candidate: catalog.candidate.clone(),
        diagnostics: catalog.diagnostics.clone(),
        saved_run: catalog.saved_run,
        persistence_error: catalog.persistence_error.clone(),
    })
}

pub(super) async fn validate(
    State(state): State<AppState>,
) -> Result<Json<ContentStatus>, ApiError> {
    // Serialize validations so a slower, older request cannot replace a newer one.
    // The runtime continues to tick while disk I/O and Formula evaluation run.
    let _validation = state.content.validation.lock().await;
    let path = state.content.path.clone();
    let result = tokio::task::spawn_blocking(move || read_candidate(path)).await;
    {
        let mut catalog = state
            .content
            .catalog
            .lock()
            .map_err(|_| ApiError::state_poisoned())?;
        catalog.candidate = None;
        catalog.diagnostics.clear();
        match result {
            Ok(Ok(candidate)) => catalog.candidate = Some(candidate),
            Ok(Err(error)) => catalog.diagnostics.push(format!("{error:#}")),
            Err(error) => catalog
                .diagnostics
                .push(format!("Tanu evaluation task failed: {error}")),
        }
    }
    status(&state).map(Json).map_err(Into::into)
}

fn read_candidate(path: PathBuf) -> Result<ContentVersion> {
    let mut bytes = Vec::new();
    std::fs::File::open(&path)
        .with_context(|| format!("open {}", path.display()))?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    ContentVersion::from_tmd(&bytes).map_err(anyhow::Error::msg)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct StageRequest {
    hash: String,
    source_sha256: String,
}

#[derive(Serialize)]
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

fn stage_candidate(state: &AppState, request: StageRequest) -> Result<StageResponse> {
    let mut catalog = state
        .content
        .catalog
        .lock()
        .map_err(|_| anyhow::anyhow!("content lock poisoned"))?;
    let candidate = catalog
        .candidate
        .as_ref()
        .context("validate a valid TMD before applying")?;
    anyhow::ensure!(
        candidate.hash == request.hash && candidate.source_sha256 == request.source_sha256,
        "candidate changed; inspect and validate the new version before applying"
    );
    let candidate = candidate.clone();
    let mut game = state
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    let id = catalog
        .next_id
        .checked_add(1)
        .context("content command IDs exhausted")?;
    let sequence = arena::stage_content(&mut game.runtime, candidate, id)?;
    catalog.next_id = id;
    Ok(StageResponse {
        sequence,
        hash: request.hash,
    })
}

pub(super) fn save_run_event(state: &AppState, event: &ServerEvent) {
    let ServerEvent::Osc { address, args } = event else {
        return;
    };
    if address != "/game/arena/run" {
        return;
    }
    let [JsonOscArg::Str(json)] = args.as_slice() else {
        return;
    };
    let Ok(run) = serde_json::from_str::<serde_json::Value>(json) else {
        return;
    };
    let runtime_id = match state.inner.lock() {
        Ok(game) => game.runtime_id.clone(),
        Err(_) => return,
    };
    let service = state.content.clone();
    tokio::spawn(async move {
        let result = persist_run(&service, &runtime_id, &run).await;
        if let Ok(mut catalog) = service.catalog.lock() {
            match result {
                Ok(number) => catalog.saved_run = Some(catalog.saved_run.unwrap_or(0).max(number)),
                Err(error) => {
                    error!("Arena run manifest could not be saved: {error:#}");
                    catalog.persistence_error = Some(format!("{error:#}"));
                }
            }
        }
    });
}

async fn persist_run(service: &Service, runtime_id: &str, run: &serde_json::Value) -> Result<u64> {
    let number = run["run"]
        .as_u64()
        .context("run event is missing a run number")?;
    let directory = service.run_directory.join(runtime_id);
    tokio::fs::create_dir_all(&directory).await?;
    let manifest = serde_json::json!({
        "manifestVersion": 1, "application": "endless-arena",
        "applicationVersion": env!("CARGO_PKG_VERSION"),
        "contractVersion": arena::SCHEMA_VERSION, "tickRate": 60,
        "runtimeId": runtime_id, "start": run,
    });
    let temporary = directory.join(format!("{number}.json.tmp"));
    tokio::fs::write(&temporary, serde_json::to_vec_pretty(&manifest)?).await?;
    tokio::fs::rename(&temporary, directory.join(format!("{number}.json"))).await?;
    Ok(number)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn invalid_edit_retains_pending_values_and_saved_runs_are_detached() {
        let mut state = super::super::tests::test_state();
        let directory = std::env::temp_dir().join(format!(
            "arena-content-{}",
            state.inner.lock().unwrap().runtime_id
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("arena.tmd");
        state.content = Arc::new(Service::new(path.clone(), directory.join("runs")));
        let mut config = arena::config::ArenaConfig::default();
        config.items[0].damage = 37;
        std::fs::write(&path, config.to_tmd().unwrap()).unwrap();
        let Json(valid) = validate(State(state.clone())).await.unwrap();
        let candidate = valid.candidate.unwrap();
        assert_ne!(valid.runtime.pending.hash, candidate.hash);
        let wrong = StageRequest {
            hash: "stale".into(),
            source_sha256: candidate.source_sha256.clone(),
        };
        assert!(stage_candidate(&state, wrong).is_err());
        stage_candidate(
            &state,
            StageRequest {
                hash: candidate.hash.clone(),
                source_sha256: candidate.source_sha256.clone(),
            },
        )
        .unwrap();
        assert_eq!(state.inner.lock().unwrap().runtime.current_tick().get(), 0);
        advance_runtime_tick(&state).unwrap();
        assert_eq!(status(&state).unwrap().runtime.pending, candidate);
        std::fs::write(&path, b"invalid edited file").unwrap();
        let Json(invalid) = validate(State(state.clone())).await.unwrap();
        assert!(invalid.candidate.is_none());
        assert!(!invalid.diagnostics.is_empty());
        assert_eq!(invalid.runtime.pending, candidate);
        assert!(stage_candidate(
            &state,
            StageRequest {
                hash: candidate.hash.clone(),
                source_sha256: candidate.source_sha256.clone()
            }
        )
        .is_err());

        let request = super::super::tests::arena_request(&state, 1, "/input/arena/start");
        enqueue_arena_request(&state, 1, request).unwrap();
        let events = advance_runtime_tick(&state).unwrap();
        let ServerEvent::Osc {args, ..} = events.iter().find(|event| matches!(event, ServerEvent::Osc {address, ..} if address == "/game/arena/run")).unwrap() else { panic!() };
        let JsonOscArg::Str(json) = &args[0] else {
            panic!()
        };
        let run: serde_json::Value = serde_json::from_str(json).unwrap();
        persist_run(&state.content, "test-runtime", &run)
            .await
            .unwrap();
        let saved: serde_json::Value = serde_json::from_slice(
            &std::fs::read(directory.join("runs/test-runtime/1.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(saved["start"]["content"]["hash"], candidate.hash);
        assert_eq!(
            saved["start"]["content"]["values"]["items"][0]["damage"],
            37
        );
        assert_eq!(saved["tickRate"], 60);
        std::fs::remove_dir_all(directory).unwrap();
    }
}
