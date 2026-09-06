//! Arena session recording and deterministic re-execution through the live Runtime.
//!
//! Sessions begin at Runtime tick zero, including opening/menu/management ticks.
//! Replays restore the saved initial content and enqueue every committed bundle;
//! they never inject snapshots or bypass application validation and update order.

use crate::{
    arena::{self, config::ContentVersion},
    build_demo_runtime, DemoRuntime,
};
use anyhow::{ensure, Context, Result};
use kitu_osc_ir::OscBundle;
use kitu_runtime::InputMetadata;
use kitu_tsq1::recording::{bundle_bytes, Recording, TimedBundle};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};

/// Maximum session duration accepted by the initial in-memory recorder (one hour).
pub const MAX_TICKS: u64 = 216_000;
const RECORDING_VERSION: u32 = 1;

/// Build identity required to replay rules with the same execution semantics.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionVersion {
    /// Application package version.
    pub package: String,
    /// SHA-256 of rule/runtime sources and the locked dependency graph.
    pub source_hash: String,
    /// Compilation target; cross-target equivalence is validated separately.
    pub target: String,
}
impl ExecutionVersion {
    /// Identity of the currently compiled application.
    pub fn current() -> Self {
        Self {
            package: env!("CARGO_PKG_VERSION").into(),
            source_hash: env!("ARENA_EXECUTION_HASH").into(),
            target: env!("ARENA_EXECUTION_TARGET").into(),
        }
    }
}

/// Exact hashes of detached application state and ordered outputs after one tick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TickProof {
    /// Canonical logical application projection hash.
    pub state: String,
    /// All ordered output bundles, including receipts and domain events.
    pub output: String,
}

/// Complete metadata needed to reconstruct and validate a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    /// Arena recording schema, distinct from OSC and TSQ1 versions.
    pub version: u32,
    /// Stable application identifier.
    pub app: String,
    /// Compiled rules and Runtime identity.
    pub execution: ExecutionVersion,
    /// OSC application contract version.
    pub contract_version: u32,
    /// Fixed Runtime ticks per second.
    pub tick_rate: u32,
    /// Evaluated content present before the very first input/tick.
    pub initial_content: ContentVersion,
    /// Projection before execution; protects initial-condition compatibility.
    pub initial_state: String,
    /// Total completed management ticks, including pauses and empty input ticks.
    pub ticks: u64,
    /// One state/output proof per completed tick, in tick order.
    pub proofs: Vec<TickProof>,
    /// Detached run-start events containing each run's full frozen configuration.
    pub runs: Vec<Value>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InputIdentity {
    sequence: u64,
    identity: Option<InputMetadata>,
}

