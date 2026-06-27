//! Transport abstraction for delivering OSC-IR messages between peers.
//!
//! # Responsibilities
//! - Define the [`Transport`] trait and event model for moving OSC/IR messages around the system.
//! - Host concrete adapters (e.g., in-memory channels) while staying open to networked transports.
//! - Keep delivery concerns isolated from gameplay logic and runtime scheduling.
//!
//! # Integration
//! Transports bridge OSC/IR types (`kitu-osc-ir`) with the runtime loop (`kitu-runtime`). See
//! `doc/crates-overview.md` for adapter expectations and how events flow into ECS systems.

use std::collections::VecDeque;

use kitu_core::{KituError, Result};
use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const OSC_BUNDLE_HEADER: &[u8; 8] = b"#bundle\0";
const OSC_IMMEDIATE_TIMETAG: u64 = 1;

/// Payload type used for OSC packet binaries inside KEP envelopes.
pub const KEP_PAYLOAD_OSC: &str = "osc";
/// Payload type used for UTF-8 JSON bytes inside KEP envelopes.
pub const KEP_PAYLOAD_JSON: &str = "json";

/// Kitu Envelope Protocol message.
///
/// KEP is a transport-independent MessagePack map. It intentionally carries
/// metadata separately from the application payload, so transports can route or
/// correlate messages without mutating OSC packet bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KepEnvelope {
    /// Payload type, for example `osc`.
    #[serde(rename = "t")]
    pub payload_type: String,
    /// Optional route identifier.
    #[serde(rename = "r", default, skip_serializing_if = "Option::is_none")]
    pub route: Option<String>,
    /// Optional correlation identifier.
    #[serde(rename = "i", default, skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<u64>,
    /// Optional implementation-specific flags.
    #[serde(rename = "f", default, skip_serializing_if = "Option::is_none")]
    pub flags: Option<u64>,
    /// Application payload bytes.
    #[serde(rename = "p", with = "serde_bytes")]
    pub payload: Vec<u8>,
}

impl KepEnvelope {
    /// Creates an OSC KEP envelope from OSC packet bytes.
    pub fn osc(payload: Vec<u8>) -> Self {
        Self {
            payload_type: KEP_PAYLOAD_OSC.to_string(),
            route: None,
            correlation_id: None,
            flags: None,
            payload,
        }
    }

    /// Creates a JSON KEP envelope from UTF-8 JSON bytes.
    pub fn json(payload: Vec<u8>) -> Self {
        Self {
            payload_type: KEP_PAYLOAD_JSON.to_string(),
            route: None,
            correlation_id: None,
            flags: None,
            payload,
        }
    }
}

