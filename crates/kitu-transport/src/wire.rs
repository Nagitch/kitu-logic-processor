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
use serde::{
    ser::{Error as _, SerializeSeq, SerializeStruct},
    Deserialize, Serialize, Serializer,
};

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

/// A serialization view that borrows an OSC bundle without cloning its messages
/// or strings.
///
/// The serialized representation is identical to [`WireBundle`]. Validation
/// occurs as the serializer visits each value, so a bounded writer can stop
/// before later messages are traversed or copied. A failed serialization may
/// have written a prefix; callers must discard it rather than publish it.
#[derive(Debug, Clone, Copy)]
pub struct WireBundleRef<'a>(&'a OscBundle);

impl<'a> WireBundleRef<'a> {
    /// Borrows a bundle for streaming serialization with OSC validation.
    ///
    /// # Examples
    /// ```
    /// use kitu_osc_ir::OscBundle;
    /// use kitu_transport::wire::WireBundleRef;
    /// let bundle = OscBundle::new();
    /// assert_eq!(serde_json::to_string(&WireBundleRef::new(&bundle))?,
    ///     r#"{"messages":[]}"#);
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    pub fn new(bundle: &'a OscBundle) -> Self {
        Self(bundle)
    }
}

/// A borrowed sequence of OSC bundles serialized without an intermediate owned
/// wire collection.
///
/// Bundles, messages, and arguments retain their original order. Validation and
/// writer errors stop iteration immediately. Like [`WireBundleRef`], this view
/// may write a prefix before reporting an error.
#[derive(Debug, Clone, Copy)]
pub struct WireBundlesRef<'a>(&'a [OscBundle]);

impl<'a> WireBundlesRef<'a> {
    /// Borrows an entire output batch for incremental serialization.
    ///
    /// # Examples
    /// ```
    /// use kitu_osc_ir::OscBundle;
    /// use kitu_transport::wire::WireBundlesRef;
    /// let bundles = [OscBundle::new()];
    /// assert_eq!(serde_json::to_string(&WireBundlesRef::new(&bundles))?,
    ///     r#"[{"messages":[]}]"#);
    /// # Ok::<(), serde_json::Error>(())
    /// ```
    pub fn new(bundles: &'a [OscBundle]) -> Self {
        Self(bundles)
    }
}

impl Serialize for WireBundlesRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for bundle in self.0 {
            sequence.serialize_element(&WireBundleRef::new(bundle))?;
        }
        sequence.end()
    }
}

impl Serialize for WireBundleRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut object = serializer.serialize_struct("WireBundle", 1)?;
        object.serialize_field("messages", &WireMessagesRef(&self.0.messages))?;
        object.end()
    }
}

struct WireMessagesRef<'a>(&'a [OscMessage]);

impl Serialize for WireMessagesRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for message in self.0 {
            sequence.serialize_element(&WireMessageRef(message))?;
        }
        sequence.end()
    }
}

/// Borrowed canonical OSC message serializer, including structural validation.
///
/// This view writes strings directly to the caller's serializer, so bounded
/// writers do not first clone an entire potentially large message.
///
/// # Examples
/// ```
/// use kitu_osc_ir::OscMessage;
/// use kitu_transport::wire::WireMessageRef;
/// let message = OscMessage::new("/example");
/// assert_eq!(serde_json::to_string(&WireMessageRef::new(&message))?,
///            r#"{"address":"/example","args":[]}"#);
/// # Ok::<(), serde_json::Error>(())
/// ```
pub struct WireMessageRef<'a>(&'a OscMessage);

impl<'a> WireMessageRef<'a> {
    /// Borrows a message; serialization validates its address and scalar values.
    pub fn new(message: &'a OscMessage) -> Self {
        Self(message)
    }
}

impl Serialize for WireMessageRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        validate_address(&self.0.address).map_err(S::Error::custom)?;
        let mut object = serializer.serialize_struct("WireMessage", 2)?;
        object.serialize_field("address", &self.0.address)?;
        object.serialize_field("args", &WireArgsRef(&self.0.args))?;
        object.end()
    }
}

struct WireArgsRef<'a>(&'a [OscArg]);

impl Serialize for WireArgsRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.0.len()))?;
        for arg in self.0 {
            sequence.serialize_element(&WireArgRef(arg))?;
        }
        sequence.end()
    }
}

struct WireArgRef<'a>(&'a OscArg);

impl Serialize for WireArgRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut object = serializer.serialize_struct("WireArg", 2)?;
        match self.0 {
            OscArg::Int(value) => {
                object.serialize_field("type", "int")?;
                object.serialize_field("value", value)?;
            }
            OscArg::Int64(value) => {
                object.serialize_field("type", "int64")?;
                object.serialize_field("value", value)?;
            }
            OscArg::Float(value) => {
                validate_float(*value).map_err(S::Error::custom)?;
                object.serialize_field("type", "float")?;
                object.serialize_field("value", value)?;
            }
            OscArg::Str(value) => {
                validate_string(value).map_err(S::Error::custom)?;
                object.serialize_field("type", "str")?;
                object.serialize_field("value", value)?;
            }
            OscArg::Bool(value) => {
                object.serialize_field("type", "bool")?;
                object.serialize_field("value", value)?;
            }
        }
        object.end()
    }
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

