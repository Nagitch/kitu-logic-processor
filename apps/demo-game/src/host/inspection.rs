//! Coherent, bounded host observations; none of these values enter game outputs.
//!
//! State, version identities and presentation come from one last-verified
//! application projection. Owner timing and retained event summaries are local
//! diagnostics; inspection errors never interrupt a successful game tick.
use super::*;
use arena::presentation::PresentationSnapshot;
use kitu_transport::wire::WireMessageRef;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::VecDeque, io::Write, time::Duration};

const RESPONSE_BYTES: usize = 8 * 1024 * 1024;
const EVENT_CAPACITY: usize = 256;
const EVENT_BYTES: usize = 2 * 1024 * 1024;
const PAYLOAD_BYTES: usize = 64 * 1024;
const TIMING_CAPACITY: usize = 256;
const MAX_DURATION_US: f64 = 86_400_000_000.0;

/// One exact Arena state with only tick/simulationSteps converted to decimal text.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct InspectionArenaState(Value);
/// Compact presentation with explicitly selected wide fields as decimal text.
#[derive(Debug, Clone, Serialize)]
#[serde(transparent)]
pub struct InspectionPresentation(Value);
/// Existing playback fields with exact decimal tick and totalTicks values.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionReplayMode {
    active: bool,
    recording_id: Option<String>,
    tick: String,
    total_ticks: String,
    playing: bool,
    seeking: bool,
    error: Option<String>,
}
/// Script failure from the same verified projection as the inspected state.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionScriptFault {
    tick: String,
    enemy_id: i32,
    script_hash: String,
    diagnostic: kitu_scripting_rhai::Diagnostic,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct VersionHashes {
    active_hash: Option<String>,
    pending_hash: String,
}
#[derive(Debug, Clone, Serialize)]
struct Versions {
    content: VersionHashes,
    script: VersionHashes,
    timeline: VersionHashes,
}
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Diagnostics {
    host_error: Option<String>,
    recording_error: Option<String>,
    script_fault: Option<InspectionScriptFault>,
}
/// One complete read-only observation, serialized separately from deterministic OSC.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionSnapshot {
    schema_version: u32,
    session_id: String,
    revision: String,
    attempt: String,
    epoch: String,
    run: String,
    mode: InspectionReplayMode,
    read_only: bool,
    execution: kitu_transport::application::ExecutionVersion,
    versions: Versions,
    diagnostics: Diagnostics,
    state: InspectionArenaState,
    presentation: InspectionPresentation,
    arena: arena::geometry::ArenaGeometry,
    events: InspectionEventWindow,
    timing: InspectionTiming,
}
/// Retained source-order event metadata; full sources in run messages are summarized.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionEvent {
    sequence: String,
    revision: String,
    epoch: String,
    run: String,
    tick: String,
    bundle_index: u32,
    message_index: u32,
    address: String,
    detail: EventDetail,
}
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum EventDetail {
    Message {
        #[serde(rename = "messageJson")]
        message_json: String,
    },
    Run {
        #[serde(rename = "contentHash")]
        content_hash: String,
        #[serde(rename = "scriptHash")]
        script_hash: String,
        #[serde(rename = "timelineHash")]
        timeline_hash: String,
    },
    Oversize {
        #[serde(rename = "encodedBytes")]
        encoded_bytes: String,
        sha256: String,
    },
}
/// Bounded full history window, with explicit eviction and oversized-payload counts.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionEventWindow {
    capacity: usize,
    byte_limit: usize,
    payload_byte_limit: usize,
    first_sequence: Option<String>,
    last_sequence: Option<String>,
    dropped: String,
    omitted_payloads: String,
    entries: Vec<InspectionEvent>,
}
/// Actual owner work; replacements need not imply a source tick was executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Outcome {
    Idle,
    Advanced,
    Replacement,
    Fault,
}
/// One measured owner attempt, including failures that do not create a publication.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionTimingSample {
    attempt: String,
    revision: String,
    tick: String,
    outcome: Outcome,
    runtime_advanced: bool,
    simulation_advanced: bool,
    duration_us: f64,
    lock_wait_us: f64,
    duration_clamped: bool,
    lock_wait_clamped: bool,
    over_budget: bool,
}
/// Last256 owner samples from this epoch/run; statistics describe only that window.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InspectionTiming {
    scope: &'static str,
    capacity: usize,
    budget_us: f64,
    max_duration_us: f64,
    total_samples: String,
    over_budget_samples: String,
    last: Option<InspectionTimingSample>,
    samples: Vec<InspectionTimingSample>,
    mean_us: Option<f64>,
    p95_us: Option<f64>,
    max_us: Option<f64>,
}

