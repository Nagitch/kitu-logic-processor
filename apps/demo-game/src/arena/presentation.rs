//! Detached TSQ1 presentation, advanced only by successful Arena gameplay steps.
//!
//! ```
//! use kitu_demo_game::arena::presentation::{default_timeline, validate_version};
//! let version = default_timeline().unwrap();
//! validate_version(&version).unwrap();
//! assert_eq!(version.clips[0].id, "boss-telegraph");
//! ```

use super::ArenaState;
use anyhow::{ensure, Context, Result};
use kitu_osc_ir::{OscArg, OscBundle};
use kitu_transport::wire::WireBundle;
use kitu_tsq1::presentation::Clip;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::OpenOptions,
    io::Read,
    path::Path,
    sync::{Arc, OnceLock},
};

/// Application presentation schema, separate from the TSQ1 framing version.
pub const CONTRACT_VERSION: u32 = 1;
/// Maximum JSON source version admitted by the management queue.
pub const MAX_VERSION_BYTES: usize = 128 * 1024;
/// Bundled warning source, with no authoring filesystem dependency.
pub const DEFAULT_BOSS: &[u8] = include_bytes!("../../content/timelines/boss-telegraph.tsq");
/// Bundled floor fade source, with no authoring filesystem dependency.
pub const DEFAULT_FLOOR: &[u8] = include_bytes!("../../content/timelines/floor-transition.tsq");
const IDS: [&str; 2] = ["boss-telegraph", "floor-transition"];

/// Exact TSQ1 bytes and their content identity, independent of authoring paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClipSource {
    /// Stable application clip identifier.
    pub id: String,
    /// SHA-256 of the exact bytes, including supported TSQ1 framing.
    pub source_sha256: String,
    /// Detached, bounded TSQ1 source.
    pub bytes: Vec<u8>,
}
/// Immutable presentation sources saved with each run and replay.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimelineVersion {
    /// SHA-256 of contract, tick rate and ordered clip identities.
    pub hash: String,
    /// Arena presentation schema.
    pub contract_version: u32,
    /// Integer gameplay updates per second.
    pub tick_rate: u32,
    /// Exactly boss-telegraph then floor-transition.
    pub clips: Vec<ClipSource>,
}
/// A live warning for one actual boss telegraph phase.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BossCue {
    /// Unique instance identity within the run.
    pub id: String,
    /// Source clip identifier.
    pub clip_id: String,
    /// Authoritative enemy entity identifier.
    pub entity_id: i32,
    /// Floor where this warning started.
    pub floor: i32,
    /// Management tick of the triggering state change.
    pub started_tick: i64,
    /// Successful gameplay steps since the trigger.
    pub offset_tick: u64,
    /// Number of consumed TSQ1 bundle events.
    pub next_event_index: u32,
    /// Total authored bundle events, including empty bundles.
    pub event_count: u32,
    /// Last authored world-space warning radius.
    pub radius: f32,
    /// Last authored warning intensity, in [0, 1].
    pub intensity: f32,
}
/// A floor transition fade whose lifetime follows the authored clip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FloorCue {
    /// Unique instance identity within the run.
    pub id: String,
    /// Source clip identifier.
    pub clip_id: String,
    /// Floor being left.
    pub from_floor: i32,
    /// Floor being entered.
    pub to_floor: i32,
    /// Management tick of the trigger.
    pub started_tick: i64,
    /// Successful gameplay steps since the trigger.
    pub offset_tick: u64,
    /// Number of consumed TSQ1 bundle events.
    pub next_event_index: u32,
    /// Total authored bundle events.
    pub event_count: u32,
    /// Last authored opacity, in [0, 1].
    pub opacity: f32,
}
/// Complete compact render state; clients never advance these clocks themselves.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationSnapshot {
    /// Arena presentation schema.
    pub contract_version: u32,
    /// Number of accepted starts.
    pub run: u64,
    /// Current management tick, including paused ticks.
    pub tick: i64,
    /// Authoritative cumulative successful gameplay step count.
    pub simulation_step: u64,
    /// Live warnings sorted by entity identifier.
    pub bosses: Vec<BossCue>,
    /// At most one live floor fade.
    pub floor: Option<FloorCue>,
}
impl Default for PresentationSnapshot {
    fn default() -> Self {
        Self {
            contract_version: CONTRACT_VERSION,
            run: 0,
            tick: -1,
            simulation_step: 0,
            bosses: Vec::new(),
            floor: None,
        }
    }
}
/// Detached active/pending sources plus their coherent current render state.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimelineSnapshot {
    /// Number of accepted starts.
    pub run: u64,
    /// Sources selected at the last accepted start.
    pub active: Option<TimelineVersion>,
    /// Validated sources to adopt on the next start/retry.
    pub pending: TimelineVersion,
    /// Current cue clocks and values.
    pub presentation: PresentationSnapshot,
}

