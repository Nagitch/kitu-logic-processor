//! Real TSQ1 recordings with explicit application ticks and typed OSC bundles.
//!
//! A track pairs a custom envelope with an OSC MessagePack event. Musical time
//! is a display axis (60 PPQ, one second per quarter); explicit ticks and order
//! are authoritative. Application manifests and per-input metadata stay opaque.

use anyhow::{bail, ensure, Context, Result};
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tsq1::{Event, EventKind, Sequence, TempoEntry, TimeDomain, Track, UnknownChunk};
use tsq1_osc::osc_ir::{IrBundle, IrBundleElement, IrValue};

/// Largest accepted binary recording, before any decoding or allocation.
pub const MAX_BYTES: usize = 64 * 1024 * 1024;
/// Upper bound on logical bundles in one recording.
pub const MAX_ENTRIES: usize = 1_000_000;
const FORMAT: u32 = 1;
const ENVELOPE: u8 = 0x4b;

/// One committed bundle, retaining its exact tick and position within that tick.
#[derive(Debug, Clone, PartialEq)]
pub struct TimedBundle {
    /// Zero-based application tick; never reconstructed from elapsed time.
    pub tick: u64,
    /// Zero-based contiguous order among bundles in this tick.
    pub order: u32,
    /// Application-owned identity and queue metadata.
    pub metadata: Value,
    /// Original ordered messages and typed arguments.
    pub bundle: OscBundle,
}

/// Portable binary session with application-owned version/initial-state metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct Recording {
    /// Application manifest, interpreted and validated by the application.
    pub manifest: Value,
    /// Ordered committed inputs, including intentional retransmissions.
    pub entries: Vec<TimedBundle>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Header {
    format: u32,
    manifest: Value,
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    tick: u64,
    order: u32,
    metadata: Value,
}

impl Recording {
    /// Encodes using the pinned TSQ1 public API, validating order before writing.
    ///
    /// # Examples
    /// ```
    /// use kitu_tsq1::recording::Recording;
    /// let document = Recording { manifest: serde_json::json!({"app":"example"}), entries: vec![] };
    /// let bytes = document.encode().unwrap();
    /// assert_eq!(&bytes[..4], b"TSQ1");
    /// assert_eq!(Recording::decode(&bytes).unwrap(), document);
    /// ```
    pub fn encode(&self) -> Result<Vec<u8>> {
        ensure!(self.entries.len() <= MAX_ENTRIES, "too many input bundles");
        let mut sequence = Sequence::new(60);
        sequence.tempo_map.push(TempoEntry {
            tick: 0,
            microseconds_per_quarter: 1_000_000,
        });
        sequence.unknown_chunks.push(UnknownChunk {
            id: *b"KITU",
            data: serde_json::to_vec(&Header {
                format: FORMAT,
                manifest: self.manifest.clone(),
            })?,
        });
        let mut track = Track::default();
        let mut previous = None;
        for entry in &self.entries {
            validate_order(previous, entry.tick, entry.order)?;
            let delta = entry.tick - previous.map_or(0, |(tick, _)| tick);
            track.events.push(Event {
                delta,
                domain: TimeDomain::Musical,
                kind: EventKind::Custom {
                    type_id: ENVELOPE,
                    data: serde_json::to_vec(&Envelope {
                        tick: entry.tick,
                        order: entry.order,
                        metadata: entry.metadata.clone(),
                    })?,
                },
            });
            track.events.push(tsq1_osc::event_from_ir(
                TimeDomain::Musical,
                0,
                &bundle_to_ir(&entry.bundle)?,
            )?);
            previous = Some((entry.tick, entry.order));
        }
        sequence.tracks.push(track);
        let bytes = sequence.encode()?;
        ensure!(bytes.len() <= MAX_BYTES, "recording exceeds size limit");
        Ok(bytes)
    }