#[derive(Clone)]
struct Projection {
    tick: i64,
    simulation_steps: u64,
    run: u64,
    state: InspectionArenaState,
    presentation: InspectionPresentation,
    versions: Versions,
    script_fault: Option<InspectionScriptFault>,
}
#[derive(Deserialize)]
struct HashOnly {
    hash: String,
}
#[derive(Deserialize)]
struct SourceSnapshot {
    run: u64,
    active: Option<HashOnly>,
    pending: HashOnly,
}
#[derive(Deserialize)]
struct ScriptSourceSnapshot {
    #[serde(flatten)]
    source: SourceSnapshot,
    fault: Option<arena::script::ScriptFault>,
}
#[derive(Deserialize)]
struct TimelineSourceSnapshot {
    #[serde(flatten)]
    source: SourceSnapshot,
    presentation: PresentationSnapshot,
}
fn hash(value: &str) -> Result<String> {
    anyhow::ensure!(
        value.len() == 64
            && value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "invalid inspection version hash"
    );
    Ok(value.into())
}
fn hashes(source: SourceSnapshot) -> Result<VersionHashes> {
    Ok(VersionHashes {
        active_hash: source.active.map(|v| hash(&v.hash)).transpose()?,
        pending_hash: hash(&source.pending.hash)?,
    })
}
fn json_at<'a>(bundles: &'a [OscBundle], address: &str) -> Result<&'a str> {
    let mut found = None;
    for message in bundles
        .iter()
        .flat_map(|b| &b.messages)
        .filter(|m| m.address == address)
    {
        anyhow::ensure!(found.is_none(), "duplicate inspection projection {address}");
        let [OscArg::Str(value)] = message.args.as_slice() else {
            anyhow::bail!("invalid inspection projection {address}");
        };
        anyhow::ensure!(
            value.len() <= RESPONSE_BYTES,
            "inspection projection exceeds response bound"
        );
        found = Some(value.as_str());
    }
    found.with_context(|| format!("missing inspection projection {address}"))
}
fn numeric_state(value: &Value) -> Result<()> {
    match value {
        Value::Null => anyhow::bail!("nonfinite/null Arena state value"),
        Value::Array(values) => {
            for v in values {
                numeric_state(v)?;
            }
        }
        Value::Object(values) => {
            for v in values.values() {
                numeric_state(v)?;
            }
        }
        _ => (),
    }
    Ok(())
}
impl Projection {
    fn capture(bundles: &[OscBundle]) -> Result<Self> {
        let state: arena::ArenaState = serde_json::from_str(json_at(bundles, "/ui/arena/state")?)?;
        let compact: PresentationSnapshot =
            serde_json::from_str(json_at(bundles, "/render/arena/presentation")?)?;
        let content: SourceSnapshot = serde_json::from_str(json_at(bundles, "/ui/arena/content")?)?;
        let script: ScriptSourceSnapshot =
            serde_json::from_str(json_at(bundles, "/ui/arena/script")?)?;
        let timeline: TimelineSourceSnapshot =
            serde_json::from_str(json_at(bundles, "/ui/arena/timeline")?)?;
        anyhow::ensure!(
            state.tick >= -1
                && state.tick == compact.tick
                && state.simulation_steps == compact.simulation_step
                && content.run == compact.run
                && script.source.run == compact.run
                && timeline.source.run == compact.run
                && timeline.presentation == compact,
            "incoherent inspection projection"
        );
        anyhow::ensure!(
            compact.contract_version == arena::presentation::CONTRACT_VERSION,
            "unsupported inspection presentation version"
        );
        let mut state_json = serde_json::to_value(&state)?;
        numeric_state(&state_json)?;
        state_json["tick"] = state.tick.to_string().into();
        state_json["simulationSteps"] = state.simulation_steps.to_string().into();
        let mut presentation = serde_json::to_value(&compact)?;
        presentation["run"] = compact.run.to_string().into();
        presentation["tick"] = compact.tick.to_string().into();
        presentation["simulationStep"] = compact.simulation_step.to_string().into();
        for (json, cue) in presentation["bosses"]
            .as_array_mut()
            .context("boss array")?
            .iter_mut()
            .zip(&compact.bosses)
        {
            anyhow::ensure!(
                cue.started_tick >= -1 && cue.radius.is_finite() && cue.intensity.is_finite(),
                "invalid boss cue values"
            );
            json["startedTick"] = cue.started_tick.to_string().into();
            json["offsetTick"] = cue.offset_tick.to_string().into();
        }
        if let Some(cue) = &compact.floor {
            anyhow::ensure!(
                cue.started_tick >= -1 && cue.opacity.is_finite(),
                "invalid floor cue values"
            );
            presentation["floor"]["startedTick"] = cue.started_tick.to_string().into();
            presentation["floor"]["offsetTick"] = cue.offset_tick.to_string().into();
        }
        let script_fault = script.fault.map(|fault| InspectionScriptFault {
            tick: fault.tick.to_string(),
            enemy_id: fault.enemy_id,
            script_hash: fault.script_hash,
            diagnostic: fault.diagnostic,
        });
        Ok(Self {
            tick: state.tick,
            simulation_steps: state.simulation_steps,
            run: compact.run,
            state: InspectionArenaState(state_json),
            presentation: InspectionPresentation(presentation),
            versions: Versions {
                content: hashes(content)?,
                script: hashes(script.source)?,
                timeline: hashes(timeline.source)?,
            },
            script_fault,
        })
    }
}

