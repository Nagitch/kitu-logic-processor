//! Typed Serde representation of flat, immediate OSC-IR bundles.
//!
//! JSON, MessagePack, and native-library adapters share these data types instead
//! of inferring OSC argument widths from the serialization format. Conversion
//! to or from [`OscBundle`] validates OSC-safe strings and finite floats. The
//! wire types themselves are data transfer objects: deserialization must be
//! followed by [`OscBundle::try_from`] before admitting input to a runtime.
//!
//! This module does not apply application rules, schedule ticks, or impose
//! transport-specific size limits. Nested bundles and timetags are not part of
//! the core OSC-IR model, and unknown fields are rejected.

use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use serde::{Deserialize, Serialize};

use crate::KepCodecError;

/// An OSC argument whose explicit tag preserves its scalar type and width.
///
/// For example, `Int64(1)` serializes as `{"type":"int64","value":1}`;
/// it remains distinct from `Int(1)` even though their numeric values match.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "value",
    rename_all = "lowercase",
    deny_unknown_fields
)]
pub enum WireArg {
    /// A signed 32-bit integer (`int`).
    Int(i32),
    /// A signed 64-bit integer (`int64`), including values outside JSON clients'
    /// floating-point safe integer range.
    Int64(i64),
    /// A 32-bit floating-point value (`float`); conversion requires finiteness.
    Float(f32),
    /// A UTF-8 string (`str`); conversion rejects embedded NUL bytes.
    Str(String),
    /// A Boolean value (`bool`).
    Bool(bool),
}

/// A typed OSC message with arguments in their original order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireMessage {
    /// OSC address beginning with `/`, without embedded NUL bytes.
    pub address: String,
    /// Explicitly typed arguments, including an empty list when appropriate.
    pub args: Vec<WireArg>,
}

/// An ordered collection of immediate OSC messages, including an empty bundle.
///
/// Convert through [`TryFrom`] in both directions to validate OSC-compatible
/// values. Addresses must start with `/`; addresses and strings cannot contain
/// NUL bytes, and floats must be finite. Application admission remains a
/// separate concern.
///
/// # Examples
///
/// ```
/// use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
/// use kitu_transport::wire::WireBundle;
///
/// let mut message = OscMessage::new("/example/count");
/// message.push_arg(OscArg::Int64(i64::MAX));
/// let bundle = OscBundle { messages: vec![message] };
/// let wire = WireBundle::try_from(&bundle)?;
/// let restored = OscBundle::try_from(wire)?;
/// assert_eq!(restored, bundle);
/// # Ok::<(), kitu_transport::KepCodecError>(())
/// ```
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WireBundle {
    /// Messages in delivery order; no flattening or sorting is performed.
    pub messages: Vec<WireMessage>,
}

impl TryFrom<&OscBundle> for WireBundle {
    type Error = KepCodecError;

    fn try_from(bundle: &OscBundle) -> Result<Self, Self::Error> {
        let messages = bundle
            .messages
            .iter()
            .map(|message| {
                validate_address(&message.address)?;
                let args = message
                    .args
                    .iter()
                    .map(|arg| {
                        Ok(match arg {
                            OscArg::Int(value) => WireArg::Int(*value),
                            OscArg::Int64(value) => WireArg::Int64(*value),
                            OscArg::Float(value) => {
                                validate_float(*value)?;
                                WireArg::Float(*value)
                            }
                            OscArg::Str(value) => {
                                validate_string(value)?;
                                WireArg::Str(value.clone())
                            }
                            OscArg::Bool(value) => WireArg::Bool(*value),
                        })
                    })
                    .collect::<Result<_, KepCodecError>>()?;
                Ok(WireMessage {
                    address: message.address.clone(),
                    args,
                })
            })
            .collect::<Result<_, KepCodecError>>()?;
        Ok(Self { messages })
    }
}

impl TryFrom<WireBundle> for OscBundle {
    type Error = KepCodecError;

