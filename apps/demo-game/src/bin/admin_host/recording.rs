//! Detached TSQ1 export, atomic local persistence and bounded replay verification.
use super::*;
use axum::{body::Bytes, http::header};
use kitu_demo_game::replay::{Recorder, Session};
use sha2::{Digest, Sha256};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) async fn status(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let game = state.inner.lock().map_err(|_| ApiError::state_poisoned())?;
    Ok(Json(
        serde_json::json!({"runtimeId":game.runtime_id,"ticks":game.recorder.ticks(),"liveTick":game.runtime.current_tick().get(),"error":game.recording_error,"maxTicks":kitu_demo_game::replay::MAX_TICKS}),
    ))
}
fn detached(state: &AppState) -> Result<Recorder> {
    let game = state
        .inner
        .lock()
        .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
    // An incomplete recorder must not be presented as a complete live session.
    anyhow::ensure!(
        game.recording_error.is_none(),
        "recording incomplete: {}",
        game.recording_error.as_deref().unwrap_or_default()
    );
    Ok(game.recorder.clone())
}
async fn encoded(state: &AppState) -> Result<Vec<u8>> {
    let recorder = detached(state)?;
    tokio::task::spawn_blocking(move || recorder.encode()).await?
}
fn binary(bytes: Vec<u8>) -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "application/octet-stream"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=arena.tsq",
            ),
        ],
        bytes,
    )
}
pub(super) async fn export(State(state): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    Ok(binary(encoded(&state).await?))
}
pub(super) async fn save(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let bytes = encoded(&state).await?;
    Ok(Json(persist(&directory(), &bytes).await?))
}
pub(super) fn directory() -> PathBuf {
    env::var_os("KITU_ARENA_RECORDING_DIRECTORY")
        .map(PathBuf::from)
        .unwrap_or_else(|| "apps/demo-game/.arena/recordings".into())
}
fn path(id: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        id.len() == 64 && id.bytes().all(|c| c.is_ascii_hexdigit()),
        "invalid recording id"
    );
    Ok(directory().join(format!("{id}.tsq")))
}
async fn persist(dir: &std::path::Path, bytes: &[u8]) -> Result<serde_json::Value> {
    let id = hex::encode(Sha256::digest(bytes));
    tokio::fs::create_dir_all(dir).await?;
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let temporary = dir.join(format!(
        ".{id}-{}-{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    tokio::fs::write(&temporary, bytes).await?;
    let destination = dir.join(format!("{id}.tsq"));
    if let Err(error) = tokio::fs::rename(&temporary, &destination).await {
        let _ = tokio::fs::remove_file(&temporary).await;
        return Err(error.into());
    }
    Ok(serde_json::json!({"id":id,"bytes":bytes.len(),"path":destination}))
}
pub(super) async fn list() -> Result<Json<serde_json::Value>, ApiError> {
    let mut entries = Vec::new();
    let mut files = match tokio::fs::read_dir(directory()).await {
        Ok(files) => files,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Json(serde_json::json!([])))
        }
        Err(e) => return Err(anyhow::Error::from(e).into()),
    };
    while let Some(file) = files.next_entry().await.map_err(anyhow::Error::from)? {
        let name = file.file_name().to_string_lossy().to_string();
        if let Some(id) = name.strip_suffix(".tsq") {
            if path(id).is_ok() {
                entries.push(serde_json::json!({"id":id,"bytes":file.metadata().await.map_err(anyhow::Error::from)?.len()}));
            }
        }
    }
    entries.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(Json(serde_json::Value::Array(entries)))
}
pub(super) async fn read(id: &str) -> Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    tokio::fs::File::open(path(id)?)
        .await?
        .take(kitu_tsq1::recording::MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .await?;
    anyhow::ensure!(
        bytes.len() <= kitu_tsq1::recording::MAX_BYTES,
        "recording exceeds size limit"
    );
    Ok(bytes)
}
pub(super) async fn download(Path(id): Path<String>) -> Result<impl IntoResponse, ApiError> {
    Ok(binary(read(&id).await?))
}
pub(super) async fn import(bytes: Bytes) -> Result<Json<serde_json::Value>, ApiError> {
    let bytes = bytes.to_vec();
    let bytes = tokio::task::spawn_blocking(move || {
        Session::decode(&bytes)?;
        Ok::<_, anyhow::Error>(bytes)
    })
    .await
    .map_err(anyhow::Error::from)??;
    Ok(Json(persist(&directory(), &bytes).await?))
}
pub(super) async fn verify(
    Path(id): Path<String>,
) -> Result<Json<kitu_demo_game::replay::Verification>, ApiError> {
    let bytes = read(&id).await?;
    // CPU work is detached from the live simulation; only one verification at a time.
    static VERIFY: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(1);
    let _permit = VERIFY.acquire().await.map_err(anyhow::Error::from)?;
    let result = tokio::task::spawn_blocking(move || Session::decode(&bytes)?.verify())
        .await
        .map_err(anyhow::Error::from)??;
    Ok(Json(result))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn unencodable_committed_input_reports_recording_failure_while_game_ticks_continue() {
        let state = super::super::tests::test_state();
        {
            let mut game = state.inner.lock().unwrap();
            let mut message = OscMessage::new("/test/ignored");
            message.args.push(OscArg::Float(f32::NAN));
            game.runtime
                .try_enqueue_input(
                    kitu_osc_ir::OscBundle {
                        messages: vec![message],
                    },
                    None,
                )
                .unwrap();
        }
        advance_runtime_tick(&state).unwrap();
        advance_runtime_tick(&state).unwrap();
        let Json(status) = status(State(state.clone())).await.unwrap();
        assert_eq!(status["liveTick"], 2);
        assert_eq!(status["ticks"], 0);
        assert!(status["error"]
            .as_str()
            .unwrap()
            .contains("non-finite OSC float"));
        assert!(encoded(&state).await.is_err());
    }

    #[tokio::test]
    async fn live_host_saves_loads_and_verifies_the_same_tick_queue() {
        let state = super::super::tests::test_state();
        {
            let mut game = state.inner.lock().unwrap();
            let mut bundle = kitu_osc_ir::OscBundle::new();
            bundle.push(OscMessage::new("/input/arena/start"));
            game.runtime
                .try_enqueue_input(
                    bundle,
                    Some(InputMetadata {
                        source: "recording-test".into(),
                        message_id: 1,
                        schema_version: 1,
                    }),
                )
                .unwrap();
        }
        for _ in 0..10 {
            advance_runtime_tick(&state).unwrap();
        }
        let bytes = encoded(&state).await.unwrap();
        let dir = std::env::temp_dir().join(format!("kitu-tsq1-{}", std::process::id()));
        let saved = persist(&dir, &bytes).await.unwrap();
        let loaded = tokio::fs::read(saved["path"].as_str().unwrap())
            .await
            .unwrap();
        assert_eq!(loaded, bytes);
        let report = Session::decode(&loaded).unwrap().verify().unwrap();
        assert_eq!((report.ticks, report.inputs, report.runs), (10, 1, 1));
        assert_eq!(state.inner.lock().unwrap().runtime.current_tick().get(), 10);
        tokio::fs::remove_dir_all(dir).await.unwrap();
        assert!(path("../secrets").is_err());
    }
}
