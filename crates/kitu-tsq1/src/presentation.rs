//! Bounded TSQ1 presentation clips with exact, clock-free integer scheduling.
//!
//! The application owns playback cursors and effects. This module decodes real
//! TSQ1 tracks, retains typed OSC bundles and defines their stable merged order.
//! Authoring/decoding belongs outside a simulation tick or its state lock.

use std::io::Cursor;

use anyhow::{ensure, Context, Result};
use kitu_osc_ir::{OscArg, OscBundle};
use serde::{de::IgnoredAny, Deserialize};
use tsq1::{AbsoluteUnit, EventKind, OscFormat, Sequence, TempoEntry, TimeDomain, Track};

use crate::recording::{bundle_from_ir, bundle_to_ir};

/// Maximum source or encoded clip size, checked before binary decoding.
pub const MAX_BYTES: usize = 8 * 1024;
/// Maximum tracks, including empty tracks.
pub const MAX_TRACKS: usize = 8;
/// Maximum events across all tracks.
pub const MAX_EVENTS: usize = 256;
/// Largest inclusive event offset, measured directly in application ticks.
pub const MAX_OFFSET_TICK: u64 = 3600;
/// Maximum messages inside one immediate event bundle.
pub const MAX_MESSAGES: usize = 32;
/// Maximum scalar arguments inside one message.
pub const MAX_ARGS: usize = 16;
/// Maximum UTF-8 bytes in an OSC address.
pub const MAX_ADDRESS_BYTES: usize = 256;
/// Maximum UTF-8 bytes in one string argument.
pub const MAX_STRING_BYTES: usize = 1024;
/// Maximum structural MessagePack nesting before allocating an OSC-IR tree.
pub const MAX_MESSAGEPACK_DEPTH: usize = 32;

/// One event positioned on an exact tick, preserving its original track order.
#[derive(Debug, Clone, PartialEq)]
pub struct ScheduledEvent {
    /// Zero-based application tick offset, with no elapsed-time conversion.
    pub offset_tick: u64,
    /// Zero-based original track index, including empty preceding tracks.
    pub track_index: usize,
    /// Zero-based contiguous index within the original track.
    pub event_index: usize,
    /// Original immediate, flat OSC bundle with ordered messages/arguments.
    pub bundle: OscBundle,
}

/// An application-independent presentation clip, without a clock or mutable cursor.
///
/// Events are ordered by `(offset_tick, track_index, event_index)`. Each track's
/// event indices must be contiguous from zero. Empty tracks/events are valid;
/// applications may require initialization or terminal events in their contract.
#[derive(Debug, Clone, PartialEq)]
pub struct Clip {
    /// Nonzero ticks per second, represented by equal TSQ1 PPQ and a one-second quarter.
    pub tick_rate: u16,
    /// Number of original tracks, in `1..=MAX_TRACKS`.
    pub track_count: usize,
    /// Events in deterministic merged order; bundle boundaries are never flattened.
    pub events: Vec<ScheduledEvent>,
}