    fn try_from(bundle: WireBundle) -> Result<Self, Self::Error> {
        let messages = bundle
            .messages
            .into_iter()
            .map(|message| {
                validate_address(&message.address)?;
                let args = message
                    .args
                    .into_iter()
                    .map(|arg| {
                        Ok(match arg {
                            WireArg::Int(value) => OscArg::Int(value),
                            WireArg::Int64(value) => OscArg::Int64(value),
                            WireArg::Float(value) => {
                                validate_float(value)?;
                                OscArg::Float(value)
                            }
                            WireArg::Str(value) => {
                                validate_string(&value)?;
                                OscArg::Str(value)
                            }
                            WireArg::Bool(value) => OscArg::Bool(value),
                        })
                    })
                    .collect::<Result<_, KepCodecError>>()?;
                Ok(OscMessage {
                    address: message.address,
                    args,
                })
            })
            .collect::<Result<_, KepCodecError>>()?;
        Ok(Self { messages })
    }
}

fn validate_address(address: &str) -> Result<(), KepCodecError> {
    if !address.starts_with('/') || address.contains('\0') {
        return Err(KepCodecError::InvalidOsc(
            "address must start with / and contain no NUL bytes",
        ));
    }
    Ok(())
}

fn validate_string(value: &str) -> Result<(), KepCodecError> {
    if value.contains('\0') {
        return Err(KepCodecError::InvalidOsc(
            "string argument must contain no NUL bytes",
        ));
    }
    Ok(())
}