    /// Decodes and validates the Kitu envelope without coercing OSC types.
    /// Unsupported chunks/tracks/time axes are rejected rather than discarded.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        ensure!(bytes.len() <= MAX_BYTES, "recording exceeds size limit");
        let sequence = Sequence::decode(bytes).context("decode TSQ1")?;
        ensure!(
            sequence.tracks.len() == 1
                && sequence.ppq == 60
                && sequence.absolute_unit == tsq1::AbsoluteUnit::Microseconds
                && sequence.flags == Sequence::new(60).flags
                && sequence.tempo_map
                    == vec![TempoEntry {
                        tick: 0,
                        microseconds_per_quarter: 1_000_000
                    }]
                && sequence.sync_anchors.is_empty()
                && sequence.markers.is_empty()
                && sequence.smpte_timing.is_none(),
            "unsupported recording time axis or tracks"
        );
        ensure!(
            sequence.unknown_chunks.len() == 1 && sequence.unknown_chunks[0].id == *b"KITU",
            "missing or unsupported recording manifest"
        );
        let header: Header = serde_json::from_slice(&sequence.unknown_chunks[0].data)?;
        ensure!(header.format == FORMAT, "incompatible recording format");
        let events = &sequence.tracks[0].events;
        ensure!(
            events.len() % 2 == 0 && events.len() / 2 <= MAX_ENTRIES,
            "unpaired or excessive recording events"
        );
        let mut previous = None;
        let mut entries = Vec::with_capacity(events.len() / 2);
        for pair in events.chunks_exact(2) {
            let EventKind::Custom {
                type_id: ENVELOPE,
                data,
            } = &pair[0].kind
            else {
                bail!("expected input envelope")
            };
            let envelope: Envelope = serde_json::from_slice(data)?;
            validate_order(previous, envelope.tick, envelope.order)?;
            let delta = envelope.tick - previous.map_or(0, |(tick, _)| tick);
            ensure!(
                pair[0].domain == TimeDomain::Musical
                    && pair[0].delta == delta
                    && pair[1].domain == TimeDomain::Musical
                    && pair[1].delta == 0,
                "tick and TSQ1 time axis disagree"
            );
            entries.push(TimedBundle {
                tick: envelope.tick,
                order: envelope.order,
                metadata: envelope.metadata,
                bundle: bundle_from_ir(&tsq1_osc::event_to_ir(&pair[1])?)?,
            });
            previous = Some((envelope.tick, envelope.order));
        }
        Ok(Self {
            manifest: header.manifest,
            entries,
        })
    }
}

fn validate_order(previous: Option<(u64, u32)>, tick: u64, order: u32) -> Result<()> {
    match previous {
        None => ensure!(order == 0, "first tick order must be zero"),
        Some((last_tick, last_order)) => {
            ensure!(tick >= last_tick, "input ticks decreased");
            let expected = if tick == last_tick {
                last_order.checked_add(1).context("input order overflow")?
            } else {
                0
            };
            ensure!(order == expected, "input order is not contiguous");
        }
    }
    Ok(())
}

/// Converts Kitu's immediate, flat bundle into OSC-IR without flattening messages.
/// Each message is `[address, type-tags, arguments]`; tags distinguish i32/i64.
/// Non-finite floats cannot enter the Runtime and are rejected at this boundary.
pub fn bundle_to_ir(bundle: &OscBundle) -> Result<IrValue> {
    let mut result = IrBundle::immediate();
    for message in &bundle.messages {
        let mut tags = String::new();
        let mut args = Vec::new();
        for arg in &message.args {
            let (tag, value) = match arg {
                OscArg::Int(v) => ('i', IrValue::Integer(i64::from(*v))),
                OscArg::Int64(v) => ('h', IrValue::Integer(*v)),
                OscArg::Float(v) => {
                    ensure!(v.is_finite(), "non-finite OSC float");
                    ('f', IrValue::Float(f64::from(*v)))
                }
                OscArg::Str(v) => ('s', IrValue::from(v.clone())),
                OscArg::Bool(v) => ('b', IrValue::Bool(*v)),
            };
            tags.push(tag);
            args.push(value);
        }
        result.add_message(IrValue::Array(vec![
            IrValue::from(message.address.clone()),
            IrValue::from(tags),
            IrValue::Array(args),
        ]));
    }
    Ok(IrValue::Bundle(result))
}