/// Error returned by KEP or OSC packet encode/decode helpers.
#[derive(Debug, Error)]
pub enum KepCodecError {
    /// MessagePack encoding failed.
    #[error("encode KEP envelope: {0}")]
    EncodeEnvelope(#[from] rmp_serde::encode::Error),
    /// MessagePack decoding failed.
    #[error("decode KEP envelope: {0}")]
    DecodeEnvelope(rmp_serde::decode::Error),
    /// OSC packet content is malformed or unsupported.
    #[error("invalid OSC packet: {0}")]
    InvalidOsc(&'static str),
    /// The OSC packet used an unsupported type tag.
    #[error("unsupported OSC type tag: {0}")]
    UnsupportedOscType(char),
}

/// Encodes a KEP envelope as MessagePack bytes.
pub fn encode_kep_envelope(envelope: &KepEnvelope) -> std::result::Result<Vec<u8>, KepCodecError> {
    Ok(rmp_serde::to_vec_named(envelope)?)
}

/// Decodes a KEP envelope from MessagePack bytes.
pub fn decode_kep_envelope(bytes: &[u8]) -> std::result::Result<KepEnvelope, KepCodecError> {
    rmp_serde::from_slice(bytes).map_err(KepCodecError::DecodeEnvelope)
}

/// Encodes a single OSC-IR message into an OSC packet binary.
pub fn encode_osc_packet(message: &OscMessage) -> std::result::Result<Vec<u8>, KepCodecError> {
    if message.address.is_empty() {
        return Err(KepCodecError::InvalidOsc("address must not be empty"));
    }

    let mut bytes = Vec::new();
    write_osc_string(&mut bytes, &message.address);

    let mut type_tags = String::from(",");
    for arg in &message.args {
        type_tags.push(match arg {
            OscArg::Int(_) => 'i',
            OscArg::Int64(_) => 'h',
            OscArg::Float(_) => 'f',
            OscArg::Str(_) => 's',
            OscArg::Bool(true) => 'T',
            OscArg::Bool(false) => 'F',
        });
    }
    write_osc_string(&mut bytes, &type_tags);

    for arg in &message.args {
        match arg {
            OscArg::Int(value) => bytes.extend_from_slice(&value.to_be_bytes()),
            OscArg::Int64(value) => bytes.extend_from_slice(&value.to_be_bytes()),
            OscArg::Float(value) => bytes.extend_from_slice(&value.to_bits().to_be_bytes()),
            OscArg::Str(value) => write_osc_string(&mut bytes, value),
            OscArg::Bool(_) => {}
        }
    }

    Ok(bytes)
}

/// Encodes an OSC-IR bundle into an OSC bundle packet binary.
///
/// Kitu currently models bundle scheduling outside the OSC payload, so encoded
/// bundles use the OSC immediate timetag. Nested bundles are not represented by
/// [`OscBundle`] and are therefore not emitted by this helper.
///
/// # Examples
///
/// ```
/// use kitu_osc_ir::{OscArg, OscBundle, OscMessage};
/// use kitu_transport::{decode_osc_bundle, encode_osc_bundle};
///
/// let mut message = OscMessage::new("/input/move");
/// message.push_arg(OscArg::Float(1.0));
///
/// let mut bundle = OscBundle::new();
/// bundle.push(message);
///
/// let bytes = encode_osc_bundle(&bundle).expect("encode bundle");
/// let decoded = decode_osc_bundle(&bytes).expect("decode bundle");
/// assert_eq!(decoded.messages.len(), 1);
/// ```
pub fn encode_osc_bundle(bundle: &OscBundle) -> std::result::Result<Vec<u8>, KepCodecError> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(OSC_BUNDLE_HEADER);
    bytes.extend_from_slice(&OSC_IMMEDIATE_TIMETAG.to_be_bytes());

    for message in &bundle.messages {
        let packet = encode_osc_packet(message)?;
        let packet_len = i32::try_from(packet.len())
            .map_err(|_| KepCodecError::InvalidOsc("bundle element is too large"))?;
        bytes.extend_from_slice(&packet_len.to_be_bytes());
        bytes.extend_from_slice(&packet);
    }

    Ok(bytes)
}

/// Decodes a single OSC packet binary into an OSC-IR message.
pub fn decode_osc_packet(bytes: &[u8]) -> std::result::Result<OscMessage, KepCodecError> {
    let (message, offset) = decode_osc_packet_with_offset(bytes)?;
    if offset != bytes.len() {
        return Err(KepCodecError::InvalidOsc("OSC packet has trailing bytes"));
    }
    Ok(message)
}

fn decode_osc_packet_with_offset(
    bytes: &[u8],
) -> std::result::Result<(OscMessage, usize), KepCodecError> {
    let (address, mut offset) = read_osc_string(bytes, 0)?;
    if address.is_empty() {
        return Err(KepCodecError::InvalidOsc("address must not be empty"));
    }

    let (type_tags, next_offset) = read_osc_string(bytes, offset)?;
    offset = next_offset;
    let Some(tags) = type_tags.strip_prefix(',') else {
        return Err(KepCodecError::InvalidOsc(
            "type tag string must start with comma",
        ));
    };

    let mut message = OscMessage::new(address);
    for tag in tags.chars() {
        match tag {
            'i' => {
                let value = read_i32(bytes, offset)?;
                offset += 4;
                message.push_arg(OscArg::Int(value));
            }
            'h' => {
                let value = read_i64(bytes, offset)?;
                offset += 8;
                message.push_arg(OscArg::Int64(value));
            }
            'f' => {
                let value = f32::from_bits(read_u32(bytes, offset)?);
                offset += 4;
                message.push_arg(OscArg::Float(value));
            }
            's' => {
                let (value, next_offset) = read_osc_string(bytes, offset)?;
                offset = next_offset;
                message.push_arg(OscArg::Str(value));
            }
            'T' => message.push_arg(OscArg::Bool(true)),
            'F' => message.push_arg(OscArg::Bool(false)),
            other => return Err(KepCodecError::UnsupportedOscType(other)),
        }
    }

    if offset > bytes.len() {
        return Err(KepCodecError::InvalidOsc("packet ended before arguments"));
    }

    Ok((message, offset))
}

/// Decodes an OSC bundle packet binary into an OSC-IR bundle.
///
/// This helper accepts bundles containing message elements. Nested bundle
/// elements are rejected until `OscBundle` grows an explicit nested-bundle
/// representation.
///
/// # Examples
///
/// ```
/// use kitu_osc_ir::{OscBundle, OscMessage};
/// use kitu_transport::{decode_osc_bundle, encode_osc_bundle};
///
/// let mut bundle = OscBundle::new();
/// bundle.push(OscMessage::new("/tick"));
///
/// let bytes = encode_osc_bundle(&bundle).expect("encode bundle");
/// let decoded = decode_osc_bundle(&bytes).expect("decode bundle");
/// assert_eq!(decoded.messages[0].address, "/tick");
/// ```
pub fn decode_osc_bundle(bytes: &[u8]) -> std::result::Result<OscBundle, KepCodecError> {
    if bytes.len() < 16 {
        return Err(KepCodecError::InvalidOsc("bundle packet is too short"));
    }
    if bytes.get(..8) != Some(OSC_BUNDLE_HEADER) {
        return Err(KepCodecError::InvalidOsc("missing OSC bundle header"));
    }
    let timetag = u64::from_be_bytes(
        bytes[8..16]
            .try_into()
            .expect("bundle timetag length checked"),
    );
    if timetag != OSC_IMMEDIATE_TIMETAG {
        return Err(KepCodecError::InvalidOsc(
            "non-immediate OSC bundle timetags are not supported",
        ));
    }

    let mut offset = 16;
    let mut bundle = OscBundle::new();
    while offset < bytes.len() {
        let element_len = read_i32(bytes, offset)?;
        offset += 4;
        let element_len = usize::try_from(element_len)
            .map_err(|_| KepCodecError::InvalidOsc("bundle element size is negative"))?;
        let element_end = offset
            .checked_add(element_len)
            .ok_or(KepCodecError::InvalidOsc("bundle element size overflows"))?;
        let element = bytes
            .get(offset..element_end)
            .ok_or(KepCodecError::InvalidOsc("bundle element is truncated"))?;
        if element.get(..8) == Some(OSC_BUNDLE_HEADER) {
            return Err(KepCodecError::InvalidOsc(
                "nested OSC bundles are not supported",
            ));
        }
        bundle.push(decode_osc_packet(element)?);
        offset = element_end;
    }

    Ok(bundle)
}

fn write_osc_string(bytes: &mut Vec<u8>, value: &str) {
    bytes.extend_from_slice(value.as_bytes());
    bytes.push(0);
    while bytes.len() % 4 != 0 {
        bytes.push(0);
    }
}

fn read_osc_string(
    bytes: &[u8],
    offset: usize,
) -> std::result::Result<(String, usize), KepCodecError> {
    if offset >= bytes.len() {
        return Err(KepCodecError::InvalidOsc("missing OSC string"));
    }

    let Some(relative_end) = bytes[offset..].iter().position(|byte| *byte == 0) else {
        return Err(KepCodecError::InvalidOsc(
            "OSC string is not null-terminated",
        ));
    };
    let end = offset + relative_end;
    let value = std::str::from_utf8(&bytes[offset..end])
        .map_err(|_| KepCodecError::InvalidOsc("OSC string is not UTF-8"))?
        .to_string();
    let mut next = end + 1;
    while next % 4 != 0 {
        next += 1;
    }
    if next > bytes.len() {
        return Err(KepCodecError::InvalidOsc(
            "OSC string padding is incomplete",
        ));
    }
    Ok((value, next))
}

fn read_u32(bytes: &[u8], offset: usize) -> std::result::Result<u32, KepCodecError> {
    let data = bytes
        .get(offset..offset + 4)
        .ok_or(KepCodecError::InvalidOsc("missing 32-bit argument"))?;
    Ok(u32::from_be_bytes(
        data.try_into().expect("slice length checked"),
    ))
}

fn read_i32(bytes: &[u8], offset: usize) -> std::result::Result<i32, KepCodecError> {
    Ok(read_u32(bytes, offset)? as i32)
}

fn read_i64(bytes: &[u8], offset: usize) -> std::result::Result<i64, KepCodecError> {
    let data = bytes
        .get(offset..offset + 8)
        .ok_or(KepCodecError::InvalidOsc("missing 64-bit argument"))?;
    Ok(i64::from_be_bytes(
        data.try_into().expect("slice length checked"),
    ))
}

/// Event emitted by a transport implementation.
///
/// Implementations should emit `Connected` and `Disconnected` when peer state
/// changes, and `Message` whenever an OSC bundle is ready for processing.
#[derive(Debug, Clone, PartialEq)]
pub enum TransportEvent {
    /// Transport is now connected to its peer.
    Connected,
    /// Transport has disconnected and should not be used until reinitialized.
    Disconnected,
    /// An OSC bundle is ready for processing.
    Message(OscBundle),
}

/// Common interface for transports.
pub trait Transport {
    /// Sends a single OSC message.
    fn send(&mut self, message: OscMessage) -> Result<()>;