impl Clip {
    /// Decodes a bounded real TSQ1 clip using the pinned public TSQ1/OSC APIs.
    ///
    /// Rejects unsupported time axes, event kinds, OSC types and unknown chunks.
    /// Per-track musical deltas are checked and summed as integers. The source's
    /// exact bytes remain the application's responsibility when hashing versions.
    ///
    /// # Examples
    /// ```
    /// use kitu_tsq1::presentation::{Clip, ScheduledEvent};
    /// let clip = Clip {
    ///     tick_rate: 60, track_count: 1,
    ///     events: vec![ScheduledEvent {
    ///         offset_tick: 12, track_index: 0, event_index: 0,
    ///         bundle: kitu_osc_ir::OscBundle::new(),
    ///     }],
    /// };
    /// let bytes = clip.encode()?;
    /// assert_eq!(&bytes[..4], b"TSQ1");
    /// assert_eq!(Clip::decode(&bytes)?, clip);
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(
            bytes.len() <= MAX_BYTES,
            "presentation clip exceeds byte limit"
        );
        let sequence = Sequence::decode(bytes).context("decode presentation TSQ1")?;
        ensure!(
            (1..=MAX_TRACKS).contains(&sequence.tracks.len()),
            "presentation track count must be 1..={MAX_TRACKS}"
        );
        ensure!(
            sequence.absolute_unit == AbsoluteUnit::Microseconds
                && sequence.flags == Sequence::new(sequence.ppq).flags
                && sequence.tempo_map == tempo_map()
                && sequence.sync_anchors.is_empty()
                && sequence.markers.is_empty()
                && sequence.smpte_timing.is_none()
                && sequence.unknown_chunks.is_empty(),
            "unsupported presentation timing, flags or chunks"
        );
        ensure!(
            sequence
                .tracks
                .iter()
                .map(|track| track.events.len())
                .sum::<usize>()
                <= MAX_EVENTS,
            "presentation clip exceeds event limit"
        );
        let mut events = Vec::new();
        for (track_index, track) in sequence.tracks.iter().enumerate() {
            let mut offset_tick = 0_u64;
            for (event_index, event) in track.events.iter().enumerate() {
                let scheduled = (|| -> Result<ScheduledEvent> {
                    ensure!(
                        event.domain == TimeDomain::Musical,
                        "expected musical tick domain"
                    );
                    offset_tick = offset_tick
                        .checked_add(event.delta)
                        .context("tick offset overflow")?;
                    ensure!(
                        offset_tick <= MAX_OFFSET_TICK,
                        "presentation tick offset exceeds limit"
                    );
                    let EventKind::Osc(osc) = &event.kind else {
                        anyhow::bail!("presentation event must be OSC")
                    };
                    ensure!(
                        osc.format == OscFormat::MessagePack,
                        "presentation OSC must use MessagePack IR"
                    );
                    preflight_messagepack(&osc.data)?;
                    let bundle = bundle_from_ir(&tsq1_osc::event_to_ir(event)?)?;
                    Ok(ScheduledEvent {
                        offset_tick,
                        track_index,
                        event_index,
                        bundle,
                    })
                })()
                .with_context(|| format!("presentation track {track_index} event {event_index}"))?;
                events.push(scheduled);
            }
        }
        events.sort_by_key(event_key);
        let clip = Self {
            tick_rate: sequence.ppq,
            track_count: sequence.tracks.len(),
            events,
        };
        // The canonical representation must fit the same bound as authoring input.
        clip.validate()?;
        Ok(clip)
    }

    /// Validates a manually built clip and encodes canonical TSQ1 bytes.
    ///
    /// Does not reorder malformed input or silently drop unsupported data.
    /// The output uses one-second quarters and preserves empty tracks, same-tick
    /// order, OSC widths and bundle boundaries. Original framing may normalize.
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate_structure()?;
        let mut sequence = Sequence::new(self.tick_rate);
        sequence.tempo_map = tempo_map();
        sequence.tracks = vec![Track::default(); self.track_count];
        let mut previous = [0_u64; MAX_TRACKS];
        // Header, track framing and one TMAP. Event size additions below are a
        // lower bound; the actual canonical bytes receive a final exact check.
        let mut payload_bytes = 14 + self.track_count * 8 + 20;
        for event in &self.events {
            let encoded = tsq1_osc::event_from_ir(
                TimeDomain::Musical,
                event.offset_tick - previous[event.track_index],
                &bundle_to_ir(&event.bundle)?,
            )?;
            let EventKind::Osc(osc) = &encoded.kind else {
                unreachable!()
            };
            payload_bytes += osc.data.len() + 4;
            ensure!(
                payload_bytes <= MAX_BYTES,
                "presentation clip exceeds byte limit"
            );
            previous[event.track_index] = event.offset_tick;
            sequence.tracks[event.track_index].events.push(encoded);
        }
        let bytes = sequence.encode()?;
        ensure!(
            bytes.len() <= MAX_BYTES,
            "presentation clip exceeds byte limit"
        );
        Ok(bytes)
    }

    /// Checks ordering, timing, OSC scalar limits and exact canonical encoded size.
    ///
    /// This performs bounded encoding and belongs with off-clock preparation;
    /// immutable decoded clips need no validation on each application tick.
    pub fn validate(&self) -> Result<()> {
        self.encode().map(|_| ())
    }

    fn validate_structure(&self) -> Result<()> {
        ensure!(self.tick_rate > 0, "presentation tick rate must be nonzero");
        ensure!(
            (1..=MAX_TRACKS).contains(&self.track_count),
            "presentation track count must be 1..={MAX_TRACKS}"
        );
        ensure!(
            self.events.len() <= MAX_EVENTS,
            "presentation clip exceeds event limit"
        );
        let mut next_indices = [0; MAX_TRACKS];
        let mut previous = None;
        let mut scalar_bytes = 0_usize;
        for event in &self.events {
            let key = event_key(event);
            ensure!(
                event.track_index < self.track_count,
                "presentation track index is out of range"
            );
            ensure!(
                event.offset_tick <= MAX_OFFSET_TICK,
                "presentation tick offset exceeds limit"
            );
            ensure!(
                previous.is_none_or(|last| key > last),
                "presentation events are not in canonical order"
            );
            ensure!(
                event.event_index == next_indices[event.track_index],
                "presentation event indices must be contiguous within each track"
            );
            next_indices[event.track_index] += 1;
            previous = Some(key);
            validate_bundle(&event.bundle, &mut scalar_bytes).with_context(|| {
                format!(
                    "presentation track {} event {}",
                    event.track_index, event.event_index
                )
            })?;
        }
        Ok(())
    }
}