/// Recorder attached at creation of a live Runtime.
#[derive(Clone)]
pub struct Recorder {
    manifest: Manifest,
    inputs: Vec<TimedBundle>,
    encoded_size_bound: usize,
}
impl Recorder {
    /// Captures initial conditions without advancing or changing the Runtime.
    ///
    /// # Examples
    /// ```
    /// let mut runtime = kitu_demo_game::build_arena_runtime().unwrap();
    /// let mut recorder = kitu_demo_game::replay::Recorder::new(&runtime).unwrap();
    /// runtime.tick_once().unwrap();
    /// let outputs = runtime.drain_output_buffer();
    /// recorder.capture(&runtime, &outputs).unwrap();
    /// let session = kitu_demo_game::replay::Session::decode(&recorder.encode().unwrap()).unwrap();
    /// assert_eq!(session.verify().unwrap().ticks, 1);
    /// ```
    pub fn new(runtime: &DemoRuntime) -> Result<Self> {
        ensure!(
            runtime.current_tick().get() == 0,
            "recording must begin at Runtime creation"
        );
        let initial = arena::inspect_content(runtime).context("Arena is not installed")?;
        ensure!(
            initial.run == 0 && initial.active.is_none(),
            "recording requires initial Arena state"
        );
        let mut recorder = Self {
            manifest: Manifest {
                version: RECORDING_VERSION,
                app: "endless-arena".into(),
                execution: ExecutionVersion::current(),
                contract_version: arena::SCHEMA_VERSION,
                tick_rate: runtime.config().tick_rate_hz,
                initial_content: initial.pending,
                initial_state: hash_bundles(&runtime.inspect_application())?,
                ticks: 0,
                proofs: Vec::new(),
                runs: Vec::new(),
            },
            inputs: Vec::new(),
            encoded_size_bound: 0,
        };
        // Reserve growth from the initial one-digit tick count to any u64.
        recorder.encoded_size_bound = recorder.encode()?.len() + 20;
        Ok(recorder)
    }
    /// Records one successfully completed tick, before committed inputs are drained.
    /// Missing ticks, excessive sessions or invalid OSC fail without partial capture.
    pub fn capture(&mut self, runtime: &DemoRuntime, outputs: &[OscBundle]) -> Result<()> {
        ensure!(
            runtime.current_tick().get() == self.manifest.ticks + 1,
            "recording missed a tick"
        );
        ensure!(
            self.manifest.ticks < MAX_TICKS,
            "recording reached one-hour limit; start a new session"
        );
        let batch = runtime.committed_input_records();
        ensure!(
            self.inputs.len() + batch.len() <= kitu_tsq1::recording::MAX_ENTRIES,
            "recording input limit reached"
        );
        let entries = batch
            .into_iter()
            .enumerate()
            .map(|(order, input)| {
                Ok(TimedBundle {
                    tick: self.manifest.ticks,
                    order: u32::try_from(order)?,
                    metadata: serde_json::to_value(InputIdentity {
                        sequence: input.sequence,
                        identity: input.metadata,
                    })?,
                    bundle: input.bundle,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let proof = proof(runtime, outputs)?;
        let mut runs = Vec::new();
        for message in outputs.iter().flat_map(|bundle| &bundle.messages) {
            if message.address == "/game/arena/run" {
                let [kitu_osc_ir::OscArg::Str(json)] = message.args.as_slice() else {
                    anyhow::bail!("invalid run manifest output")
                };
                runs.push(serde_json::from_str(json)?);
            }
        }
        let mut size = self.encoded_size_bound;
        for (offset, entry) in entries.iter().enumerate() {
            let identity: InputIdentity = serde_json::from_value(entry.metadata.clone())?;
            ensure!(
                identity.sequence == (self.inputs.len() + offset) as u64,
                "recording queue sequence was skipped"
            );
            size = size
                .checked_add(entry.encoded_size_bound()?)
                .context("recording size overflow")?;
        }
        // These values are already JSON-owned or strings, so serde_json's value
        // conversion does not change their representation in the TSQ1 manifest.
        size = size
            .checked_add(serde_json::to_vec(&proof)?.len() + 1)
            .context("recording size overflow")?;
        for run in &runs {
            size = size
                .checked_add(serde_json::to_vec(run)?.len() + 1)
                .context("recording size overflow")?;
        }
        ensure!(
            size <= kitu_tsq1::recording::MAX_BYTES,
            "recording reached encoded-size limit; start a new session"
        );
        self.encoded_size_bound = size;
        self.inputs.extend(entries);
        self.manifest.proofs.push(proof);
        self.manifest.runs.extend(runs);
        self.manifest.ticks += 1;
        Ok(())
    }
    /// Encodes a detached point-in-time TSQ1 session, suitable for atomic disk saves.
    pub fn encode(&self) -> Result<Vec<u8>> {
        Recording {
            manifest: serde_json::to_value(&self.manifest)?,
            entries: self.inputs.clone(),
        }
        .encode()
    }
    /// Completed tick count available in this recorder.
    pub fn ticks(&self) -> u64 {
        self.manifest.ticks
    }
}

/// Validated, immutable TSQ1 session loaded for replay.
#[derive(Clone)]
pub struct Session {
    manifest: Manifest,
    inputs: Vec<TimedBundle>,
}

/// Result of a complete deterministic re-execution.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Verification {
    /// Completed ticks compared to saved state/output proofs.
    pub ticks: u64,
    /// Number of replayed bundles, including retransmissions.
    pub inputs: usize,
    /// Number of complete saved run-start manifests compared.
    pub runs: usize,
    /// Final authoritative application projection.
    pub state: Value,
}
impl Session {
    /// Loads real TSQ1, rejecting incompatible execution/contract versions and bounds.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let document = Recording::decode(bytes)?;
        let manifest: Manifest = serde_json::from_value(document.manifest)?;
        ensure!(
            manifest.version == RECORDING_VERSION && manifest.app == "endless-arena",
            "incompatible Arena recording version"
        );
        ensure!(
            manifest.execution == ExecutionVersion::current(),
            "incompatible Arena execution version"
        );
        ensure!(
            manifest.contract_version == arena::SCHEMA_VERSION && manifest.tick_rate == 60,
            "incompatible Arena contract or tick rate"
        );
        ensure!(
            manifest.ticks <= MAX_TICKS && manifest.proofs.len() as u64 == manifest.ticks,
            "invalid recording tick count"
        );
        manifest.initial_content.validate()?;
        ensure!(
            valid_hash(&manifest.initial_state)
                && manifest
                    .proofs
                    .iter()
                    .all(|p| valid_hash(&p.state) && valid_hash(&p.output)),
            "invalid state/output proof"
        );
        for (sequence, entry) in document.entries.iter().enumerate() {
            ensure!(
                entry.tick < manifest.ticks,
                "input lies outside the recording"
            );
            let identity: InputIdentity = serde_json::from_value(entry.metadata.clone())?;
            ensure!(
                identity.sequence == sequence as u64,
                "recorded queue order is not contiguous"
            );
        }
        Ok(Self {
            manifest,
            inputs: document.entries,
        })
    }
    /// Returns saved metadata, including all detached run configurations.
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }
    /// Creates a fresh Runtime with saved initial conditions and no authoring I/O.
    pub fn runtime(&self) -> Result<DemoRuntime> {
        let mut runtime = build_demo_runtime()?;
        arena::install_with_content(&mut runtime, self.manifest.initial_content.clone())?;
        ensure!(
            hash_bundles(&runtime.inspect_application())? == self.manifest.initial_state,
            "incompatible initial Arena state"
        );
        Ok(runtime)
    }
    /// Applies one recorded tick through the ordinary admission queue and tick loop.
    /// State and ordered outputs must match before the resulting projection is usable.
    /// The caller supplies a Runtime created by [`Self::runtime`], advanced in order.
    pub fn tick(&self, runtime: &mut DemoRuntime) -> Result<Vec<OscBundle>> {
        let tick = runtime.current_tick().get();
        ensure!(
            tick < self.manifest.ticks,
            "replay reached end of recording"
        );
        let first = self.inputs.partition_point(|entry| entry.tick < tick);
        for entry in self.inputs[first..]
            .iter()
            .take_while(|entry| entry.tick == tick)
        {
            let identity: InputIdentity = serde_json::from_value(entry.metadata.clone())?;
            let sequence = runtime
                .try_enqueue_input(entry.bundle.clone(), identity.identity)
                .with_context(|| {
                    format!(
                        "replay input rejected at tick {tick}, order {}",
                        entry.order
                    )
                })?;
            ensure!(
                sequence == identity.sequence,
                "replay queue sequence diverged at tick {tick}"
            );
        }
        runtime.tick_once()?;
        let outputs = runtime.drain_output_buffer();
        let actual = proof(runtime, &outputs)?;
        let expected = &self.manifest.proofs[tick as usize];
        ensure!(
            actual.state == expected.state,
            "replay state diverged at tick {tick}"
        );
        ensure!(
            actual.output == expected.output,
            "replay output/event order diverged at tick {tick}"
        );
        Ok(outputs)
    }
    /// Re-executes every tick and compares every state/output and frozen run manifest.
    pub fn verify(&self) -> Result<Verification> {
        let mut runtime = self.runtime()?;
        let mut recorder = Recorder::new(&runtime)?;
        while runtime.current_tick().get() < self.manifest.ticks {
            let outputs = self.tick(&mut runtime)?;
            recorder.capture(&runtime, &outputs)?;
        }
        ensure!(
            recorder.manifest.runs == self.manifest.runs,
            "saved run manifests diverged"
        );
        let projection = runtime.inspect_application();
        let kitu_osc_ir::OscArg::Str(json) = &projection[0].messages[0].args[0] else {
            unreachable!()
        };
        Ok(Verification {
            ticks: self.manifest.ticks,
            inputs: self.inputs.len(),
            runs: self.manifest.runs.len(),
            state: serde_json::from_str(json)?,
        })
    }
}

fn valid_hash(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit())
}
fn proof(runtime: &DemoRuntime, outputs: &[OscBundle]) -> Result<TickProof> {
    Ok(TickProof {
        state: hash_bundles(&runtime.inspect_application())?,
        output: hash_bundles(outputs)?,
    })
}
fn hash_bundles(bundles: &[OscBundle]) -> Result<String> {
    let mut hash = Sha256::new();
    hash.update((bundles.len() as u64).to_le_bytes());
    for bundle in bundles {
        let bytes = bundle_bytes(bundle)?;
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    Ok(hex::encode(hash.finalize()))
}

#[cfg(test)]
mod recording_bounds_tests {
    use super::*;
    use kitu_osc_ir::{OscArg, OscMessage};

    fn pending(runtime: &mut DemoRuntime, argument: OscArg) {
        let mut message = OscMessage::new("/test/unhandled");
        message.args.push(argument);
        runtime
            .try_enqueue_input(
                OscBundle {
                    messages: vec![message],
                },
                None,
            )
            .unwrap();
        runtime.tick_once().unwrap();
    }

    #[test]
    fn capture_rejects_unencodable_osc_without_poisoning_the_saved_prefix() {
        let mut runtime = crate::build_arena_runtime().unwrap();
        let mut recorder = Recorder::new(&runtime).unwrap();
        let prefix = recorder.encode().unwrap();
        pending(&mut runtime, OscArg::Float(f32::NAN));
        let outputs = runtime.drain_output_buffer();
        assert!(recorder
            .capture(&runtime, &outputs)
            .unwrap_err()
            .to_string()
            .contains("non-finite OSC float"));
        assert_eq!(recorder.encode().unwrap(), prefix);
        assert_eq!(recorder.ticks(), 0);
    }

    #[test]
    fn capture_checks_cumulative_encoded_bound_before_appending() {
        let mut runtime = crate::build_arena_runtime().unwrap();
        let mut recorder = Recorder::new(&runtime).unwrap();
        pending(&mut runtime, OscArg::Str("small".into()));
        let outputs = runtime.drain_output_buffer();
        recorder.capture(&runtime, &outputs).unwrap();
        assert!(recorder.encode().unwrap().len() <= recorder.encoded_size_bound);
        let prefix = recorder.encode().unwrap();
        // Model an almost-full recorder without allocating 64 MiB in every test.
        recorder.encoded_size_bound = kitu_tsq1::recording::MAX_BYTES - 1;
        pending(&mut runtime, OscArg::Str("next valid finite input".into()));
        let outputs = runtime.drain_output_buffer();
        assert!(recorder
            .capture(&runtime, &outputs)
            .unwrap_err()
            .to_string()
            .contains("encoded-size limit"));
        assert_eq!(recorder.encode().unwrap(), prefix);
        assert_eq!(recorder.ticks(), 1);
    }
}
