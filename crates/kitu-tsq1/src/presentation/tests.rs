use super::*;
use kitu_osc_ir::OscMessage;
use tsq1::{Event, Marker, OscEvent, SmpteFps, SmpteTiming, SyncAnchor, UnknownChunk};
use tsq1_osc::osc_ir::{IrBundle, IrTimetag, IrValue};

fn event(offset_tick: u64, track_index: usize, event_index: usize) -> ScheduledEvent {
    ScheduledEvent {
        offset_tick,
        track_index,
        event_index,
        bundle: OscBundle::new(),
    }
}

fn clip() -> Clip {
    Clip {
        tick_rate: 60,
        track_count: 1,
        events: vec![event(0, 0, 0)],
    }
}

fn typed_bundle(args: Vec<OscArg>) -> OscBundle {
    OscBundle {
        messages: vec![OscMessage {
            address: "/render/test".into(),
            args,
        }],
    }
}

fn sequence() -> Sequence {
    Sequence::decode(&clip().encode().unwrap()).unwrap()
}

fn reject_sequence(sequence: &Sequence) {
    assert!(Clip::decode(&sequence.encode().unwrap()).is_err());
}

#[test]
fn actual_tsq1_preserves_tracks_ties_bundles_and_every_scalar_width() {
    let bundle = typed_bundle(vec![
        OscArg::Int(i32::MIN),
        OscArg::Int(i32::MAX),
        OscArg::Int64(i64::MIN),
        OscArg::Int64(i64::MAX),
        OscArg::Float(0.1),
        OscArg::Float(-0.0),
        OscArg::Str("日本語\n".into()),
        OscArg::Bool(false),
        OscArg::Bool(true),
    ]);
    let mut original = Clip {
        tick_rate: 60,
        track_count: 5,
        events: vec![
            event(0, 1, 0),
            event(12, 0, 0),
            event(12, 0, 1),
            event(12, 1, 1),
            event(60, 3, 0),
        ],
    };
    original.events[2].bundle = bundle;
    original.events[2]
        .bundle
        .messages
        .push(OscMessage::new("/second"));
    let bytes = original.encode().unwrap();
    assert_eq!(&bytes[..4], b"TSQ1");
    let model = Sequence::decode(&bytes).unwrap();
    assert_eq!(model.ppq, 60);
    assert_eq!(model.tracks.len(), 5);
    assert!(model.tracks[2].events.is_empty());
    assert!(model.tracks[4].events.is_empty());
    assert_eq!(model.tracks[0].events[0].delta, 12);
    assert_eq!(model.tracks[0].events[1].delta, 0);
    assert_eq!(model.tracks[1].events[1].delta, 12);
    let restored = Clip::decode(&bytes).unwrap();
    assert_eq!(restored, original);
    assert_eq!(restored.encode().unwrap(), bytes);
    let OscArg::Float(zero) = restored.events[2].bundle.messages[0].args[5] else {
        panic!("exact float type")
    };
    assert_eq!(zero.to_bits(), (-0.0_f32).to_bits());
}

#[test]
fn empty_tracks_generic_rates_and_inclusive_last_offset_are_valid() {
    for tick_rate in [1, 60, u16::MAX] {
        let empty = Clip {
            tick_rate,
            track_count: MAX_TRACKS,
            events: vec![],
        };
        assert_eq!(Clip::decode(&empty.encode().unwrap()).unwrap(), empty);
    }
    let mut last = clip();
    last.events[0].offset_tick = MAX_OFFSET_TICK;
    assert_eq!(
        Clip::decode(&last.encode().unwrap()).unwrap().events[0].offset_tick,
        MAX_OFFSET_TICK
    );
    last.events[0].offset_tick += 1;
    assert!(last.encode().is_err());
}

