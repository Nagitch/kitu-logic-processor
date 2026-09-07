//! Detached TSQ1 export, atomic local persistence and bounded replay verification.
use super::*;
use crate::replay::{Recorder, Session};
use axum::{body::Bytes, http::header};
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
        serde_json::json!({"runtimeId":game.runtime_id,"ticks":game.recorder.ticks(),"liveTick":game.runtime.current_tick().get(),"error":game.recording_error,"maxTicks":crate::replay::MAX_TICKS}),
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
    state.spawn_blocking(move || recorder.encode())?.await?
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
    let directory = directory(&state)?;
    let bytes = encoded(&state).await?;
    Ok(Json(persist(&directory, &bytes).await?))
}
pub(super) fn directory(state: &AppState) -> Result<PathBuf> {
    anyhow::ensure!(
        !state.options.recording_directory.as_os_str().is_empty(),
        "recording storage is disabled for this host"
    );
    Ok(state.options.recording_directory.clone())
}
fn path(state: &AppState, id: &str) -> Result<PathBuf> {
    anyhow::ensure!(
        id.len() == 64 && id.bytes().all(|c| c.is_ascii_hexdigit()),
        "invalid recording id"
    );
    Ok(directory(state)?.join(format!("{id}.tsq")))
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
pub(super) async fn list(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let mut entries = Vec::new();
    let mut files = match tokio::fs::read_dir(directory(&state)?).await {
        Ok(files) => files,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Json(serde_json::json!([])))
        }
        Err(e) => return Err(anyhow::Error::from(e).into()),
    };
    while let Some(file) = files.next_entry().await.map_err(anyhow::Error::from)? {
        let name = file.file_name().to_string_lossy().to_string();
        if let Some(id) = name.strip_suffix(".tsq") {
            if path(&state, id).is_ok() {
                entries.push(serde_json::json!({"id":id,"bytes":file.metadata().await.map_err(anyhow::Error::from)?.len()}));
            }
        }
    }
    entries.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Ok(Json(serde_json::Value::Array(entries)))
}
pub(super) async fn read(state: &AppState, id: &str) -> Result<Vec<u8>> {
    use tokio::io::AsyncReadExt;
    let mut bytes = Vec::new();
    tokio::fs::File::open(path(state, id)?)
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
pub(super) async fn download(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(binary(read(&state, &id).await?))
}
pub(super) async fn import(
    State(state): State<AppState>,
    bytes: Bytes,
) -> Result<Json<serde_json::Value>, ApiError> {
    let directory = directory(&state)?;
    let bytes = bytes.to_vec();
    let bytes = state
        .spawn_blocking(move || {
            Session::decode(&bytes)?;
            Ok::<_, anyhow::Error>(bytes)
        })?
        .await
        .map_err(anyhow::Error::from)??;
    Ok(Json(persist(&directory, &bytes).await?))
}
pub(super) async fn verify(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<Json<crate::replay::Verification>, ApiError> {
    let bytes = read(&state, &id).await?;
    let permit = state
        .verification_slots
        .clone()
        .acquire_owned()
        .await
        .map_err(anyhow::Error::from)?;
    let work = state.work.clone();
    let result = state
        .spawn_blocking(move || {
            let _permit = permit;
            Session::decode(&bytes)?.verify_with_cancel(|| work.check())
        })?
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
        assert!(path(&state, "../secrets").is_err());
    }
}