fn sha(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
fn identity(boss: &[u8], floor: &[u8]) -> TimelineVersion {
    let clips = IDS
        .into_iter()
        .zip([boss, floor])
        .map(|(id, bytes)| ClipSource {
            id: id.into(),
            source_sha256: sha(bytes),
            bytes: bytes.into(),
        })
        .collect::<Vec<_>>();
    let hashes = clips
        .iter()
        .map(|clip| (&clip.id, &clip.source_sha256))
        .collect::<Vec<_>>();
    let hash =
        sha(&serde_json::to_vec(&(CONTRACT_VERSION, 60u32, hashes)).expect("timeline identity"));
    TimelineVersion {
        hash,
        contract_version: CONTRACT_VERSION,
        tick_rate: 60,
        clips,
    }
}
impl TimelineVersion {
    /// Validates two detached real TSQ1 clips in fixed boss/floor order.
    pub fn from_sources(boss: &[u8], floor: &[u8]) -> Result<Self> {
        ensure!(
            boss.len() <= kitu_tsq1::presentation::MAX_BYTES
                && floor.len() <= kitu_tsq1::presentation::MAX_BYTES,
            "Arena clip source exceeds 8 KiB"
        );
        let version = identity(boss, floor);
        prepare_version(&version)?;
        Ok(version)
    }
}
/// Loads two regular files once. Each opened handle is bounded before decoding.
/// No path is included in the detached version or used during replay.
pub fn load_timeline(directory: &Path) -> Result<TimelineVersion> {
    ensure!(
        !directory.as_os_str().is_empty(),
        "timeline source is disabled for this host"
    );
    let read = |id: &str| -> Result<Vec<u8>> {
        let path = directory.join(format!("{id}.tsq"));
        let mut options = OpenOptions::new();
        options.read(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.custom_flags(libc::O_NONBLOCK);
        }
        let file = options
            .open(&path)
            .with_context(|| bounded(format!("{}: cannot open timeline source", path.display())))?;
        ensure!(
            file.metadata()?.is_file(),
            "Arena timeline source must be a regular file"
        );
        let mut bytes = Vec::new();
        file.take(kitu_tsq1::presentation::MAX_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(
            bytes.len() <= kitu_tsq1::presentation::MAX_BYTES,
            "Arena clip source exceeds 8 KiB"
        );
        Ok(bytes)
    };
    TimelineVersion::from_sources(&read(IDS[0])?, &read(IDS[1])?)
}
fn bounded(mut message: String) -> String {
    if message.len() > 1024 {
        let mut end = 1024;
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        message.truncate(end);
    }
    message
}
/// Returns immutable bundled presentation sources without authoring I/O.
pub fn default_timeline() -> Result<TimelineVersion> {
    Ok(default_prepared()?.version.clone())
}
/// Checks detached hashes, exact framing, rate and every whitelisted render operation.
pub fn validate_version(version: &TimelineVersion) -> Result<()> {
    prepare_version(version).map(|_| ())
}

/// Decoded sources retained by catalogs, Arena and replay sessions outside clock locks.
pub(crate) struct PreparedTimeline {
    pub version: TimelineVersion,
    clips: [Clip; 2],
}
pub(crate) fn default_prepared() -> Result<Arc<PreparedTimeline>> {
    static DEFAULT: OnceLock<Arc<PreparedTimeline>> = OnceLock::new();
    if let Some(prepared) = DEFAULT.get() {
        return Ok(prepared.clone());
    }
    let prepared = prepare_version(&identity(DEFAULT_BOSS, DEFAULT_FLOOR))?;
    let _ = DEFAULT.set(prepared.clone());
    Ok(prepared)
}
pub(crate) fn prepare_version(version: &TimelineVersion) -> Result<Arc<PreparedTimeline>> {
    ensure!(
        version.contract_version == CONTRACT_VERSION
            && version.tick_rate == 60
            && version.clips.len() == 2,
        "incompatible Arena timeline contract, rate or clip set"
    );
    for (clip, id) in version.clips.iter().zip(IDS) {
        ensure!(
            clip.id == id && clip.bytes.len() <= kitu_tsq1::presentation::MAX_BYTES,
            "invalid Arena timeline source identity or size"
        );
    }
    ensure!(
        version == &identity(&version.clips[0].bytes, &version.clips[1].bytes),
        "Arena timeline hash or source identity mismatch"
    );
    ensure!(
        serde_json::to_vec(version)?.len() <= MAX_VERSION_BYTES,
        "Arena timeline version exceeds 128 KiB"
    );
    let boss = Clip::decode(&version.clips[0].bytes).context("boss-telegraph")?;
    let floor = Clip::decode(&version.clips[1].bytes).context("floor-transition")?;
    validate_clip(&boss, true)?;
    validate_clip(&floor, false)?;
    Ok(Arc::new(PreparedTimeline {
        version: version.clone(),
        clips: [boss, floor],
    }))
}
fn validate_clip(clip: &Clip, boss: bool) -> Result<()> {
    ensure!(clip.tick_rate == 60, "Arena timeline requires 60 Hz");
    let mut initial = false;
    let mut last_opacity = None;
    for event in &clip.events {
        for message in &event.bundle.messages {
            if boss {
                ensure!(
                    message.address == "/render/arena/cue/boss",
                    "boss clip contains unsupported OSC address"
                );
                let [OscArg::Float(radius), OscArg::Float(intensity)] = message.args.as_slice()
                else {
                    anyhow::bail!("boss cue requires two f32 arguments")
                };
                ensure!(
                    radius.is_finite()
                        && intensity.is_finite()
                        && (0.25..=8.0).contains(radius)
                        && (0.0..=1.0).contains(intensity),
                    "boss cue radius/intensity outside allowed range"
                );
            } else {
                ensure!(
                    message.address == "/render/arena/cue/floor",
                    "floor clip contains unsupported OSC address"
                );
                let [OscArg::Float(opacity)] = message.args.as_slice() else {
                    anyhow::bail!("floor cue requires one f32 argument")
                };
                ensure!(
                    opacity.is_finite() && (0.0..=1.0).contains(opacity),
                    "floor cue opacity outside allowed range"
                );
                last_opacity = Some(*opacity);
            }
            initial |= event.offset_tick == 0;
        }
    }
    ensure!(initial, "Arena clip requires an assignment at offset zero");
    ensure!(
        boss || last_opacity == Some(0.0),
        "floor clip must finish with opacity zero"
    );
    Ok(())
}

#[derive(Default)]
pub(super) struct Presentation {
    pub snapshot: PresentationSnapshot,
    serial: u64,
}
impl Presentation {
    pub fn synchronize(&mut self, run: u64, state: &ArenaState, tick: i64) {
        self.snapshot.run = run;
        self.snapshot.tick = tick;
        self.snapshot.simulation_step = state.simulation_steps;
    }
    fn id(&mut self) -> String {
        self.serial += 1;
        format!("r{}:c{}", self.snapshot.run, self.serial)
    }
    pub fn clear(&mut self, reason: &str, reset: bool, events: &mut Vec<Value>) {
        let first = events.len();
        for cue in self.snapshot.bosses.drain(..) {
            events.push(audit(
                &cue.id,
                &cue.clip_id,
                "stop",
                cue.offset_tick,
                json!({"reason":reason}),
            ));
        }
        if let Some(cue) = self.snapshot.floor.take() {
            events.push(audit(
                &cue.id,
                &cue.clip_id,
                "stop",
                cue.offset_tick,
                json!({"reason":reason}),
            ));
        }
        if reset {
            self.serial = 0;
        }
        for event in &mut events[first..] {
            event["run"] = self.snapshot.run.into();
        }
    }
    pub fn step(
        &mut self,
        prepared: &PreparedTimeline,
        previous_phase: i32,
        previous_bosses: &[(i32, i32)],
        state: &ArenaState,
        events: &mut Vec<Value>,
    ) {
        let first = events.len();
        if state.phase == 0 || state.phase == 5 {
            self.clear("lifecycle", false, events);
            return;
        }
        self.snapshot.bosses.retain(|cue| {
            let alive = state.phase == 3
                && state.enemies.iter().any(|enemy| {
                    enemy.id == cue.entity_id && enemy.health > 0 && enemy.boss_state == 1
                });
            if !alive {
                events.push(audit(
                    &cue.id,
                    &cue.clip_id,
                    "stop",
                    cue.offset_tick,
                    json!({"reason":"boss-phase-ended"}),
                ));
            }
            alive
        });
        for cue in &mut self.snapshot.bosses {
            cue.offset_tick = cue.offset_tick.saturating_add(1);
            apply_boss(cue, &prepared.clips[0], events);
        }
        if let Some(cue) = &mut self.snapshot.floor {
            cue.offset_tick = cue.offset_tick.saturating_add(1);
            apply_floor(cue, &prepared.clips[1], events);
            if cue.next_event_index == cue.event_count {
                events.push(audit(
                    &cue.id,
                    &cue.clip_id,
                    "stop",
                    cue.offset_tick,
                    json!({"reason":"completed"}),
                ));
                self.snapshot.floor = None;
            }
        }
        if state.phase == 3 {
            let mut triggers = state
                .enemies
                .iter()
                .filter(|enemy| {
                    enemy.kind == 3
                        && enemy.health > 0
                        && enemy.boss_state == 1
                        && previous_bosses.contains(&(enemy.id, 0))
                })
                .collect::<Vec<_>>();
            triggers.sort_by_key(|enemy| enemy.id);
            for enemy in triggers {
                let mut cue = BossCue {
                    id: self.id(),
                    clip_id: IDS[0].into(),
                    entity_id: enemy.id,
                    floor: state.floor,
                    started_tick: self.snapshot.tick,
                    offset_tick: 0,
                    next_event_index: 0,
                    event_count: prepared.clips[0].events.len() as u32,
                    radius: 0.0,
                    intensity: 0.0,
                };
                events.push(audit(
                    &cue.id,
                    &cue.clip_id,
                    "start",
                    0,
                    json!({"entityId":cue.entity_id,"floor":cue.floor}),
                ));
                apply_boss(&mut cue, &prepared.clips[0], events);
                self.snapshot.bosses.push(cue);
            }
            self.snapshot.bosses.sort_by_key(|cue| cue.entity_id);
        }
        if previous_phase != 2 && state.phase == 2 {
            if let Some(cue) = self.snapshot.floor.take() {
                events.push(audit(
                    &cue.id,
                    &cue.clip_id,
                    "stop",
                    cue.offset_tick,
                    json!({"reason":"replaced"}),
                ));
            }
            let mut cue = FloorCue {
                id: self.id(),
                clip_id: IDS[1].into(),
                from_floor: state.floor,
                to_floor: state.floor + 1,
                started_tick: self.snapshot.tick,
                offset_tick: 0,
                next_event_index: 0,
                event_count: prepared.clips[1].events.len() as u32,
                opacity: 0.0,
            };
            events.push(audit(
                &cue.id,
                &cue.clip_id,
                "start",
                0,
                json!({"fromFloor":cue.from_floor,"toFloor":cue.to_floor}),
            ));
            apply_floor(&mut cue, &prepared.clips[1], events);
            if cue.next_event_index == cue.event_count {
                events.push(audit(
                    &cue.id,
                    &cue.clip_id,
                    "stop",
                    0,
                    json!({"reason":"completed"}),
                ));
            } else {
                self.snapshot.floor = Some(cue);
            }
        }
        for event in &mut events[first..] {
            event["run"] = self.snapshot.run.into();
        }
    }
}
fn audit(id: &str, clip: &str, kind: &str, offset: u64, extra: Value) -> Value {
    let mut value = json!({"cueId":id,"clipId":clip,"kind":kind,"offsetTick":offset});
    value
        .as_object_mut()
        .expect("audit object")
        .extend(extra.as_object().expect("audit fields").clone());
    value
}
fn event_audit(id: &str, clip: &str, event: &kitu_tsq1::presentation::ScheduledEvent) -> Value {
    audit(
        id,
        clip,
        "event",
        event.offset_tick,
        json!({"trackIndex":event.track_index,"eventIndex":event.event_index,"bundle":WireBundle::try_from(&event.bundle).expect("validated timeline OSC")}),
    )
}
fn apply_boss(cue: &mut BossCue, clip: &Clip, events: &mut Vec<Value>) {
    while let Some(event) = clip
        .events
        .get(cue.next_event_index as usize)
        .filter(|event| event.offset_tick <= cue.offset_tick)
    {
        for message in &event.bundle.messages {
            let [OscArg::Float(radius), OscArg::Float(intensity)] = message.args.as_slice() else {
                unreachable!("prepared boss event")
            };
            cue.radius = *radius;
            cue.intensity = *intensity;
        }
        events.push(event_audit(&cue.id, &cue.clip_id, event));
        cue.next_event_index += 1;
    }
}
fn apply_floor(cue: &mut FloorCue, clip: &Clip, events: &mut Vec<Value>) {
    while let Some(event) = clip
        .events
        .get(cue.next_event_index as usize)
        .filter(|event| event.offset_tick <= cue.offset_tick)
    {
        for message in &event.bundle.messages {
            let [OscArg::Float(opacity)] = message.args.as_slice() else {
                unreachable!("prepared floor event")
            };
            cue.opacity = *opacity;
        }
        events.push(event_audit(&cue.id, &cue.clip_id, event));
        cue.next_event_index += 1;
    }
}

/// Produces the checked reference clips for reproducible binary authoring.
/// These are render assignments only; their duration never controls gameplay.
pub fn reference_sources() -> Result<[Vec<u8>; 2]> {
    use kitu_osc_ir::OscMessage;
    use kitu_tsq1::presentation::ScheduledEvent;
    let build = |boss: bool| -> Result<Vec<u8>> {
        let offsets = if boss {
            [0, 12, 24, 36, 47]
        } else {
            [0, 12, 24, 36, 59]
        };
        let values = if boss {
            [0.45, 0.7, 1.0, 0.65, 1.0]
        } else {
            [0.0, 0.2, 0.35, 0.2, 0.0]
        };
        let events = offsets
            .into_iter()
            .zip(values)
            .enumerate()
            .map(|(index, (offset, value))| ScheduledEvent {
                offset_tick: offset,
                track_index: 0,
                event_index: index,
                bundle: OscBundle {
                    messages: vec![OscMessage {
                        address: if boss {
                            "/render/arena/cue/boss"
                        } else {
                            "/render/arena/cue/floor"
                        }
                        .into(),
                        args: if boss {
                            vec![OscArg::Float(3.0), OscArg::Float(value)]
                        } else {
                            vec![OscArg::Float(value)]
                        },
                    }],
                },
            })
            .collect();
        Clip {
            tick_rate: 60,
            track_count: 1,
            events,
        }
        .encode()
    };
    Ok([build(true)?, build(false)?])
}

#[cfg(test)]
mod tests;