/// Data retained exclusively by the host, never the Runtime world or recorder.
pub(super) struct Inspection {
    projection: Option<Arc<Projection>>,
    error: Option<String>,
    host_error: Option<String>,
    attempt: u64,
    epoch: u64,
    next_sequence: u64,
    events: VecDeque<(InspectionEvent, usize)>,
    event_bytes: usize,
    dropped: u64,
    omitted_payloads: u64,
    samples: VecDeque<InspectionTimingSample>,
    total_samples: u64,
    over_budget_samples: u64,
}
fn bounded(error: impl std::fmt::Display) -> String {
    // Formatting through a bounded writer avoids building an unbounded diagnostic.
    struct Text(String);
    impl std::fmt::Write for Text {
        fn write_str(&mut self, s: &str) -> std::fmt::Result {
            let mut n = s.len().min(4096 - self.0.len());
            while !s.is_char_boundary(n) {
                n -= 1;
            }
            self.0.push_str(&s[..n]);
            if n != s.len() {
                Err(std::fmt::Error)
            } else {
                Ok(())
            }
        }
    }
    let mut output = Text(String::new());
    if std::fmt::write(&mut output, format_args!("{error}")).is_err() {
        const SUFFIX: &str = "… [truncated]";
        let mut n = output.0.len().min(4096 - SUFFIX.len());
        while !output.0.is_char_boundary(n) {
            n -= 1;
        }
        output.0.truncate(n);
        output.0.push_str(SUFFIX);
    }
    output.0
}
impl Inspection {
    pub(super) fn new(projection: &[OscBundle]) -> Self {
        let captured = Projection::capture(projection);
        let (projection, error) = match captured {
            Ok(value) => (Some(Arc::new(value)), None),
            Err(error) => (None, Some(bounded(error))),
        };
        Self {
            projection,
            error,
            host_error: None,
            attempt: 0,
            epoch: 0,
            next_sequence: 0,
            events: VecDeque::new(),
            event_bytes: 0,
            dropped: 0,
            omitted_payloads: 0,
            samples: VecDeque::new(),
            total_samples: 0,
            over_budget_samples: 0,
        }
    }
    fn disable(&mut self, error: impl std::fmt::Display) {
        if self.error.is_none() {
            self.error = Some(bounded(error));
        }
    }
    pub(super) fn begin_attempt(&mut self) {
        if self.error.is_some() {
            return;
        }
        match self.attempt.checked_add(1) {
            Some(value) => self.attempt = value,
            None => self.disable("inspection attempt counter exhausted"),
        }
    }
    fn reset_timing(&mut self) {
        self.samples.clear();
        self.total_samples = 0;
        self.over_budget_samples = 0;
    }
    fn commit(
        &mut self,
        bundles: &[OscBundle],
        output: &[OscBundle],
        revision: u64,
        replacement: bool,
        runtime_advanced: bool,
        before: Option<(u64, u64)>,
    ) -> Result<bool> {
        let next = Projection::capture(bundles)?;
        let previous = self
            .projection
            .as_ref()
            .context("inspection has no valid initial projection")?;
        let previous_run = before.map_or(previous.run, |p| p.0);
        let simulation_advanced = runtime_advanced
            && next.simulation_steps > before.context("missing pre-tick inspection position")?.1;
        let reset_timing = replacement || previous_run != next.run;
        if replacement {
            self.epoch = self
                .epoch
                .checked_add(1)
                .context("inspection epoch counter exhausted")?;
            self.events.clear();
            self.event_bytes = 0;
            self.dropped = 0;
            self.omitted_payloads = 0;
        }
        if reset_timing {
            self.reset_timing();
        }
        if runtime_advanced {
            // A returned live Runtime may have a different run from the replay.
            // Its pre-tick run is supplied by the caller in the output walk.
            self.capture_events(output, revision, next.tick, previous_run)?;
        }
        self.projection = Some(Arc::new(next));
        Ok(simulation_advanced)
    }
    fn capture_events(
        &mut self,
        bundles: &[OscBundle],
        revision: u64,
        tick: i64,
        mut run: u64,
    ) -> Result<()> {
        for (bundle_index, bundle) in bundles.iter().enumerate() {
            for (message_index, message) in bundle.messages.iter().enumerate() {
                if !message.address.starts_with("/game/arena/")
                    && !matches!(
                        message.address.as_str(),
                        "/ui/arena/command" | "/ui/arena/use" | "/ui/arena/timeline/event"
                    )
                {
                    continue;
                }
                let mut event_run = run;
                let detail = if message.address == "/game/arena/run" {
                    #[derive(Deserialize)]
                    struct Run {
                        run: u64,
                        content: HashOnly,
                        script: HashOnly,
                        timeline: HashOnly,
                    }
                    let [OscArg::Str(raw)] = message.args.as_slice() else {
                        anyhow::bail!("invalid run inspection event");
                    };
                    let value: Run = serde_json::from_str(raw)?;
                    run = value.run;
                    event_run = run;
                    EventDetail::Run {
                        content_hash: hash(&value.content.hash)?,
                        script_hash: hash(&value.script.hash)?,
                        timeline_hash: hash(&value.timeline.hash)?,
                    }
                } else {
                    if message.address == "/ui/arena/timeline/event" {
                        #[derive(Deserialize)]
                        struct Run {
                            run: u64,
                        }
                        let [OscArg::Str(raw)] = message.args.as_slice() else {
                            anyhow::bail!("invalid timeline inspection event");
                        };
                        event_run = serde_json::from_str::<Run>(raw)?.run;
                    }
                    event_detail(message)?
                };
                let event = InspectionEvent {
                    sequence: self.next_sequence.to_string(),
                    revision: revision.to_string(),
                    epoch: self.epoch.to_string(),
                    run: event_run.to_string(),
                    tick: tick.to_string(),
                    bundle_index: bundle_index.try_into()?,
                    message_index: message_index.try_into()?,
                    address: message.address.clone(),
                    detail,
                };
                self.next_sequence = self
                    .next_sequence
                    .checked_add(1)
                    .context("inspection event counter exhausted")?;
                if matches!(event.detail, EventDetail::Oversize { .. }) {
                    self.omitted_payloads = self
                        .omitted_payloads
                        .checked_add(1)
                        .context("inspection omission counter exhausted")?;
                }
                let mut size = CountWriter::default();
                serde_json::to_writer(&mut size, &event)?;
                anyhow::ensure!(
                    size.bytes <= EVENT_BYTES,
                    "inspection event metadata exceeds retention bound"
                );
                self.event_bytes += size.bytes;
                self.events.push_back((event, size.bytes));
                while self.events.len() > EVENT_CAPACITY || self.event_bytes > EVENT_BYTES {
                    self.event_bytes -= self.events.pop_front().expect("nonempty bounded ring").1;
                    self.dropped = self
                        .dropped
                        .checked_add(1)
                        .context("inspection eviction counter exhausted")?;
                }
            }
        }
        Ok(())
    }
    fn finish(
        &mut self,
        revision: u64,
        outcome: Outcome,
        runtime_advanced: bool,
        simulation_advanced: bool,
        duration: Duration,
        lock_wait: Duration,
    ) -> Result<()> {
        let duration_us = duration.as_secs_f64() * 1_000_000.0;
        let lock_wait_us = lock_wait.as_secs_f64() * 1_000_000.0;
        let over_budget = duration.as_nanos() * 60 > 1_000_000_000;
        self.total_samples = self
            .total_samples
            .checked_add(1)
            .context("inspection sample counter exhausted")?;
        if over_budget {
            self.over_budget_samples = self
                .over_budget_samples
                .checked_add(1)
                .context("inspection over-budget counter exhausted")?;
        }
        self.samples.push_back(InspectionTimingSample {
            attempt: self.attempt.to_string(),
            revision: revision.to_string(),
            tick: self
                .projection
                .as_ref()
                .context("missing inspection state")?
                .tick
                .to_string(),
            outcome,
            runtime_advanced,
            simulation_advanced,
            duration_us: duration_us.min(MAX_DURATION_US),
            lock_wait_us: lock_wait_us.min(MAX_DURATION_US),
            duration_clamped: duration_us > MAX_DURATION_US,
            lock_wait_clamped: lock_wait_us > MAX_DURATION_US,
            over_budget,
        });
        if self.samples.len() > TIMING_CAPACITY {
            self.samples.pop_front();
        }
        Ok(())
    }
}