#[test]
fn rejects_bad_order_indices_and_structural_limits_before_encoding() {
    for track_count in [0, MAX_TRACKS + 1, usize::MAX] {
        assert!(Clip {
            track_count,
            ..clip()
        }
        .encode()
        .is_err());
    }
    assert!(Clip {
        tick_rate: 0,
        ..clip()
    }
    .encode()
    .is_err());
    assert!(Clip {
        events: vec![event(0, 1, 0)],
        ..clip()
    }
    .encode()
    .is_err());
    assert!(Clip {
        events: vec![event(0, 0, 1)],
        ..clip()
    }
    .encode()
    .is_err());
    assert!(Clip {
        events: vec![event(1, 0, 0), event(0, 0, 1)],
        ..clip()
    }
    .encode()
    .is_err());
    assert!(Clip {
        events: vec![event(0, 0, 0), event(0, 0, 0)],
        ..clip()
    }
    .encode()
    .is_err());
    assert!(Clip {
        track_count: 2,
        events: vec![event(0, 1, 0), event(0, 0, 0)],
        ..clip()
    }
    .encode()
    .is_err());
    assert!(Clip {
        events: (0..=MAX_EVENTS).map(|index| event(0, 0, index)).collect(),
        ..clip()
    }
    .encode()
    .is_err());
    assert!(Clip::decode(&vec![0; MAX_BYTES + 1]).is_err());
    assert!(Clip::decode(b"emit:not-binary").is_err());
}

#[test]
fn decoded_event_counts_and_accumulated_offsets_are_checked() {
    let mut many = sequence();
    many.tracks[0].events = vec![many.tracks[0].events[0].clone(); MAX_EVENTS + 1];
    reject_sequence(&many);
    let mut overflowing = sequence();
    let mut second = overflowing.tracks[0].events[0].clone();
    overflowing.tracks[0].events[0].delta = 1;
    second.delta = u64::MAX;
    overflowing.tracks[0].events.push(second);
    let error = Clip::decode(&overflowing.encode().unwrap()).unwrap_err();
    assert!(format!("{error:#}").contains("tick offset overflow"));
    assert!(error.to_string().contains("track 0 event 1"));
}

#[test]
fn unsupported_time_axes_flags_chunks_and_event_kinds_are_not_dropped() {
    let mutations: &[fn(&mut Sequence)] = &[
        |s| s.tempo_map.clear(),
        |s| s.tempo_map[0].microseconds_per_quarter = 500_000,
        |s| {
            s.tempo_map.push(TempoEntry {
                tick: 1,
                microseconds_per_quarter: 1_000_000,
            })
        },
        |s| s.absolute_unit = AbsoluteUnit::Nanoseconds,
        |s| s.flags |= 0x8000,
        |s| s.sync_anchors.push(SyncAnchor { tick: 0, time: 0 }),
        |s| {
            s.markers.push(Marker {
                domain: TimeDomain::Musical,
                position: 0,
                name: "cue".into(),
                class: 0x20,
                color_rgba: None,
            })
        },
        |s| {
            s.smpte_timing = Some(SmpteTiming {
                fps: SmpteFps::Fps30,
                subframes: 80,
            })
        },
        |s| {
            s.unknown_chunks.push(UnknownChunk {
                id: *b"TEST",
                data: vec![],
            })
        },
        |s| s.tracks[0].events[0].domain = TimeDomain::Absolute,
        |s| s.tracks[0].events[0].kind = EventKind::Midi([0x90, 60, 100]),
        |s| {
            s.tracks[0].events[0].kind = EventKind::Custom {
                type_id: 1,
                data: vec![],
            }
        },
        |s| {
            s.tracks[0].events[0].kind = EventKind::Osc(OscEvent {
                format: OscFormat::Raw,
                data: b"/raw".to_vec(),
            })
        },
        |s| {
            s.tracks[0].events[0].kind = EventKind::Osc(OscEvent {
                format: OscFormat::Cbor,
                data: vec![],
            })
        },
    ];
    for mutate in mutations {
        let mut model = sequence();
        mutate(&mut model);
        reject_sequence(&model);
    }
}

