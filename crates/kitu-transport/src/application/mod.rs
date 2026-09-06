//! Bounded application frames shared by JSON, MessagePack, and native adapters.
//!
//! The codec checks structural validity and preserves typed, ordered OSC batches.
//! Hosts own identity, permissions, compatibility requirements and clock ordering.
//! A WebSocket message is one complete frame; never publish an encoded prefix.
//!
//! ```
//! use kitu_transport::application::{Codec, Encoding, InputFrame, NETWORK_INPUT_LIMITS};
//! use kitu_transport::wire::WireBundle;
//! let codec = Codec::new(Encoding::MessagePack, NETWORK_INPUT_LIMITS)?;
//! let input = InputFrame { metadata: None, bundle: WireBundle::default() };
//! let bytes = codec.encode_input(&input)?;
//! assert_eq!(codec.decode_input(&bytes)?, input);
//! # Ok::<(), kitu_transport::application::CodecError>(())
//! ```
mod dto;
mod preflight;
pub use dto::*;

use crate::wire::{self, WireArg, WireBundle};
use serde::{de::DeserializeOwned, Serialize};
use std::io::{self, Write};

/// Encoding fixed for a connection; decoding never switches it implicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Json,
    MessagePack,
}
/// Inclusive frame and structural bounds, checked before deserialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Limits {
    pub max_bytes: usize,
    pub max_depth: usize,
    pub max_nodes: usize,
    pub max_collection_len: usize,
}
/// Network input profile: 128 KiB and a bounded structural parse.
pub const NETWORK_INPUT_LIMITS: Limits = Limits {
    max_bytes: 128 * 1024,
    max_depth: 32,
    max_nodes: 16_384,
    max_collection_len: 8_192,
};
/// Network output profile: 8 MiB including the entire frame envelope.
pub const NETWORK_OUTPUT_LIMITS: Limits = Limits {
    max_bytes: 8 * 1024 * 1024,
    max_depth: 32,
    max_nodes: 262_144,
    max_collection_len: 65_536,
};
/// Existing generic native request domain remains 1 MiB, including legacy identity omission.
pub const NATIVE_INPUT_LIMITS: Limits = Limits {
    max_bytes: 1024 * 1024,
    max_depth: 32,
    max_nodes: 1024 * 1024,
    max_collection_len: 1024 * 1024,
};
/// Stable structural error categories, separate from gameplay operation results.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecErrorKind {
    Limit,
    Malformed,
    UnknownField,
    InvalidValue,
    TrailingData,
}
/// A bounded, displayable diagnostic with a stable category.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct CodecError {
    pub kind: CodecErrorKind,
    pub message: String,
}
impl CodecError {
    pub(super) fn new(kind: CodecErrorKind, message: impl AsRef<str>) -> Self {
        let message = message.as_ref();
        let mut end = message.len().min(1024);
        while !message.is_char_boundary(end) {
            end -= 1;
        }
        Self {
            kind,
            message: message[..end].into(),
        }
    }
    fn serde(error: impl std::fmt::Display) -> Self {
        let message = error.to_string();
        let kind = if message.contains("unknown field") {
            CodecErrorKind::UnknownField
        } else {
            CodecErrorKind::Malformed
        };
        Self::new(kind, message)
    }
}
/// An invalid or incompatible advertised application contract.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct CompatibilityError(String);
/// Requires exact identity/versions/rate and all required capabilities.
///
/// Extra offered features do not authorize a host to use them without agreement.
/// Both lists must already be sorted and unique.
/// ```
/// use kitu_transport::application::{Compatibility, check_compatibility};
/// let required = Compatibility { app_id: "example".into(), wire_version: 1,
///     schema_version: 1, presentation_version: 1, tick_rate: 60,
///     features: vec!["typed-osc".into()] };
/// check_compatibility(&required, &required)?;
/// # Ok::<(), kitu_transport::application::CompatibilityError>(())
/// ```
pub fn check_compatibility(
    required: &Compatibility,
    offered: &Compatibility,
) -> Result<(), CompatibilityError> {
    validate_compatibility(required)
        .and_then(|()| validate_compatibility(offered))
        .map_err(|error| CompatibilityError(error.to_string()))?;
    if required.app_id != offered.app_id
        || required.wire_version != offered.wire_version
        || required.schema_version != offered.schema_version
        || required.presentation_version != offered.presentation_version
        || required.tick_rate != offered.tick_rate
    {
        return Err(CompatibilityError(
            "incompatible application, versions, or tick rate".into(),
        ));
    }
    if required
        .features
        .iter()
        .any(|feature| offered.features.binary_search(feature).is_err())
    {
        return Err(CompatibilityError(
            "missing required application feature".into(),
        ));
    }
    Ok(())
}