    /// Receives the next pending event, if any.
    fn poll_event(&mut self) -> Option<TransportEvent>;
}

/// In-memory channel transport useful for tests and local simulations.
///
/// This lightweight transport is intentionally synchronous and allocates minimal
/// resources, making it ideal for unit tests or deterministic playback.
#[derive(Default)]
pub struct LocalChannel {
    inbox: VecDeque<TransportEvent>,
}

impl LocalChannel {
    /// Creates a connected channel with no queued messages.
    ///
    /// # Examples
    ///
    /// ```
    /// use kitu_transport::{LocalChannel, Transport, TransportEvent};
    ///
    /// let mut channel = LocalChannel::connected();
    /// assert_eq!(channel.poll_event(), Some(TransportEvent::Connected));
    /// ```
    pub fn connected() -> Self {
        let mut channel = Self::default();
        channel.inbox.push_back(TransportEvent::Connected);
        channel
    }
}

impl Transport for LocalChannel {
    fn send(&mut self, message: OscMessage) -> Result<()> {
        let mut bundle = OscBundle::new();
        bundle.push(message);
        self.inbox.push_back(TransportEvent::Message(bundle));
        Ok(())
    }

    fn poll_event(&mut self) -> Option<TransportEvent> {
        self.inbox.pop_front()
    }
}

/// Validates that transports transition to disconnected state.
///
/// The current placeholder always returns [`KituError::NotImplemented`], but the
/// helper documents the expected shape of a graceful shutdown.
///
/// # Examples
///
/// ```
/// use kitu_core::KituError;
/// use kitu_transport::{disconnect, LocalChannel};
///
/// let mut channel = LocalChannel::default();
/// let result = disconnect(&mut channel);
/// assert!(matches!(result, Err(KituError::NotImplemented(_))));
/// ```
pub fn disconnect<T: Transport>(transport: &mut T) -> Result<()> {
    let _ = transport;
    Err(KituError::NotImplemented("disconnect".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_channel_reports_connection_then_messages() {
        let mut channel = LocalChannel::connected();
        assert_eq!(channel.poll_event(), Some(TransportEvent::Connected));

        channel
            .send(OscMessage::new("/ping"))
            .expect("send should enqueue message");
        match channel.poll_event() {
            Some(TransportEvent::Message(bundle)) => {
                assert_eq!(bundle.len(), 1);
                assert_eq!(bundle.messages[0].address, "/ping");
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn disconnect_returns_not_implemented_error() {
        let mut channel = LocalChannel::default();
        let result = disconnect(&mut channel);
        assert!(matches!(result, Err(KituError::NotImplemented(_))));
    }

    #[test]
    fn kep_envelope_round_trips_messagepack() {
        let envelope = KepEnvelope {
            payload_type: KEP_PAYLOAD_OSC.to_string(),
            route: Some("/room/main".to_string()),
            correlation_id: Some(42),
            flags: Some(0),
            payload: vec![1, 2, 3, 4],
        };

        let encoded = encode_kep_envelope(&envelope).expect("encode envelope");
        let decoded = decode_kep_envelope(&encoded).expect("decode envelope");

        assert_eq!(decoded, envelope);
    }

    #[test]
    fn osc_packet_round_trips_supported_args() {
        let mut message = OscMessage::new("/avatar/pose");
        message.push_arg(OscArg::Float(1.0));
        message.push_arg(OscArg::Float(2.0));
        message.push_arg(OscArg::Float(3.0));
        message.push_arg(OscArg::Str("player".to_string()));
        message.push_arg(OscArg::Bool(true));
        message.push_arg(OscArg::Int64(1001));

        let encoded = encode_osc_packet(&message).expect("encode OSC packet");
        let decoded = decode_osc_packet(&encoded).expect("decode OSC packet");

        assert_eq!(decoded, message);
    }

    #[test]
    fn osc_bundle_round_trips_messages() {
        let mut first = OscMessage::new("/input/move");
        first.push_arg(OscArg::Str("player:local".to_string()));
        first.push_arg(OscArg::Float(1.0));

        let mut second = OscMessage::new("/input/jump");
        second.push_arg(OscArg::Bool(true));

        let mut bundle = OscBundle::new();
        bundle.push(first);
        bundle.push(second);

        let encoded = encode_osc_bundle(&bundle).expect("encode OSC bundle");
        assert_eq!(&encoded[..8], OSC_BUNDLE_HEADER);
        assert_eq!(
            u64::from_be_bytes(encoded[8..16].try_into().expect("timetag bytes")),
            OSC_IMMEDIATE_TIMETAG
        );

        let decoded = decode_osc_bundle(&encoded).expect("decode OSC bundle");

        assert_eq!(decoded, bundle);
    }

    #[test]
    fn osc_bundle_rejects_truncated_element() {
        let mut encoded = Vec::new();
        encoded.extend_from_slice(OSC_BUNDLE_HEADER);
        encoded.extend_from_slice(&OSC_IMMEDIATE_TIMETAG.to_be_bytes());
        encoded.extend_from_slice(&64_i32.to_be_bytes());
        encoded.extend_from_slice(b"/a\0\0");

        let err = decode_osc_bundle(&encoded).expect_err("truncated element should fail");

        assert!(err.to_string().contains("bundle element is truncated"));
    }

    #[test]
    fn osc_bundle_rejects_nested_bundle_elements() {
        let nested = encode_osc_bundle(&OscBundle::new()).expect("encode nested bundle");
        let mut encoded = Vec::new();
        encoded.extend_from_slice(OSC_BUNDLE_HEADER);
        encoded.extend_from_slice(&OSC_IMMEDIATE_TIMETAG.to_be_bytes());
        encoded.extend_from_slice(&(nested.len() as i32).to_be_bytes());
        encoded.extend_from_slice(&nested);

        let err = decode_osc_bundle(&encoded).expect_err("nested bundle should fail");

        assert!(err
            .to_string()
            .contains("nested OSC bundles are not supported"));
    }

    #[test]
    fn osc_bundle_rejects_non_immediate_timetag() {
        let mut encoded = Vec::new();
        encoded.extend_from_slice(OSC_BUNDLE_HEADER);
        encoded.extend_from_slice(&2_u64.to_be_bytes());

        let err = decode_osc_bundle(&encoded).expect_err("non-immediate timetag should fail");

        assert!(err
            .to_string()
            .contains("non-immediate OSC bundle timetags are not supported"));
    }

    #[test]
    fn osc_bundle_rejects_trailing_bytes_inside_element() {
        let message = encode_osc_packet(&OscMessage::new("/tick")).expect("encode OSC packet");
        let mut element = message.clone();
        element.extend_from_slice(&[0, 0, 0, 0]);

        let mut encoded = Vec::new();
        encoded.extend_from_slice(OSC_BUNDLE_HEADER);
        encoded.extend_from_slice(&OSC_IMMEDIATE_TIMETAG.to_be_bytes());
        encoded.extend_from_slice(&(element.len() as i32).to_be_bytes());
        encoded.extend_from_slice(&element);

        let err = decode_osc_bundle(&encoded).expect_err("trailing bytes should fail");

        assert!(err.to_string().contains("OSC packet has trailing bytes"));
    }

    #[test]
    fn osc_bundle_rejects_negative_element_size() {
        let mut encoded = Vec::new();
        encoded.extend_from_slice(OSC_BUNDLE_HEADER);
        encoded.extend_from_slice(&OSC_IMMEDIATE_TIMETAG.to_be_bytes());
        encoded.extend_from_slice(&(-1_i32).to_be_bytes());

        let err = decode_osc_bundle(&encoded).expect_err("negative element size should fail");

        assert!(err.to_string().contains("bundle element size is negative"));
    }
}
