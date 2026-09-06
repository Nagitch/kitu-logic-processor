use super::*;
use crate::wire::{WireArg, WireMessage};
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use serde_json::json;

fn codec(encoding: Encoding) -> Codec {
    Codec::new(encoding, NETWORK_OUTPUT_LIMITS).unwrap()
}
fn compatibility() -> Compatibility {
    Compatibility {
        app_id: "endless-arena".into(),
        wire_version: 1,
        schema_version: 1,
        presentation_version: 1,
        tick_rate: 60,
        features: vec![
            "output-batches".into(),
            "presentation".into(),
            "replay".into(),
            "typed-osc".into(),
        ],
    }
}
fn status(tick: i64) -> ExecutionStatus {
    ExecutionStatus {
        playback_mode: ReplayMode {
            active: false,
            recording_id: None,
            tick,
            total_ticks: u64::MAX,
            playing: false,
            seeking: false,
            error: None,
        },
        read_only: false,
    }
}
fn input() -> InputFrame {
    InputFrame {
        metadata: Some(InputMetadata {
            source: "producer".into(),
            message_id: u64::MAX,
            schema_version: 1,
        }),
        bundle: WireBundle {
            messages: vec![
                WireMessage {
                    address: "/example/widths".into(),
                    args: vec![
                        WireArg::Int(i32::MIN),
                        WireArg::Int(i32::MAX),
                        WireArg::Int(1),
                        WireArg::Int64(i64::MIN),
                        WireArg::Int64(i64::MAX),
                        WireArg::Int64(1),
                        WireArg::Int64(9_007_199_254_740_993),
                        WireArg::Float(-0.0),
                        WireArg::Float(f32::from_bits(1)),
                        WireArg::Float(f32::MAX),
                        WireArg::Float(-f32::MAX),
                        WireArg::Float(1.25),
                        WireArg::Str("OSC 日本語 🌌".into()),
                        WireArg::Bool(false),
                        WireArg::Bool(true),
                    ],
                },
                WireMessage {
                    address: "/example/empty".into(),
                    args: vec![],
                },
            ],
        },
    }
}
fn hello() -> ServerHello {
    ServerHello {
        compatibility: compatibility(),
        execution: ExecutionVersion {
            package: "0.1.0".into(),
            source_hash: "0123456789abcdef".repeat(4),
            target: "aarch64-apple-darwin".into(),
        },
        session_id: "session-1".into(),
        role: Role::Controller,
        limits: WireLimits {
            max_input_bytes: 128 * 1024,
            max_output_bytes: 8 * 1024 * 1024,
        },
        status: status(-1),
    }
}
fn output() -> ServerFrame {
    ServerFrame {
        delivery_sequence: u64::MAX,
        frame: ServerPayload::Output(OutputFrame {
            batch: OutputBatch {
                tick: -1,
                bundles: vec![
                    input().bundle,
                    WireBundle::default(),
                    WireBundle {
                        messages: vec![WireMessage {
                            address: "/example/last".into(),
                            args: vec![],
                        }],
                    },
                ],
            },
            status: status(-1),
        }),
    }
}
fn float_bits(input: &InputFrame) -> Vec<u32> {
    input
        .bundle
        .messages
        .iter()
        .flat_map(|m| &m.args)
        .filter_map(|arg| match arg {
            WireArg::Float(value) => Some(value.to_bits()),
            _ => None,
        })
        .collect()
}
#[test]
fn encodings_preserve_exact_widths_bits_ids_and_bundle_boundaries() {
    for encoding in [Encoding::Json, Encoding::MessagePack] {
        let codec = codec(encoding);
        let value = input();
        let decoded = codec
            .decode_input(&codec.encode_input(&value).unwrap())
            .unwrap();
        assert_eq!(value, decoded);
        assert_eq!(float_bits(&value), float_bits(&decoded));
        let frame = ClientFrame::Input(value);
        assert_eq!(
            codec
                .decode_client(&codec.encode_client(&frame).unwrap())
                .unwrap(),
            frame
        );
        let frame = output();
        assert_eq!(
            codec
                .decode_server(&codec.encode_server(&frame).unwrap())
                .unwrap(),
            frame
        );
        let ServerPayload::Output(out) = frame.frame else {
            panic!()
        };
        let restored = out
            .batch
            .bundles
            .into_iter()
            .map(OscBundle::try_from)
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(restored.len(), 3);
        assert!(restored[1].messages.is_empty());
        assert!(matches!(restored[0].messages[0].args[5], OscArg::Int64(1)));
    }
}
#[test]
fn borrowed_frames_have_exact_owned_bytes_and_stop_before_later_invalid_values() {
    let frame = output();
    let ServerPayload::Output(out) = &frame.frame else {
        panic!()
    };
    let bundles = out
        .batch
        .bundles
        .clone()
        .into_iter()
        .map(OscBundle::try_from)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    let borrowed = ServerFrameRef {
        delivery_sequence: frame.delivery_sequence,
        frame: ServerPayloadRef::Output(OutputFrameRef {
            batch: OutputBatchRef {
                tick: -1,
                bundles: &bundles,
            },
            status: &out.status,
        }),
    };
    for encoding in [Encoding::Json, Encoding::MessagePack] {
        assert_eq!(
            codec(encoding).encode_server(&frame).unwrap(),
            codec(encoding).encode_server_ref(&borrowed).unwrap()
        );
    }
    let big = vec![
        OscBundle {
            messages: vec![OscMessage {
                address: "/large".into(),
                args: vec![OscArg::Str("x".repeat(8192))],
            }],
        },
        OscBundle {
            messages: vec![OscMessage {
                address: "invalid".into(),
                args: vec![],
            }],
        },
    ];
    let borrowed = ServerFrameRef {
        delivery_sequence: 0,
        frame: ServerPayloadRef::Output(OutputFrameRef {
            batch: OutputBatchRef {
                tick: -1,
                bundles: &big,
            },
            status: &out.status,
        }),
    };
    for encoding in [Encoding::Json, Encoding::MessagePack] {
        let codec = Codec::new(
            encoding,
            Limits {
                max_bytes: 1024,
                ..NETWORK_OUTPUT_LIMITS
            },
        )
        .unwrap();
        assert_eq!(
            codec.encode_server_ref(&borrowed).unwrap_err().kind,
            CodecErrorKind::Limit
        );
    }
}
#[test]
fn exact_encoded_limit_and_escaped_strings_use_bounded_capacity() {
    let frame = ClientFrame::Input(InputFrame {
        metadata: None,
        bundle: WireBundle {
            messages: vec![WireMessage {
                address: "/escaped".into(),
                args: vec![WireArg::Str("\"\\\n".repeat(1000))],
            }],
        },
    });
    for encoding in [Encoding::Json, Encoding::MessagePack] {
        let bytes = codec(encoding).encode_client(&frame).unwrap();
        let exact = Codec::new(
            encoding,
            Limits {
                max_bytes: bytes.len(),
                ..NETWORK_OUTPUT_LIMITS
            },
        )
        .unwrap();
        assert_eq!(exact.encode_client(&frame).unwrap(), bytes);
        let short = Codec::new(
            encoding,
            Limits {
                max_bytes: bytes.len() - 1,
                ..NETWORK_OUTPUT_LIMITS
            },
        )
        .unwrap();
        assert_eq!(
            short.encode_client(&frame).unwrap_err().kind,
            CodecErrorKind::Limit
        );
    }
    let mut writer = LimitedWriter {
        bytes: vec![],
        maximum: 3000,
        exceeded: false,
    };
    for _ in 0..3000 {
        writer.write_all(b"x").unwrap();
        assert!(writer.bytes.capacity() <= 3000);
    }
    assert!(writer.write_all(b"x").is_err());
    assert_eq!(writer.bytes.len(), 3000);
}
#[test]
fn limits_apply_before_declared_messagepack_collections_can_allocate() {
    let codec = codec(Encoding::MessagePack);
    for bytes in [
        vec![0xdf, 255, 255, 255, 255],
        vec![
            0x81, 0xa8, b'm', b'e', b's', b's', b'a', b'g', b'e', b's', 0xdd, 255, 255, 255, 255,
        ],
    ] {
        assert_eq!(
            codec.decode_input(&bytes).unwrap_err().kind,
            CodecErrorKind::Limit
        );
    }
    let mut deep = String::from("{\"metadata\":");
    deep.push_str(&"{\"x\":".repeat(33));
    deep.push_str("null");
    deep.push_str(&"}".repeat(33));
    deep.push('}');
    assert_eq!(
        super::tests::codec(Encoding::Json)
            .decode_input(deep.as_bytes())
            .unwrap_err()
            .kind,
        CodecErrorKind::Limit
    );
    let input = input();
    for encoding in [Encoding::Json, Encoding::MessagePack] {
        let bytes = super::tests::codec(encoding).encode_input(&input).unwrap();
        let codec = Codec::new(
            encoding,
            Limits {
                max_nodes: 8,
                max_collection_len: 8,
                ..NETWORK_INPUT_LIMITS
            },
        )
        .unwrap();
        assert_eq!(
            codec.decode_input(&bytes).unwrap_err().kind,
            CodecErrorKind::Limit
        );
    }
}
#[test]
fn compatibility_checks_versions_required_features_and_sorted_unique_names() {
    let expected = compatibility();
    assert!(check_compatibility(&expected, &expected).is_ok());
    let mut offered = expected.clone();
    offered.features.push("z-extra".into());
    assert!(check_compatibility(&expected, &offered).is_ok());
    for field in 0..5 {
        let mut offered = expected.clone();
        match field {
            0 => offered.app_id = "other".into(),
            1 => offered.wire_version += 1,
            2 => offered.schema_version += 1,
            3 => offered.presentation_version += 1,
            _ => offered.tick_rate += 1,
        }
        assert!(check_compatibility(&expected, &offered).is_err());
    }
    offered = expected.clone();
    offered.features.remove(0);
    assert!(check_compatibility(&expected, &offered).is_err());
    offered = expected.clone();
    offered.features.swap(0, 1);
    assert!(check_compatibility(&expected, &offered).is_err());
    offered = expected.clone();
    offered.features.push("typed-osc".into());
    assert!(check_compatibility(&expected, &offered).is_err());
}
#[test]
fn native_profile_preserves_legacy_metadata_omission_and_larger_valid_requests() {
    let bare = br#"{"bundle":{"messages":[]}}"#;
    let native = Codec::new(Encoding::Json, NATIVE_INPUT_LIMITS).unwrap();
    assert!(native.decode_input(bare).unwrap().metadata.is_none());
    let input = InputFrame {
        metadata: None,
        bundle: WireBundle {
            messages: vec![WireMessage {
                address: "/large".into(),
                args: vec![WireArg::Str("x".repeat(256 * 1024))],
            }],
        },
    };
    let bytes = native.encode_input(&input).unwrap();
    assert_eq!(native.decode_input(&bytes).unwrap(), input);
    assert_eq!(
        Codec::new(Encoding::Json, NETWORK_INPUT_LIMITS)
            .unwrap()
            .decode_input(&bytes)
            .unwrap_err()
            .kind,
        CodecErrorKind::Limit
    );
}
#[test]
fn structural_osc_errors_nonfinite_and_mismatched_status_are_rejected() {
    for arg in [
        WireArg::Float(f32::INFINITY),
        WireArg::Float(f32::NEG_INFINITY),
        WireArg::Float(f32::NAN),
        WireArg::Str("a\0b".into()),
    ] {
        let mut value = input();
        value.bundle.messages[0].args = vec![arg];
        for encoding in [Encoding::Json, Encoding::MessagePack] {
            assert!(codec(encoding).encode_input(&value).is_err());
        }
    }
    let mut value = input();
    value.bundle.messages[0].address = "bad".into();
    for encoding in [Encoding::Json, Encoding::MessagePack] {
        let bytes = match encoding {
            Encoding::Json => serde_json::to_vec(&value).unwrap(),
            Encoding::MessagePack => rmp_serde::to_vec_named(&value).unwrap(),
        };
        assert!(codec(encoding).decode_input(&bytes).is_err());
    }
    let mut frame = output();
    let ServerPayload::Output(output) = &mut frame.frame else {
        panic!()
    };
    output.status.playback_mode.tick = 0;
    assert!(codec(Encoding::Json).encode_server(&frame).is_err());
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Case {
    name: String,
    kind: String,
    valid: bool,
    json: Option<String>,
    msgpack: Option<String>,
}
fn valid_cases() -> Vec<(&'static str, &'static str, Vec<u8>, Vec<u8>)> {
    let mut cases = vec![];
    let mut client = |name, frame| {
        cases.push((
            name,
            "client",
            codec(Encoding::Json).encode_client(&frame).unwrap(),
            codec(Encoding::MessagePack).encode_client(&frame).unwrap(),
        ))
    };
    client(
        "client-hello",
        ClientFrame::Hello(ClientHello {
            compatibility: compatibility(),
            client_id: "unity-fixture".into(),
            role: Role::Controller,
            expected_session_id: None,
        }),
    );
    client("client-widths", ClientFrame::Input(input()));
    let omitted = json!({"type":"input","payload":{"bundle":{"messages":[]}}});
    cases.push((
        "client-omitted-metadata",
        "client",
        serde_json::to_vec(&omitted).unwrap(),
        codec(Encoding::MessagePack)
            .encode_client(&ClientFrame::Input(InputFrame {
                metadata: None,
                bundle: WireBundle::default(),
            }))
            .unwrap(),
    ));
    let long_float = format!(
        r#"{{"type":"input","payload":{{"metadata":null,"bundle":{{"messages":[{{"address":"/example/value","args":[{{"type":"float","value":1.{}}}]}}]}}}}}}"#,
        "0".repeat(129)
    );
    let normalized = codec(Encoding::Json)
        .decode_client(long_float.as_bytes())
        .unwrap();
    cases.push((
        "client-long-finite-float",
        "client",
        long_float.into_bytes(),
        codec(Encoding::MessagePack)
            .encode_client(&normalized)
            .unwrap(),
    ));
    let bare = InputFrame {
        metadata: None,
        bundle: WireBundle::default(),
    };
    cases.push((
        "input-empty",
        "input",
        codec(Encoding::Json).encode_input(&bare).unwrap(),
        codec(Encoding::MessagePack).encode_input(&bare).unwrap(),
    ));
    let batch = OutputBatch {
        tick: -1,
        bundles: vec![],
    };
    for (name, frame) in [
        (
            "server-hello",
            ServerFrame {
                delivery_sequence: 0,
                frame: ServerPayload::Hello(hello()),
            },
        ),
        ("server-bundles", output()),
        (
            "server-initial",
            ServerFrame {
                delivery_sequence: 1,
                frame: ServerPayload::Snapshot(SnapshotFrame {
                    reason: SnapshotReason::Initial,
                    batch: batch.clone(),
                    status: status(-1),
                }),
            },
        ),
        (
            "server-seek",
            ServerFrame {
                delivery_sequence: 100,
                frame: ServerPayload::Snapshot(SnapshotFrame {
                    reason: SnapshotReason::Seek,
                    batch,
                    status: status(-1),
                }),
            },
        ),
        (
            "server-replay",
            ServerFrame {
                delivery_sequence: 101,
                frame: ServerPayload::Replay(status(i64::MIN)),
            },
        ),
        (
            "server-error",
            ServerFrame {
                delivery_sequence: 102,
                frame: ServerPayload::Error(ErrorFrame {
                    code: ErrorCode::ReadOnly,
                    message: "Replay owns the input queue".into(),
                    fatal: false,
                    input_id: Some(u64::MAX),
                }),
            },
        ),
    ] {
        cases.push((
            name,
            "server",
            codec(Encoding::Json).encode_server(&frame).unwrap(),
            codec(Encoding::MessagePack).encode_server(&frame).unwrap(),
        ));
    }
    cases
}
fn invalid_cases() -> Vec<(&'static str, &'static str, Encoding, Vec<u8>)> {
    let mut cases = vec![];
    for (name, bytes) in [
        (
            "json-duplicate",
            r#"{"type":"input","type":"input","payload":{"metadata":null,"bundle":{"messages":[]}}}"#,
        ),
        (
            "json-unknown",
            r#"{"type":"input","payload":{"metadata":null,"bundle":{"messages":[]},"extra":true}}"#,
        ),
        ("json-unknown-variant", r#"{"type":"resync","payload":{}}"#),
        (
            "json-array-struct",
            r#"{"type":"input","payload":[null,{"messages":[]}] }"#,
        ),
        (
            "json-trailing",
            r#"{"type":"input","payload":{"bundle":{"messages":[]}}} {}"#,
        ),
        (
            "json-comment",
            r#"{/* comment */"type":"input","payload":{"bundle":{"messages":[]}}}"#,
        ),
        (
            "json-trailing-comma",
            r#"{"type":"input","payload":{"bundle":{"messages":[]}},}"#,
        ),
        (
            "json-u64-overflow",
            r#"{"type":"input","payload":{"metadata":{"source":"p","messageId":18446744073709551616,"schemaVersion":1},"bundle":{"messages":[]}}}"#,
        ),
        (
            "json-int-negative-zero",
            r#"{"type":"input","payload":{"metadata":null,"bundle":{"messages":[{"address":"/example/value","args":[{"type":"int","value":-0}]}]}}}"#,
        ),
        (
            "json-int64-negative-zero",
            r#"{"type":"input","payload":{"metadata":null,"bundle":{"messages":[{"address":"/example/value","args":[{"type":"int64","value":-0}]}]}}}"#,
        ),
        (
            "json-u64-negative-zero",
            r#"{"type":"input","payload":{"metadata":{"source":"p","messageId":-0,"schemaVersion":1},"bundle":{"messages":[]}}}"#,
        ),
        (
            "json-missing-nullable",
            r#"{"deliverySequence":0,"frame":{"type":"error","payload":{"code":"protocol","message":"bad","fatal":true}}}"#,
        ),
    ] {
        cases.push((
            name,
            if name == "json-missing-nullable" {
                "server"
            } else {
                "client"
            },
            Encoding::Json,
            bytes.as_bytes().to_vec(),
        ));
    }
    cases.push((
        "json-invalid-utf8",
        "client",
        Encoding::Json,
        vec![b'{', b'"', 255, b'"', b':', b'1', b'}'],
    ));
    for (name, tag, value) in [
        ("i32-overflow", "int", 2_147_483_648_u64),
        ("i64-overflow", "int64", u64::MAX),
    ] {
        let frame = json!({"type":"input","payload":{"metadata":null,"bundle":{"messages":[{"address":"/example/value","args":[{"type":tag,"value":value}]}]}}});
        cases.push((
            name,
            "client",
            Encoding::Json,
            serde_json::to_vec(&frame).unwrap(),
        ));
        cases.push((
            name,
            "client",
            Encoding::MessagePack,
            rmp_serde::to_vec_named(&frame).unwrap(),
        ));
    }
    let bytes = codec(Encoding::MessagePack)
        .encode_client(&ClientFrame::Input(input()))
        .unwrap();
    let offset = bytes
        .windows(7)
        .position(|s| s == [0xa5, b'v', b'a', b'l', b'u', b'e', 0xca])
        .unwrap()
        + 6;
    for (name, replacement) in [
        (
            "mp-float64",
            [vec![0xcb], 1.25f64.to_be_bytes().to_vec()].concat(),
        ),
        ("mp-float-integer", vec![1]),
        ("mp-nonfinite", vec![0xca, 0x7f, 0x80, 0, 0]),
        ("mp-bin", vec![0xc4, 1, 0]),
        ("mp-extension", vec![0xd4, 0, 0]),
    ] {
        let mut invalid = bytes.clone();
        invalid.splice(offset..offset + 5, replacement);
        cases.push((name, "client", Encoding::MessagePack, invalid));
    }
    let mut trailing = bytes.clone();
    trailing.push(0);
    cases.push(("mp-trailing", "client", Encoding::MessagePack, trailing));
    let mut truncated = bytes.clone();
    truncated.pop();
    cases.push(("mp-truncated", "client", Encoding::MessagePack, truncated));
    let mut duplicate = bytes.clone();
    duplicate[0] += 1;
    duplicate.extend_from_slice(&[
        0xa4, b't', b'y', b'p', b'e', 0xa5, b'i', b'n', b'p', b'u', b't',
    ]);
    cases.push(("mp-duplicate", "client", Encoding::MessagePack, duplicate));
    cases.push((
        "mp-huge-map",
        "client",
        Encoding::MessagePack,
        vec![0xdf, 255, 255, 255, 255],
    ));
    cases.push((
        "mp-nonstring-key",
        "client",
        Encoding::MessagePack,
        vec![0x81, 1, 0],
    ));
    cases.push((
        "mp-invalid-utf8",
        "client",
        Encoding::MessagePack,
        vec![0x81, 0xa1, 255, 0],
    ));
    cases.push((
        "mp-array-struct",
        "input",
        Encoding::MessagePack,
        rmp_serde::to_vec(&InputFrame {
            metadata: None,
            bundle: WireBundle::default(),
        })
        .unwrap(),
    ));
    cases
}
fn decode_case(kind: &str, encoding: Encoding, bytes: &[u8]) -> bool {
    match kind {
        "client" => codec(encoding).decode_client(bytes).is_ok(),
        "server" => codec(encoding).decode_server(bytes).is_ok(),
        "input" => codec(encoding).decode_input(bytes).is_ok(),
        _ => panic!(),
    }
}
/// Golden files are generated explicitly once, then normal tests verify bytes and every classification.
#[test]
fn shared_application_wire_fixtures() {
    let directory =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/application-wire");
    let update = std::env::var_os("UPDATE_APPLICATION_WIRE_FIXTURES").is_some();
    let mut cases = vec![];
    let check = |filename: &str, bytes: &[u8]| {
        let path = directory.join(filename);
        if update {
            std::fs::write(&path, bytes).unwrap();
        }
        assert_eq!(std::fs::read(path).unwrap(), bytes, "fixture {filename}");
    };
    for (name, kind, json, msgpack) in valid_cases() {
        assert!(decode_case(kind, Encoding::Json, &json), "{name} JSON");
        assert!(
            decode_case(kind, Encoding::MessagePack, &msgpack),
            "{name} MessagePack"
        );
        let json_name = format!("{name}.json");
        let msgpack_name = format!("{name}.msgpack");
        check(&json_name, &json);
        check(&msgpack_name, &msgpack);
        cases.push(Case {
            name: name.into(),
            kind: kind.into(),
            valid: true,
            json: Some(json_name),
            msgpack: Some(msgpack_name),
        });
    }
    for (name, kind, encoding, bytes) in invalid_cases() {
        assert!(
            !decode_case(kind, encoding, &bytes),
            "accepted invalid {name}"
        );
        let filename = format!(
            "{name}.{}",
            if encoding == Encoding::Json {
                "json"
            } else {
                "msgpack"
            }
        );
        check(&filename, &bytes);
        cases.push(Case {
            name: name.into(),
            kind: kind.into(),
            valid: false,
            json: (encoding == Encoding::Json).then(|| filename.clone()),
            msgpack: (encoding == Encoding::MessagePack).then_some(filename),
        });
    }
    check(
        "manifest.json",
        &serde_json::to_vec_pretty(&json!({"schemaVersion":1,"cases":cases})).unwrap(),
    );
}

fn canonical_messagepack(kind: &str, encoding: Encoding, bytes: &[u8]) -> Vec<u8> {
    let decoder = codec(encoding);
    let encoder = codec(Encoding::MessagePack);
    match kind {
        "client" => encoder
            .encode_client(&decoder.decode_client(bytes).unwrap())
            .unwrap(),
        "server" => encoder
            .encode_server(&decoder.decode_server(bytes).unwrap())
            .unwrap(),
        "input" => encoder
            .encode_input(&decoder.decode_input(bytes).unwrap())
            .unwrap(),
        _ => panic!("unknown fixture kind"),
    }
}

#[test]
fn csharp_reencoded_frames_decode_to_identical_typed_values_and_float_bits() {
    let Some(directory) = std::env::var_os("KITU_APPLICATION_WIRE_CSHARP_FIXTURES") else {
        eprintln!("C# exports were not provided; run with KITU_APPLICATION_WIRE_CSHARP_FIXTURES for cross-language evidence");
        return;
    };
    let directory = std::path::PathBuf::from(directory);
    let cases = valid_cases();
    for (name, kind, json, messagepack) in &cases {
        let expected = canonical_messagepack(kind, Encoding::MessagePack, messagepack);
        assert_eq!(canonical_messagepack(kind, Encoding::Json, json), expected);
        for (encoding, extension) in [(Encoding::Json, "json"), (Encoding::MessagePack, "msgpack")]
        {
            let path = directory.join(format!("{name}.{extension}"));
            let bytes = std::fs::read(&path)
                .unwrap_or_else(|error| panic!("C# export {}: {error}", path.display()));
            // Canonical MP includes the exact f32 bits, unlike PartialEq's
            // equality between positive and negative floating-point zero.
            assert_eq!(
                canonical_messagepack(kind, encoding, &bytes),
                expected,
                "C# {name} {encoding:?}: logical type, width, order, or float bits diverged"
            );
        }
    }
    println!(
        "verified {} current C# frames in both JSON and MessagePack",
        cases.len()
    );
}