#[derive(Default)]
struct CountWriter {
    bytes: usize,
}
impl Write for CountWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len())
            .ok_or_else(|| std::io::Error::other("inspection size overflow"))?;
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
struct EventWriter {
    bytes: u64,
    hash: Sha256,
    prefix: Vec<u8>,
}
impl Write for EventWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.bytes = self
            .bytes
            .checked_add(bytes.len() as u64)
            .ok_or_else(|| std::io::Error::other("inspection event size overflow"))?;
        self.hash.update(bytes);
        if self.bytes <= PAYLOAD_BYTES as u64 {
            let needed = self.bytes as usize;
            if needed > self.prefix.capacity() {
                let target = needed
                    .max(self.prefix.capacity().saturating_mul(2).max(1024))
                    .min(PAYLOAD_BYTES);
                self.prefix
                    .try_reserve_exact(target - self.prefix.len())
                    .map_err(std::io::Error::other)?;
            }
            self.prefix.extend_from_slice(bytes);
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn event_detail(message: &OscMessage) -> Result<EventDetail> {
    let mut writer = EventWriter {
        bytes: 0,
        hash: Sha256::new(),
        prefix: Vec::new(),
    };
    serde_json::to_writer(&mut writer, &WireMessageRef::new(message))?;
    if writer.bytes <= PAYLOAD_BYTES as u64 {
        Ok(EventDetail::Message {
            message_json: String::from_utf8(writer.prefix)?,
        })
    } else {
        Ok(EventDetail::Oversize {
            encoded_bytes: writer.bytes.to_string(),
            sha256: hex::encode(writer.hash.finalize()),
        })
    }
}

/// Context supplied by the real owner branch, never inferred from a larger seek tick.
pub(super) struct Update {
    pub replacement: bool,
    pub runtime_advanced: bool,
    pub outcome: Outcome,
    pub before: Option<(u64, u64)>,
}
/// Read the verified pre-tick identity without touching an advanced failed replay Runtime.
pub(super) fn position(game: &GameState) -> Option<(u64, u64)> {
    game.inspection
        .projection
        .as_ref()
        .map(|p| (p.run, p.simulation_steps))
}
/// A control just replaced the selected Runtime; read its actual pre-tick position.
pub(super) fn replacement_position(game: &mut GameState) -> Option<(u64, u64)> {
    let bundles = game.application_projection();
    let result = json_at(&bundles, "/render/arena/presentation")
        .and_then(|raw| Ok(serde_json::from_str::<PresentationSnapshot>(raw)?));
    match result {
        Ok(value) => Some((value.run, value.simulation_step)),
        Err(error) => {
            game.inspection.disable(error);
            None
        }
    }
}
/// Capture errors are sticky inspection errors; successful game outputs remain intact.
pub(super) fn capture(game: &mut GameState, output: &[OscBundle], update: &Update) -> bool {
    if game.inspection.error.is_some() {
        return false;
    }
    let projection = game.application_projection();
    match game.inspection.commit(
        &projection,
        output,
        game.publication_id,
        update.replacement,
        update.runtime_advanced,
        update.before,
    ) {
        Ok(advanced) => advanced,
        Err(error) => {
            game.inspection.disable(error);
            false
        }
    }
}
/// Finalize a measured attempt without propagating observer errors into gameplay.
pub(super) fn finish(
    game: &mut GameState,
    update: &Update,
    simulation_advanced: bool,
    error: Option<&str>,
    duration: Duration,
    wait: Duration,
) {
    if game.inspection.error.is_some() {
        return;
    }
    game.inspection.host_error = error.map(bounded);
    if let Err(error) = game.inspection.finish(
        game.publication_id,
        update.outcome,
        update.runtime_advanced,
        simulation_advanced,
        duration,
        wait,
    ) {
        game.inspection.disable(error);
    }
}

fn detached(game: &GameState) -> Result<InspectionSnapshot> {
    let inspection = &game.inspection;
    if let Some(error) = &inspection.error {
        anyhow::bail!("Arena inspection unavailable: {error}");
    }
    let projection = inspection
        .projection
        .as_ref()
        .context("Arena inspection unavailable")?;
    let mode = playback::mode(game);
    Ok(InspectionSnapshot {
        schema_version: 1,
        session_id: game.runtime_id.clone(),
        revision: game.publication_id.to_string(),
        attempt: inspection.attempt.to_string(),
        epoch: inspection.epoch.to_string(),
        run: projection.run.to_string(),
        mode: InspectionReplayMode {
            active: mode.active,
            recording_id: mode.recording_id,
            tick: projection.tick.to_string(),
            total_ticks: mode.total_ticks.to_string(),
            playing: mode.playing,
            seeking: mode.seeking,
            error: mode.error.map(bounded),
        },
        read_only: game.ensure_live_input().is_err(),
        execution: arena_wire::execution(),
        versions: projection.versions.clone(),
        diagnostics: Diagnostics {
            host_error: inspection.host_error.clone(),
            recording_error: game.recording_error.as_ref().map(bounded),
            script_fault: projection.script_fault.clone(),
        },
        state: projection.state.clone(),
        presentation: projection.presentation.clone(),
        arena: arena::geometry::inspection_geometry(),
        events: InspectionEventWindow {
            capacity: EVENT_CAPACITY,
            byte_limit: EVENT_BYTES,
            payload_byte_limit: PAYLOAD_BYTES,
            first_sequence: inspection.events.front().map(|(e, _)| e.sequence.clone()),
            last_sequence: inspection.events.back().map(|(e, _)| e.sequence.clone()),
            dropped: inspection.dropped.to_string(),
            omitted_payloads: inspection.omitted_payloads.to_string(),
            entries: inspection.events.iter().map(|(e, _)| e.clone()).collect(),
        },
        timing: InspectionTiming {
            scope: "hostUpdate",
            capacity: TIMING_CAPACITY,
            budget_us: 1_000_000.0 / 60.0,
            max_duration_us: MAX_DURATION_US,
            total_samples: inspection.total_samples.to_string(),
            over_budget_samples: inspection.over_budget_samples.to_string(),
            last: inspection.samples.back().cloned(),
            samples: inspection.samples.iter().cloned().collect(),
            mean_us: None,
            p95_us: None,
            max_us: None,
        },
    })
}
fn statistics(snapshot: &mut InspectionSnapshot) {
    if snapshot.timing.samples.is_empty() {
        return;
    }
    let mut values: Vec<_> = snapshot
        .timing
        .samples
        .iter()
        .map(|s| s.duration_us)
        .collect();
    values.sort_by(f64::total_cmp);
    snapshot.timing.mean_us = Some(values.iter().sum::<f64>() / values.len() as f64);
    snapshot.timing.p95_us = Some(values[(95 * values.len()).div_ceil(100) - 1]);
    snapshot.timing.max_us = values.last().copied();
}
pub(super) fn inspect(state: &AppState) -> Result<InspectionSnapshot> {
    let mut snapshot = {
        let game = state
            .inner
            .lock()
            .map_err(|_| anyhow::anyhow!("state lock poisoned"))?;
        detached(&game)?
    };
    statistics(&mut snapshot);
    Ok(snapshot)
}
struct ResponseWriter(Vec<u8>);
impl Write for ResponseWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let needed = self
            .0
            .len()
            .checked_add(bytes.len())
            .filter(|n| *n <= RESPONSE_BYTES)
            .ok_or_else(|| std::io::Error::other("inspection response exceeds 8 MiB"))?;
        if needed > self.0.capacity() {
            let target = needed
                .max(self.0.capacity().saturating_mul(2).max(1024))
                .min(RESPONSE_BYTES);
            self.0
                .try_reserve_exact(target - self.0.len())
                .map_err(std::io::Error::other)?;
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
pub(super) async fn get(State(state): State<AppState>) -> axum::response::Response {
    let result = inspect(&state).and_then(|snapshot| {
        let mut writer = ResponseWriter(Vec::new());
        serde_json::to_writer(&mut writer, &snapshot)?;
        Ok(writer.0)
    });
    use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
    match result {
        Ok(body) => (
            [
                (CACHE_CONTROL, "no-store"),
                (CONTENT_TYPE, "application/json"),
            ],
            body,
        )
            .into_response(),
        Err(error) => (
            StatusCode::SERVICE_UNAVAILABLE,
            [(CACHE_CONTROL, "no-store")],
            Json(serde_json::json!({"error":bounded(error)})),
        )
            .into_response(),
    }
}

#[cfg(test)]
mod tests;