fn event_key(event: &ScheduledEvent) -> (u64, usize, usize) {
    (event.offset_tick, event.track_index, event.event_index)
}

fn tempo_map() -> Vec<TempoEntry> {
    vec![TempoEntry {
        tick: 0,
        microseconds_per_quarter: 1_000_000,
    }]
}

fn validate_bundle(bundle: &OscBundle, scalar_bytes: &mut usize) -> Result<()> {
    ensure!(
        bundle.messages.len() <= MAX_MESSAGES,
        "presentation bundle exceeds message limit"
    );
    for message in &bundle.messages {
        ensure!(
            message.address.starts_with('/')
                && message.address.len() <= MAX_ADDRESS_BYTES
                && !message.address.contains('\0'),
            "invalid presentation OSC address"
        );
        ensure!(
            message.args.len() <= MAX_ARGS,
            "presentation message exceeds argument limit"
        );
        *scalar_bytes += message.address.len();
        for arg in &message.args {
            *scalar_bytes += match arg {
                OscArg::Str(value) => {
                    ensure!(
                        value.len() <= MAX_STRING_BYTES && !value.contains('\0'),
                        "invalid presentation OSC string"
                    );
                    value.len() + 1
                }
                OscArg::Float(value) => {
                    ensure!(value.is_finite(), "non-finite presentation OSC float");
                    1
                }
                _ => 1,
            };
            ensure!(
                *scalar_bytes <= MAX_BYTES,
                "presentation clip exceeds scalar byte limit"
            );
        }
        ensure!(
            *scalar_bytes <= MAX_BYTES,
            "presentation clip exceeds scalar byte limit"
        );
    }
    Ok(())
}

fn preflight_messagepack(payload: &[u8]) -> Result<()> {
    // Ignore values rather than allocate an IR tree from untrusted lengths or
    // nesting. Cursor bounds the reader's copies to the already bounded payload.
    let mut decoder = rmp_serde::Deserializer::new(Cursor::new(payload));
    decoder.set_max_depth(MAX_MESSAGEPACK_DEPTH);
    IgnoredAny::deserialize(&mut decoder)
        .context("invalid or excessive presentation MessagePack")?;
    ensure!(
        decoder.position() as usize == payload.len(),
        "trailing presentation MessagePack bytes"
    );
    Ok(())
}

#[cfg(test)]
mod tests;