fn validate_float(value: f32) -> Result<(), KepCodecError> {
    if !value.is_finite() {
        return Err(KepCodecError::InvalidOsc("float argument must be finite"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_types() -> OscBundle {
        OscBundle {
            messages: vec![
                OscMessage {
                    address: "/example/all".into(),
                    args: vec![
                        OscArg::Int(i32::MIN),
                        OscArg::Int(i32::MAX),
                        OscArg::Int64(i64::MIN),
                        OscArg::Int64(i64::MAX),
                        OscArg::Int(1),
                        OscArg::Int64(1),
                        OscArg::Float(-0.0),
                        OscArg::Float(f32::MAX),
                        OscArg::Float(f32::from_bits(1)),
                        OscArg::Str("日本語 /,\n\"".into()),
                        OscArg::Str(String::new()),
                        OscArg::Bool(true),
                        OscArg::Bool(false),
                    ],
                },
                OscMessage::new("/example/empty"),
                OscMessage::new("/example/all"),
            ],
        }
    }

    fn assert_exact(actual: &OscBundle, expected: &OscBundle) {
        assert_eq!(actual, expected);
        for (actual, expected) in actual.messages.iter().zip(&expected.messages) {
            for (actual, expected) in actual.args.iter().zip(&expected.args) {
                if let (OscArg::Float(actual), OscArg::Float(expected)) = (actual, expected) {
                    assert_eq!(actual.to_bits(), expected.to_bits());
                }
            }
        }
    }

    #[test]
    fn json_and_messagepack_preserve_argument_widths_float_bits_and_order() {
        for bundle in [all_types(), OscBundle::new()] {
            let wire = WireBundle::try_from(&bundle).unwrap();
            let json = serde_json::to_vec(&wire).unwrap();
            let from_json: WireBundle = serde_json::from_slice(&json).unwrap();
            assert_exact(&OscBundle::try_from(from_json).unwrap(), &bundle);

            let messagepack = rmp_serde::to_vec_named(&wire).unwrap();
            let from_messagepack: WireBundle = rmp_serde::from_slice(&messagepack).unwrap();
            assert_exact(&OscBundle::try_from(from_messagepack).unwrap(), &bundle);

            let osc = crate::encode_osc_bundle(&OscBundle::try_from(wire).unwrap()).unwrap();
            assert_exact(&crate::decode_osc_bundle(&osc).unwrap(), &bundle);
        }
    }

    #[test]
    fn explicit_wire_tags_do_not_infer_numeric_widths() {
        let wire = WireBundle::try_from(&all_types()).unwrap();
        let json = serde_json::to_value(wire).unwrap();
        let args = &json["messages"][0]["args"];
        assert_eq!(args[0]["type"], "int");
        assert_eq!(args[2]["type"], "int64");
        assert_eq!(args[2]["value"].as_i64(), Some(i64::MIN));
        assert_eq!(args[3]["value"].as_i64(), Some(i64::MAX));
        assert_eq!(args[6]["type"], "float");
        assert_eq!(args[9]["type"], "str");
        assert_eq!(args[11]["type"], "bool");
    }

    #[test]
    fn malformed_unknown_and_unsupported_wire_shapes_are_rejected() {
        for json in [
            r#"{}"#,
            r#"{"messages":[],"timetag":1}"#,
            r#"{"messages":[{"address":"/a","args":[],"extra":1}]}"#,
            r#"{"messages":[{"address":"/a"}]}"#,
            r#"{"messages":[{"messages":[]}]}"#,
            r#"{"messages":[{"address":"/a","args":[{"type":"int","value":1,"extra":0}]}]}"#,
            r#"{"messages":[{"address":"/a","args":[{"type":"int","value":2147483648}]}]}"#,
            r#"{"messages":[{"address":"/a","args":[{"type":"int64","value":9223372036854775808}]}]}"#,
            r#"{"messages":[{"address":"/a","args":[{"type":"float","value":null}]}]}"#,
            r#"{"messages":[{"address":"/a","args":[{"type":"bool","value":1}]}]}"#,
            r#"{"messages":[{"address":"/a","args":[{"type":"blob","value":[]}]}]}"#,
            r#"{"messages":[{"address":"/a","args":[{"value":1}]}]}"#,
        ] {
            assert!(serde_json::from_str::<WireBundle>(json).is_err(), "{json}");
        }
    }

    #[test]
    fn invalid_addresses_and_nul_strings_fail_in_both_directions() {
        let valid = WireBundle {
            messages: vec![WireMessage {
                address: "/example/check".into(),
                args: Vec::new(),
            }],
        };
        for address in ["", "relative", "#bundle", "/a\0b"] {
            let mut wire = valid.clone();
            wire.messages[0].address = address.into();
            assert!(OscBundle::try_from(wire).is_err());
            let raw = OscBundle {
                messages: vec![OscMessage::new(address)],
            };
            assert!(WireBundle::try_from(&raw).is_err());
        }

        let mut wire = valid;
        wire.messages[0].args.push(WireArg::Str("a\0b".into()));
        assert!(OscBundle::try_from(wire).is_err());
        let mut raw = OscBundle::new();
        let mut message = OscMessage::new("/example/check");
        message.push_arg(OscArg::Str("a\0b".into()));
        raw.push(message);
        assert!(WireBundle::try_from(&raw).is_err());
    }

    #[test]
    fn nonfinite_floats_fail_before_runtime_admission_or_output_encoding() {
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let wire = WireBundle {
                messages: vec![WireMessage {
                    address: "/example/check".into(),
                    args: vec![WireArg::Float(value)],
                }],
            };
            // MessagePack can carry nonfinite floats, so Serde alone is not
            // sufficient validation of an incoming bundle.
            let bytes = rmp_serde::to_vec_named(&wire).unwrap();
            let decoded: WireBundle = rmp_serde::from_slice(&bytes).unwrap();
            assert!(OscBundle::try_from(decoded).is_err());
            let raw = OscBundle {
                messages: vec![OscMessage {
                    address: "/example/check".into(),
                    args: vec![OscArg::Float(value)],
                }],
            };
            assert!(WireBundle::try_from(&raw).is_err());
        }

        let oversized = r#"{"messages":[{"address":"/a","args":[{"type":"float","value":1e39}]}]}"#;
        // Depending on the deserializer, a finite JSON number outside f32's
        // range can fail decoding or become infinity. Neither may be admitted.
        if let Ok(wire) = serde_json::from_str::<WireBundle>(oversized) {
            assert!(OscBundle::try_from(wire).is_err());
        }
    }
}