/// One encoding/profile pair; decoding always consumes the complete message.
#[derive(Debug, Clone, Copy)]
pub struct Codec {
    encoding: Encoding,
    limits: Limits,
}
impl Codec {
    /// Creates a validated bounded profile. No storage is allocated up front.
    pub fn new(encoding: Encoding, limits: Limits) -> Result<Self, CodecError> {
        if limits.max_bytes == 0
            || limits.max_bytes > 64 * 1024 * 1024
            || limits.max_depth == 0
            || limits.max_depth > 128
            || limits.max_nodes == 0
            || limits.max_collection_len == 0
            || limits.max_collection_len > limits.max_nodes
        {
            return Err(CodecError::new(
                CodecErrorKind::InvalidValue,
                "invalid codec limits",
            ));
        }
        Ok(Self { encoding, limits })
    }
    /// Decodes a strict client frame without applying host/game permissions.
    pub fn decode_client(&self, bytes: &[u8]) -> Result<ClientFrame, CodecError> {
        let frame = self.decode(bytes)?;
        validate_client(&frame)?;
        Ok(frame)
    }
    /// Decodes a complete server frame including every original bundle.
    pub fn decode_server(&self, bytes: &[u8]) -> Result<ServerFrame, CodecError> {
        let frame = self.decode(bytes)?;
        validate_server(&frame)?;
        Ok(frame)
    }
    /// Encodes one complete validated client frame.
    pub fn encode_client(&self, frame: &ClientFrame) -> Result<Vec<u8>, CodecError> {
        validate_client(frame)?;
        self.encode(frame)
    }
    /// Encodes an owned complete server frame.
    pub fn encode_server(&self, frame: &ServerFrame) -> Result<Vec<u8>, CodecError> {
        validate_server(frame)?;
        self.encode(frame)
    }
    /// Encodes a borrowed batch incrementally with no owned OSC/string clone.
    pub fn encode_server_ref(&self, frame: &ServerFrameRef<'_>) -> Result<Vec<u8>, CodecError> {
        match &frame.frame {
            ServerPayloadRef::Hello(hello) => validate_hello(hello)?,
            ServerPayloadRef::Output(output) => {
                validate_status_tick(output.status, output.batch.tick)?
            }
            ServerPayloadRef::Snapshot(snapshot) => {
                validate_status_tick(snapshot.status, snapshot.batch.tick)?
            }
            ServerPayloadRef::Replay(status) => validate_status(status)?,
            ServerPayloadRef::Error(error) => validate_error(error)?,
        }
        // WireBundlesRef validates OSC values during the bounded write, stopping
        // before later values when the byte budget is already exhausted.
        self.encode(frame)
    }
    /// Decodes the native-compatible bare input payload; metadata may be omitted.
    pub fn decode_input(&self, bytes: &[u8]) -> Result<InputFrame, CodecError> {
        let input = self.decode(bytes)?;
        validate_input(&input)?;
        Ok(input)
    }
    /// Encodes the same payload used inside ClientFrame::Input and by the C ABI.
    pub fn encode_input(&self, input: &InputFrame) -> Result<Vec<u8>, CodecError> {
        validate_input(input)?;
        self.encode(input)
    }
    fn decode<T: DeserializeOwned>(&self, bytes: &[u8]) -> Result<T, CodecError> {
        preflight::check(bytes, self.encoding, self.limits)?;
        match self.encoding {
            Encoding::Json => serde_json::from_slice(bytes).map_err(CodecError::serde),
            Encoding::MessagePack => rmp_serde::from_slice(bytes).map_err(CodecError::serde),
        }
    }
    fn encode<T: Serialize>(&self, value: &T) -> Result<Vec<u8>, CodecError> {
        let mut writer = LimitedWriter {
            bytes: Vec::new(),
            maximum: self.limits.max_bytes,
            exceeded: false,
        };
        let result = match self.encoding {
            Encoding::Json => serde_json::to_writer(&mut writer, value).map_err(CodecError::serde),
            Encoding::MessagePack => value
                .serialize(&mut rmp_serde::Serializer::new(&mut writer).with_struct_map())
                .map_err(CodecError::serde),
        };
        if writer.exceeded {
            return Err(CodecError::new(
                CodecErrorKind::Limit,
                "encoded frame exceeds byte limit",
            ));
        }
        result?;
        preflight::check(&writer.bytes, self.encoding, self.limits)?;
        Ok(writer.bytes)
    }
}

