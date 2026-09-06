//! Detached, bounded boss rules. Rhai requests actions; Rust owns clocks and effects.
//!
//! ```
//! let version = kitu_demo_game::arena::script::default_script().unwrap();
//! kitu_demo_game::arena::script::validate_version(&version).unwrap();
//! assert!(version.source.contains("fn boss("));
//! ```

use super::Enemy;
pub use kitu_scripting_rhai::Diagnostic;
use kitu_scripting_rhai::{CompiledScript, Limits, ScriptHost};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    fs::OpenOptions,
    io::Read,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

/// Application boss context/action schema, independent of the OSC envelope.
pub const CONTRACT_VERSION: u32 = 1;
/// Maximum detached source size, matching the generic execution policy.
pub const MAX_SOURCE_BYTES: usize = 64 * 1024;
/// Maximum encoded version admitted by the OSC management command.
pub const MAX_VERSION_BYTES: usize = 128 * 1024;
/// Reference behavior whose timings match the frozen Unity oracle.
pub const DEFAULT_SOURCE: &str = include_str!("../../content/boss.rhai");

/// Source and execution policy saved with every run and replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptVersion {
    /// SHA-256 of source plus the exact contract and execution policy identities.
    pub hash: String,
    /// SHA-256 of the UTF-8 source bytes.
    pub source_sha256: String,
    /// Detached source; never a filename or a reference to authoring state.
    pub source: String,
    /// Arena boss context/action schema.
    pub contract_version: u32,
    /// Generic sandbox policy identity.
    pub policy_version: String,
    /// Exact Rhai dependency version.
    pub rhai_version: String,
}

/// A runtime failure tied to the consumed tick and boss, without partial actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptFault {
    /// Management tick consumed when the failure was observed.
    pub tick: i64,
    /// Boss entity whose request failed before gameplay mutation.
    pub enemy_id: i32,
    /// Active detached script identity.
    pub script_hash: String,
    /// Bounded compile/evaluation/application-contract diagnostic.
    pub diagnostic: Diagnostic,
}

/// Current next-run candidate and the immutable active run rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ScriptSnapshot {
    /// Number of started runs.
    pub run: u64,
    /// Rules selected when the current or most recent run began.
    pub active: Option<ScriptVersion>,
    /// Validated candidate to adopt on start/retry.
    pub pending: ScriptVersion,
    /// Late evaluation failure; cleared only by starting a new run.
    pub fault: Option<ScriptFault>,
}

fn diagnostic(kind: &str, message: impl Into<String>) -> Diagnostic {
    let mut message = message.into();
    if message.len() > 1024 {
        let mut end = 1024;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
    }
    Diagnostic {
        kind: kind.into(),
        message,
        line: None,
        column: None,
    }
}
fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn identity(source: &str) -> ScriptVersion {
    let source_sha256 = sha(source.as_bytes());
    let policy_version = kitu_scripting_rhai::POLICY_VERSION.to_string();
    let rhai_version = kitu_scripting_rhai::RHAI_VERSION.to_string();
    let hash = sha(&serde_json::to_vec(&(
        CONTRACT_VERSION,
        &policy_version,
        &rhai_version,
        &source_sha256,
    ))
    .expect("serializable identity"));
    ScriptVersion {
        hash,
        source_sha256,
        source: source.into(),
        contract_version: CONTRACT_VERSION,
        policy_version,
        rhai_version,
    }
}

impl ScriptVersion {
    /// Compiles detached source and probes every phase/timer branch before adoption.
    /// Probes cannot prove every conditional path; late faults pause the run.
    pub fn from_source(source: &str) -> Result<Self, Diagnostic> {
        if source.len() > MAX_SOURCE_BYTES {
            return Err(diagnostic(
                "source_limit",
                "Arena boss source exceeds 64 KiB",
            ));
        }
        let version = identity(source);
        prepare(&version)?;
        Ok(version)
    }
}

/// Loads a UTF-8 source once, with a 64 KiB read bound; paths are diagnostic-only.
pub fn load_script(path: &Path) -> Result<ScriptVersion, Diagnostic> {
    let io_error = |error| diagnostic("source_io", format!("{}: {error}", path.display()));
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        // Opening a FIFO must not block, even if an editor replaces the path
        // between discovery and open. Validate the opened handle before reading.
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(io_error)?;
    if !file.metadata().map_err(io_error)?.is_file() {
        return Err(diagnostic(
            "source_io",
            "Arena boss source must be a regular file",
        ));
    }
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(io_error)?;
    if bytes.len() > MAX_SOURCE_BYTES {
        return Err(diagnostic(
            "source_limit",
            "Arena boss source exceeds 64 KiB",
        ));
    }
    let source = std::str::from_utf8(&bytes)
        .map_err(|_| diagnostic("source_encoding", "Arena boss source is not UTF-8"))?;
    ScriptVersion::from_source(source)
}

/// Returns validated bundled reference behavior without authoring filesystem I/O.
pub fn default_script() -> Result<ScriptVersion, Diagnostic> {
    ScriptVersion::from_source(DEFAULT_SOURCE)
}