#[test]
fn osc_scalar_and_container_limits_apply_on_both_sides() {
    let valid_args = vec![OscArg::Str("s".repeat(MAX_STRING_BYTES))];
    let mut original = clip();
    original.events[0].bundle = typed_bundle(valid_args);
    original.events[0].bundle.messages[0].address =
        format!("/{}", "a".repeat(MAX_ADDRESS_BYTES - 1));
    assert_eq!(Clip::decode(&original.encode().unwrap()).unwrap(), original);
    let invalid = [
        typed_bundle(vec![OscArg::Str("s".repeat(MAX_STRING_BYTES + 1))]),
        typed_bundle(vec![OscArg::Str("a\0b".into())]),
        typed_bundle(vec![OscArg::Float(f32::INFINITY)]),
        typed_bundle(vec![OscArg::Float(f32::NAN)]),
        typed_bundle(vec![OscArg::Bool(true); MAX_ARGS + 1]),
        OscBundle {
            messages: vec![OscMessage::new("/a"); MAX_MESSAGES + 1],
        },
        OscBundle {
            messages: vec![OscMessage::new("relative")],
        },
        OscBundle {
            messages: vec![OscMessage::new("/a\0b")],
        },
        OscBundle {
            messages: vec![OscMessage::new(format!(
                "/{}",
                "a".repeat(MAX_ADDRESS_BYTES)
            ))],
        },
    ];
    for bundle in invalid {
        original.events[0].bundle = bundle.clone();
        assert!(original.encode().is_err());
        if let Ok(ir) = bundle_to_ir(&bundle) {
            let mut model = sequence();
            model.tracks[0].events[0] =
                tsq1_osc::event_from_ir(TimeDomain::Musical, 0, &ir).unwrap();
            reject_sequence(&model);
        }
    }
}

#[test]
fn total_encoded_bytes_are_bounded_even_when_individual_scalars_fit() {
    let mut original = clip();
    original.events[0].bundle = typed_bundle(vec![OscArg::Str("s".repeat(MAX_STRING_BYTES)); 8]);
    assert!(original.encode().is_err());
    // Small logical scalars still incur real OSC/MessagePack/TSQ1 framing.
    original.events = (0..MAX_EVENTS)
        .map(|index| {
            let mut item = event(0, 0, index);
            item.bundle = typed_bundle(vec![OscArg::Bool(false)]);
            item
        })
        .collect();
    assert!(original
        .encode()
        .unwrap_err()
        .to_string()
        .contains("byte limit"));
}

#[test]
fn nested_scheduled_and_lossy_ir_are_rejected() {
    let mut nested = IrBundle::immediate();
    nested.add_bundle(IrBundle::immediate());
    let mut lossy = IrBundle::immediate();
    lossy.add_message(IrValue::Array(vec![
        IrValue::from("/float"),
        IrValue::from("f"),
        IrValue::Array(vec![IrValue::Float(0.1_f64)]),
    ]));
    for ir in [
        IrValue::Bundle(nested),
        IrValue::Bundle(IrBundle::new(IrTimetag::from_ntp(42))),
        IrValue::Bundle(lossy),
        IrValue::Array(vec![]),
    ] {
        let mut model = sequence();
        model.tracks[0].events[0] = tsq1_osc::event_from_ir(TimeDomain::Musical, 0, &ir).unwrap();
        reject_sequence(&model);
    }
}

#[test]
fn messagepack_preflight_rejects_depth_length_truncation_and_trailing_data() {
    let mut too_deep = vec![0x91; MAX_MESSAGEPACK_DEPTH + 1];
    too_deep.push(0xc0);
    for payload in [
        too_deep,
        vec![0xdd, 0xff, 0xff, 0xff, 0xff], // array32, impossible element count
        vec![0xdf, 0xff, 0xff, 0xff, 0xff], // map32, impossible element count
        vec![0xdb, 0xff, 0xff, 0xff, 0xff], // str32, missing declared contents
        vec![0xc6, 0xff, 0xff, 0xff, 0xff], // bin32, missing declared contents
        vec![0x91],
        vec![0xc0, 0xc0],
    ] {
        assert!(preflight_messagepack(&payload).is_err());
        let mut model = sequence();
        model.tracks[0].events[0] = Event {
            delta: 0,
            domain: TimeDomain::Musical,
            kind: EventKind::Osc(OscEvent {
                format: OscFormat::MessagePack,
                data: payload,
            }),
        };
        reject_sequence(&model);
    }
    let mut model = sequence();
    let EventKind::Osc(osc) = &mut model.tracks[0].events[0].kind else {
        unreachable!()
    };
    osc.data.push(0xc0);
    let error = Clip::decode(&model.encode().unwrap()).unwrap_err();
    assert!(format!("{error:#}").contains("trailing presentation MessagePack bytes"));
}
