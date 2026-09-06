//! Script authoring work runs outside the simulation lock; staging uses its input queue.

use arena::script::{ScriptSnapshot, ScriptVersion};
use kitu_scripting_rhai::Diagnostic;

use super::*;

pub(super) struct Service {
    path: Option<PathBuf>,
    validation: tokio::sync::Mutex<()>,
    catalog: Mutex<Catalog>,
}

#[derive(Default)]
struct Catalog {
    candidate: Option<Arc<arena::script::PreparedScript>>,
    diagnostics: Vec<Diagnostic>,
    next_id: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ScriptStatus {
    path: Option<String>,
    read_only: bool,
    runtime: ScriptSnapshot,
    candidate: Option<ScriptVersion>,
    diagnostics: Vec<Diagnostic>,
}

impl Service {
    pub(super) fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            validation: tokio::sync::Mutex::new(()),
            catalog: Mutex::new(Catalog::default()),
        }
    }
}

fn status(state: &AppState) -> Result<ScriptStatus> {
    let (runtime, read_only) = {
        let game = state
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        (
            arena::inspect_script(game.observed_runtime())?,
            game.ensure_live_input().is_err(),
        )
    };
    let catalog = state
        .script
        .catalog
        .lock()
        .map_err(|_| anyhow::anyhow!("script lock poisoned"))?;
    Ok(ScriptStatus {
        path: state
            .script
            .path
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

pub(super) async fn inspect(State(state): State<AppState>) -> Result<Json<ScriptStatus>, ApiError> {
    status(&state).map(Json).map_err(Into::into)
}

pub(super) async fn validate(
    State(state): State<AppState>,
) -> Result<Json<ScriptStatus>, ApiError> {
    let _validation = state.script.validation.lock().await;
    state.work.check()?;
    let path = state.script.path.clone();
    let work = state.work.clone();
    let result = state
        .spawn_blocking(move || {
            if work.is_closing() {
                return Err(diagnostic("shutdown", "host is closing").into());
            }
            let version = match path {
                Some(path) => arena::script::load_script(&path),
                None => arena::script::default_script(),
            }?;
            if work.is_closing() {
                return Err(diagnostic("shutdown", "host is closing").into());
            }
            arena::script::prepare_version(&version).map_err(Into::into)
        })?
        .await;
    {
        let mut catalog = state
            .script
            .catalog
            .lock()
            .map_err(|_| ApiError::state_poisoned())?;
        catalog.candidate = None;
        catalog.diagnostics.clear();
        match result {
            Ok(Ok(version)) => catalog.candidate = Some(version),
            Ok(Err(error)) => catalog.diagnostics.push(
                error
                    .downcast_ref::<Diagnostic>()
                    .cloned()
                    .unwrap_or_else(|| diagnostic("validation", &error.to_string())),
            ),
            Err(error) => catalog.diagnostics.push(diagnostic(
                "worker",
                &format!("script validation task failed: {error}"),
            )),
        }
    }
    status(&state).map(Json).map_err(Into::into)
}

fn diagnostic(kind: &str, message: &str) -> Diagnostic {
    let mut end = message.len().min(1024);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    Diagnostic {
        kind: kind.into(),
        message: message[..end].into(),
        line: None,
        column: None,
    }
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
        .script
        .catalog
        .lock()
        .map_err(|_| anyhow::anyhow!("script lock poisoned"))?;
    let candidate = catalog
        .candidate
        .as_ref()
        .context("validate a valid script before applying")?;
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
        .context("script command IDs exhausted")?;
    let sequence = arena::stage_prepared_script(&mut game.runtime, candidate, id)?;
    catalog.next_id = id;
    Ok(StageResponse {
        sequence,
        hash: request.hash,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn source_tokens_staging_and_invalid_edits_preserve_active_rules() {
        let mut state = super::super::tests::test_state();
        let directory = std::env::temp_dir().join(format!(
            "arena-script-host-{}",
            state.inner.lock().unwrap().runtime_id
        ));
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join("boss.rhai");
        state.script = Arc::new(Service::new(Some(path.clone())));
        let initial = status(&state).unwrap().runtime.pending;
        let start = super::super::tests::arena_request(&state, 1, "/input/arena/start");
        enqueue_arena_request(&state, 1, start).unwrap();
        advance_runtime_tick(&state).unwrap();
        assert_eq!(
            status(&state).unwrap().runtime.active,
            Some(initial.clone())
        );

        let edited = initial.source.replace("duration: 0.8", "duration: 1.6");
        assert_ne!(edited, initial.source);
        std::fs::write(&path, &edited).unwrap();
        let Json(first) = validate(State(state.clone())).await.unwrap();
        let reviewed = first.candidate.unwrap();
        std::fs::write(&path, format!("{edited}\n// Reviewed source changed.\n")).unwrap();
        let Json(second) = validate(State(state.clone())).await.unwrap();
        let current = second.candidate.unwrap();
        assert_ne!(reviewed.hash, current.hash);
        // Other authoring sessions can evict the shared preparation cache. The
        // reviewed catalog still owns its compiled program, so staging is cache-only.
        for index in 0..20 {
            ScriptVersion::from_source(&format!("{edited}\n// Another candidate {index}\n"))
                .unwrap();
        }
        assert!(stage_candidate(
            &state,
            StageRequest {
                hash: reviewed.hash
            }
        )
        .unwrap_err()
        .to_string()
        .contains("candidate changed"));
        stage_candidate(
            &state,
            StageRequest {
                hash: current.hash.clone(),
            },
        )
        .unwrap();
        assert_eq!(status(&state).unwrap().runtime.pending, initial);
        advance_runtime_tick(&state).unwrap();
        let staged = status(&state).unwrap().runtime;
        assert_eq!(staged.pending, current);
        assert_eq!(staged.active, Some(initial));

        std::fs::write(&path, "fn boss(input) { return #{action: ").unwrap();
        let Json(invalid) = validate(State(state.clone())).await.unwrap();
        assert!(invalid.candidate.is_none());
        assert!(!invalid.diagnostics.is_empty());
        assert_eq!(invalid.runtime.pending, current);
        assert_eq!(invalid.runtime.active, staged.active);
        assert!(stage_candidate(
            &state,
            StageRequest {
                hash: current.hash.clone()
            }
        )
        .is_err());
        for (id, address) in [(2, "/input/arena/menu"), (3, "/input/arena/start")] {
            enqueue_arena_request(
                &state,
                1,
                super::super::tests::arena_request(&state, id, address),
            )
            .unwrap();
            advance_runtime_tick(&state).unwrap();
        }
        assert_eq!(status(&state).unwrap().runtime.active, Some(current));
        std::fs::remove_dir_all(directory).unwrap();
        let recorded = state.inner.lock().unwrap().recorder.encode().unwrap();
        let session = crate::replay::Session::decode(&recorded).unwrap();
        assert_eq!(session.verify().unwrap().runs, 2);
    }

    #[tokio::test]
    async fn bundled_validation_requires_no_authoring_path_and_shutdown_refuses_work() {
        let state = super::super::tests::test_state();
        let Json(valid) = validate(State(state.clone())).await.unwrap();
        assert!(valid.path.is_none());
        assert_eq!(valid.candidate, Some(valid.runtime.pending));
        state.work.close();
        assert!(validate(State(state.clone())).await.is_err());
        assert!(stage_candidate(
            &state,
            StageRequest {
                hash: valid.candidate.unwrap().hash
            }
        )
        .is_err());
        state.work.wait().await;
    }
}