/// Checks hashes, policy, bounds, syntax and representative action requests.
pub fn validate_version(version: &ScriptVersion) -> Result<(), Diagnostic> {
    prepare(version).map(|_| ())
}

/// Validated immutable program retained by authoring catalogs and replay sessions.
pub(crate) struct PreparedScript {
    pub version: ScriptVersion,
    host: ScriptHost,
    compiled: CompiledScript,
}

// Only validated programs enter this bounded host-authoring cache. Applications
// pin admitted programs separately until their queue batch has been consumed.
fn cache() -> &'static Mutex<VecDeque<Arc<PreparedScript>>> {
    static CACHE: OnceLock<Mutex<VecDeque<Arc<PreparedScript>>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(VecDeque::new()))
}

pub(crate) fn prepare(version: &ScriptVersion) -> Result<Arc<PreparedScript>, Diagnostic> {
    if let Some(prepared) = cache()
        .lock()
        .map_err(|_| diagnostic("cache", "Arena script cache is unavailable"))?
        .iter()
        .find(|prepared| prepared.version == *version)
        .cloned()
    {
        return Ok(prepared);
    }
    let prepared = Arc::new(compile_version(version)?);
    let mut entries = cache()
        .lock()
        .map_err(|_| diagnostic("cache", "Arena script cache is unavailable"))?;
    if let Some(existing) = entries.iter().find(|entry| entry.version == *version) {
        return Ok(existing.clone());
    }
    if entries.len() == 16 {
        entries.pop_front();
    }
    entries.push_back(prepared.clone());
    Ok(prepared)
}

/// Prepares a source off the simulation lock and retains it across cache eviction.
pub(crate) fn prepare_version(version: &ScriptVersion) -> Result<Arc<PreparedScript>, Diagnostic> {
    prepare(version)
}

fn compile_version(version: &ScriptVersion) -> Result<PreparedScript, Diagnostic> {
    if version.source.len() > MAX_SOURCE_BYTES {
        return Err(diagnostic(
            "source_limit",
            "Arena boss source exceeds 64 KiB",
        ));
    }
    if *version != identity(&version.source) {
        return Err(diagnostic(
            "version",
            "Arena boss hash, contract or execution policy is incompatible",
        ));
    }
    if serde_json::to_vec(version)
        .expect("serializable version")
        .len()
        > MAX_VERSION_BYTES
    {
        return Err(diagnostic(
            "source_limit",
            "Encoded Arena boss version exceeds 128 KiB",
        ));
    }
    let host = ScriptHost::new(Limits::default())?;
    let compiled = host.compile(&version.source)?;
    let prepared = PreparedScript {
        version: version.clone(),
        host,
        compiled,
    };
    for phase in 0..=2 {
        for timer_expired in [false, true] {
            for hp in [1, 100] {
                prepared.decide(phase, hp, 100, 5, timer_expired)?;
            }
        }
    }
    Ok(prepared)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum BossAction {
    Pursue,
    Telegraph,
    Burst,
    Recover,
    Wait,
}
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct BossDecision {
    pub action: BossAction,
    pub duration: f32,
}

impl PreparedScript {
    fn decide(
        &self,
        phase: i32,
        hp: i32,
        max_hp: i32,
        floor: i32,
        timer_expired: bool,
    ) -> Result<BossDecision, Diagnostic> {
        let request = self.host.invoke(&self.compiled, "boss", &json!({"phase":phase,"hp":hp,"maxHp":max_hp,"floor":floor,"timerExpired":timer_expired}))?;
        let decision: BossDecision = serde_json::from_value(request).map_err(|error| {
            diagnostic(
                "action_contract",
                format!("Invalid Arena boss action: {error}"),
            )
        })?;
        let transition = matches!(
            decision.action,
            BossAction::Telegraph | BossAction::Burst | BossAction::Recover
        );
        let allowed = match decision.action {
            BossAction::Pursue => phase == 0 && !timer_expired,
            BossAction::Telegraph => phase == 0 && timer_expired,
            BossAction::Burst => phase == 1 && timer_expired,
            BossAction::Recover => phase == 2 && timer_expired,
            BossAction::Wait => phase == 1 || phase == 2,
        };
        let duration_valid = decision.duration.is_finite()
            && if transition {
                decision.duration > 0.0 && decision.duration <= 60.0
            } else {
                decision.duration == 0.0
            };
        if !allowed || !duration_valid {
            return Err(diagnostic("action_contract", "Boss action must match its phase/timer; transitions require a finite duration in (0, 60] seconds and other actions require zero"));
        }
        Ok(decision)
    }

    pub(super) fn for_enemy(
        &self,
        enemy: &Enemy,
        floor: i32,
        dt: f32,
        tick: i64,
    ) -> Result<BossDecision, ScriptFault> {
        // Keep subtraction and epsilon in Rust f32, exactly like the C# oracle.
        self.decide(
            enemy.boss_state,
            enemy.health,
            enemy.max_health,
            floor,
            enemy.phase_remaining - dt <= 0.00001,
        )
        .map_err(|diagnostic| ScriptFault {
            tick,
            enemy_id: enemy.id,
            script_hash: self.version.hash.clone(),
            diagnostic,
        })
    }
}

#[cfg(test)]
mod tests;