/// Restores exact Kitu widths and message order. Nested or scheduled bundles and
/// unsupported argument types are errors; they are never silently flattened.
pub fn bundle_from_ir(value: &IrValue) -> Result<OscBundle> {
    let IrValue::Bundle(bundle) = value else {
        bail!("expected OSC bundle")
    };
    ensure!(
        bundle.is_immediate(),
        "scheduled bundle cannot be a Runtime input"
    );
    let mut result = OscBundle::new();
    for element in &bundle.elements {
        let IrBundleElement::Message(IrValue::Array(fields)) = element else {
            bail!("expected flat OSC message")
        };
        let [IrValue::String(address), IrValue::String(tags), IrValue::Array(args)] =
            fields.as_slice()
        else {
            bail!("invalid typed OSC message")
        };
        ensure!(tags.len() == args.len(), "OSC type/argument count mismatch");
        let mut message = OscMessage::new(address.to_string());
        for (tag, value) in tags.bytes().zip(args) {
            message.args.push(match (tag, value) {
                (b'i', IrValue::Integer(v)) => {
                    OscArg::Int(i32::try_from(*v).context("i32 overflow")?)
                }
                (b'h', IrValue::Integer(v)) => OscArg::Int64(*v),
                (b'f', IrValue::Float(v)) => {
                    ensure!(
                        v.is_finite() && f64::from(*v as f32).to_bits() == v.to_bits(),
                        "OSC float is not lossless f32"
                    );
                    OscArg::Float(*v as f32)
                }
                (b's', IrValue::String(v)) => OscArg::Str(v.to_string()),
                (b'b', IrValue::Bool(v)) => OscArg::Bool(*v),
                _ => bail!("OSC type/value mismatch"),
            });
        }
        result.push(message);
    }
    Ok(result)
}

/// Canonical MessagePack bytes for hashing ordered logical output bundles.
pub fn bundle_bytes(bundle: &OscBundle) -> Result<Vec<u8>> {
    let event = tsq1_osc::event_from_ir(TimeDomain::Musical, 0, &bundle_to_ir(bundle)?)?;
    let EventKind::Osc(osc) = event.kind else {
        unreachable!()
    };
    Ok(osc.data)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn binary_roundtrip_preserves_width_order_bundles_and_large_ticks() {
        let bundle = OscBundle {
            messages: vec![
                OscMessage {
                    address: "/input/test".into(),
                    args: vec![
                        OscArg::Int(7),
                        OscArg::Int64(7),
                        OscArg::Int64(i64::MAX),
                        OscArg::Int64(i64::MIN),
                        OscArg::Float(-0.0),
                        OscArg::Float(0.1),
                        OscArg::Str("日本語\n\0".into()),
                        OscArg::Bool(false),
                        OscArg::Bool(true),
                    ],
                },
                OscMessage::new("/second"),
            ],
        };
        let tick = 9_007_199_254_740_993;
        let doc = Recording {
            manifest: serde_json::json!({"contract":1}),
            entries: vec![
                TimedBundle {
                    tick,
                    order: 0,
                    metadata: serde_json::json!({"id":u64::MAX}),
                    bundle: bundle.clone(),
                },
                TimedBundle {
                    tick,
                    order: 1,
                    metadata: Value::Null,
                    bundle: OscBundle::new(),
                },
            ],
        };
        let bytes = doc.encode().unwrap();
        assert_eq!(&bytes[..4], b"TSQ1");
        let restored = Recording::decode(&bytes).unwrap();
        assert_eq!(restored, doc);
        assert_eq!(
            bundle_bytes(&restored.entries[0].bundle).unwrap(),
            bundle_bytes(&bundle).unwrap()
        );
        let OscArg::Float(zero) = restored.entries[0].bundle.messages[0].args[4] else {
            panic!()
        };
        assert_eq!(zero.to_bits(), (-0.0f32).to_bits());
    }
    #[test]
    fn rejects_reordered_inputs_lossy_types_and_unsupported_bundles() {
        let mut doc = Recording {
            manifest: Value::Null,
            entries: vec![TimedBundle {
                tick: 3,
                order: 1,
                metadata: Value::Null,
                bundle: OscBundle::new(),
            }],
        };
        assert!(doc.encode().is_err());
        doc.entries[0].order = 0;
        let mut sequence = Sequence::decode(&doc.encode().unwrap()).unwrap();
        sequence.tracks[0].events[0].delta = 4;
        assert!(Recording::decode(&sequence.encode().unwrap()).is_err());
        let mut bundle = IrBundle::immediate();
        bundle.add_bundle(IrBundle::immediate());
        assert!(bundle_from_ir(&IrValue::Bundle(bundle)).is_err());
        assert!(Recording::decode(b"emit:fake").is_err());
        let bundle = OscBundle {
            messages: vec![OscMessage {
                address: "/bad".into(),
                args: vec![OscArg::Float(f32::NAN)],
            }],
        };
        assert!(bundle_to_ir(&bundle).is_err());
    }
}