fn invalid(message: &str) -> CodecError {
    CodecError::new(CodecErrorKind::InvalidValue, message)
}
fn bounded_text(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.contains('\0')
}
fn validate_compatibility(value: &Compatibility) -> Result<(), CodecError> {
    if !bounded_text(&value.app_id, 128)
        || value.wire_version == 0
        || value.schema_version == 0
        || value.presentation_version == 0
        || value.tick_rate == 0
        || value.features.len() > 16
        || value.features.windows(2).any(|pair| pair[0] >= pair[1])
        || value.features.iter().any(|feature| {
            feature.is_empty()
                || feature.len() > 64
                || !feature
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
    {
        return Err(invalid(
            "invalid compatibility identity, versions, or feature list",
        ));
    }
    Ok(())
}
fn validate_input(input: &InputFrame) -> Result<(), CodecError> {
    validate_bundle(&input.bundle)
}
fn validate_bundle(bundle: &WireBundle) -> Result<(), CodecError> {
    for message in &bundle.messages {
        wire::validate_address(&message.address).map_err(CodecError::serde)?;
        for arg in &message.args {
            match arg {
                WireArg::Float(value) => wire::validate_float(*value).map_err(CodecError::serde)?,
                WireArg::Str(value) => wire::validate_string(value).map_err(CodecError::serde)?,
                _ => (),
            }
        }
    }
    Ok(())
}
fn validate_client(frame: &ClientFrame) -> Result<(), CodecError> {
    match frame {
        ClientFrame::Input(input) => validate_input(input),
        ClientFrame::Hello(hello) => {
            validate_compatibility(&hello.compatibility)?;
            if !bounded_text(&hello.client_id, 128)
                || hello
                    .expected_session_id
                    .as_ref()
                    .is_some_and(|id| !bounded_text(id, 128))
            {
                return Err(invalid("invalid client or expected session identity"));
            }
            Ok(())
        }
    }
}
fn validate_status(status: &ExecutionStatus) -> Result<(), CodecError> {
    let mode = &status.playback_mode;
    if mode
        .recording_id
        .as_ref()
        .is_some_and(|id| !bounded_text(id, 128))
        || mode
            .error
            .as_ref()
            .is_some_and(|error| error.len() > 1024 || error.contains('\0'))
    {
        return Err(invalid("invalid replay identity or diagnostic"));
    }
    Ok(())
}
fn validate_status_tick(status: &ExecutionStatus, tick: i64) -> Result<(), CodecError> {
    validate_status(status)?;
    if status.playback_mode.tick != tick {
        return Err(invalid("output tick and host status tick differ"));
    }
    Ok(())
}
fn validate_hello(hello: &ServerHello) -> Result<(), CodecError> {
    validate_compatibility(&hello.compatibility)?;
    validate_status(&hello.status)?;
    if !bounded_text(&hello.session_id, 128)
        || !bounded_text(&hello.execution.package, 128)
        || !bounded_text(&hello.execution.target, 128)
        || hello.execution.source_hash.len() != 64
        || !hello
            .execution
            .source_hash
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || hello.limits.max_input_bytes == 0
        || hello.limits.max_output_bytes == 0
    {
        return Err(invalid(
            "invalid server session, execution identity, or limits",
        ));
    }
    Ok(())
}
fn validate_error(error: &ErrorFrame) -> Result<(), CodecError> {
    if !bounded_text(&error.message, 1024) {
        return Err(invalid("invalid error diagnostic"));
    }
    Ok(())
}
fn validate_server(frame: &ServerFrame) -> Result<(), CodecError> {
    let batch = match &frame.frame {
        ServerPayload::Hello(hello) => return validate_hello(hello),
        ServerPayload::Replay(status) => return validate_status(status),
        ServerPayload::Error(error) => return validate_error(error),
        ServerPayload::Output(output) => {
            validate_status_tick(&output.status, output.batch.tick)?;
            &output.batch
        }
        ServerPayload::Snapshot(snapshot) => {
            validate_status_tick(&snapshot.status, snapshot.batch.tick)?;
            &snapshot.batch
        }
    };
    for bundle in &batch.bundles {
        validate_bundle(bundle)?;
    }
    Ok(())
}
struct LimitedWriter {
    bytes: Vec<u8>,
    maximum: usize,
    exceeded: bool,
}
impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let Some(required) = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|n| *n <= self.maximum)
        else {
            self.exceeded = true;
            return Err(io::Error::other("encoded frame exceeds byte limit"));
        };
        if required > self.bytes.capacity() {
            let target = required
                .max(self.bytes.capacity().saturating_mul(2).max(1024))
                .min(self.maximum);
            self.bytes
                .try_reserve_exact(target - self.bytes.len())
                .map_err(io::Error::other)?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
#[cfg(test)]
mod tests;