pub(crate) fn validate_address(address: &str) -> Result<(), KepCodecError> {
    if !address.starts_with('/') || address.contains('\0') {
        return Err(KepCodecError::InvalidOsc(
            "address must start with / and contain no NUL bytes",
        ));
    }
    Ok(())
}

pub(crate) fn validate_string(value: &str) -> Result<(), KepCodecError> {
    if value.contains('\0') {
        return Err(KepCodecError::InvalidOsc(
            "string argument must contain no NUL bytes",
        ));
    }
    Ok(())
}

pub(crate) fn validate_float(value: f32) -> Result<(), KepCodecError> {
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
    fn borrowed_encoding_matches_owned_json_and_messagepack_bytes() {
        let bundles = [all_types(), OscBundle::new()];
        for bundle in &bundles {
            let owned = WireBundle::try_from(bundle).unwrap();
            let borrowed = WireBundleRef::new(bundle);
            for (message, owned_message) in bundle.messages.iter().zip(&owned.messages) {
                assert_eq!(
                    serde_json::to_vec(&WireMessageRef::new(message)).unwrap(),
                    serde_json::to_vec(owned_message).unwrap()
                );
                assert_eq!(
                    rmp_serde::to_vec_named(&WireMessageRef::new(message)).unwrap(),
                    rmp_serde::to_vec_named(owned_message).unwrap()
                );
            }
            assert_eq!(
                serde_json::to_vec(&borrowed).unwrap(),
                serde_json::to_vec(&owned).unwrap()
            );
            assert_eq!(
                rmp_serde::to_vec_named(&borrowed).unwrap(),
                rmp_serde::to_vec_named(&owned).unwrap()
            );
            assert_eq!(
                rmp_serde::to_vec(&borrowed).unwrap(),
                rmp_serde::to_vec(&owned).unwrap()
            );
        }
        for batch in [bundles.as_slice(), &[]] {
            let owned: Vec<_> = batch
                .iter()
                .map(|bundle| WireBundle::try_from(bundle).unwrap())
                .collect();
            let borrowed = WireBundlesRef::new(batch);
            assert_eq!(
                serde_json::to_vec(&borrowed).unwrap(),
                serde_json::to_vec(&owned).unwrap()
            );
            assert_eq!(
                rmp_serde::to_vec_named(&borrowed).unwrap(),
                rmp_serde::to_vec_named(&owned).unwrap()
            );
        }
    }

    #[test]
    fn borrowed_serialization_rejects_the_same_invalid_osc_values() {
        let mut cases: Vec<_> = ["", "relative", "/a\0b"]
            .into_iter()
            .map(|address| OscBundle {
                messages: vec![OscMessage::new(address)],
            })
            .collect();
        for arg in [
            OscArg::Str("a\0b".into()),
            OscArg::Float(f32::NAN),
            OscArg::Float(f32::INFINITY),
            OscArg::Float(f32::NEG_INFINITY),
        ] {
            cases.push(OscBundle {
                messages: vec![OscMessage {
                    address: "/example/invalid".into(),
                    args: vec![arg],
                }],
            });
        }
        for bundle in &cases {
            let borrowed = WireBundleRef::new(bundle);
            assert!(WireBundle::try_from(bundle).is_err());
            assert!(serde_json::to_vec(&borrowed).is_err());
            assert!(rmp_serde::to_vec_named(&borrowed).is_err());
        }
    }

    #[test]
    fn bounded_writer_stops_before_visiting_later_bundles() {
        struct Budget {
            written: usize,
            limit: usize,
        }
        impl std::io::Write for Budget {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                if bytes.len() > self.limit - self.written {
                    return Err(std::io::Error::other("test byte budget exhausted"));
                }
                self.written += bytes.len();
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let bundles = [
            OscBundle {
                messages: vec![OscMessage {
                    address: "/example/large".into(),
                    args: vec![OscArg::Str("x".repeat(1024 * 1024))],
                }],
            },
            // A pre-conversion of the complete batch would fail on this address
            // before the writer's earlier byte limit could take effect.
            OscBundle {
                messages: vec![OscMessage::new("invalid")],
            },
        ];
        let mut writer = Budget {
            written: 0,
            limit: 128,
        };
        let error = serde_json::to_writer(&mut writer, &WireBundlesRef::new(&bundles)).unwrap_err();
        assert!(error.to_string().contains("test byte budget exhausted"));
        assert!(writer.written <= writer.limit);
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
