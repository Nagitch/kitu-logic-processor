//! Shared logical frames; encodings never infer OSC scalar widths.
use crate::wire::{WireBundle, WireBundlesRef};
use kitu_osc_ir::OscBundle;
use serde::{Deserialize, Serialize};

/// Stable producer identity shared with Runtime and native input admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputMetadata {
    /// Stable across reconnections to the same runtime.
    pub source: String,
    /// Exact producer-scoped unsigned operation identifier.
    pub message_id: u64,
    /// Application contract, independent of encoding.
    pub schema_version: u32,
}
/// Input payload, also accepted without a frame wrapper by the native ABI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputFrame {
    /// Legacy generic native inputs may omit identity; applications may require it.
    pub metadata: Option<InputMetadata>,
    /// Complete immediate bundle, including empty bundles in the generic model.
    pub bundle: WireBundle,
}
/// Application-advertised protocol requirements/capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Compatibility {
    pub app_id: String,
    pub wire_version: u32,
    pub schema_version: u32,
    pub presentation_version: u32,
    pub tick_rate: u32,
    pub features: Vec<String>,
}
/// Observable execution identity; the application owns compatibility policy.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionVersion {
    pub package: String,
    pub source_hash: String,
    pub target: String,
}
/// Requested/granted connection role; granting control belongs to the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Role {
    Controller,
    Observer,
}
/// First client application frame, before any input admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ClientHello {
    pub compatibility: Compatibility,
    pub client_id: String,
    pub role: Role,
    #[serde(deserialize_with = "required_option")]
    pub expected_session_id: Option<String>,
}
/// Client wire 1 has no in-place resynchronization operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum ClientFrame {
    Hello(ClientHello),
    Input(InputFrame),
}
/// Replay status; a seek may move tick backwards without changing delivery order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReplayMode {
    pub active: bool,
    #[serde(deserialize_with = "required_option")]
    pub recording_id: Option<String>,
    /// Last completed tick, or -1 before the first tick.
    pub tick: i64,
    pub total_ticks: u64,
    pub playing: bool,
    pub seeking: bool,
    #[serde(deserialize_with = "required_option")]
    pub error: Option<String>,
}
/// Captured atomically with any associated application output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExecutionStatus {
    pub playback_mode: ReplayMode,
    pub read_only: bool,
}
/// Advertised inclusive complete-message byte limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct WireLimits {
    pub max_input_bytes: u32,
    pub max_output_bytes: u32,
}
/// First server frame after a compatible client greeting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerHello {
    pub compatibility: Compatibility,
    pub execution: ExecutionVersion,
    pub session_id: String,
    pub role: Role,
    pub limits: WireLimits,
    pub status: ExecutionStatus,
}
/// One output transaction, retaining every original bundle boundary.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputBatch {
    pub tick: i64,
    pub bundles: Vec<WireBundle>,
}
/// A complete transaction and its corresponding host status.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OutputFrame {
    pub batch: OutputBatch,
    pub status: ExecutionStatus,
}
/// A snapshot is a projection replacement, not transient event playback.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SnapshotReason {
    Initial,
    Seek,
}
/// Full inspection used on a new connection or verified seek.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SnapshotFrame {
    pub reason: SnapshotReason,
    pub batch: OutputBatch,
    pub status: ExecutionStatus,
}
/// Stable host error classes; codec failures do not imply application acceptance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ErrorCode {
    Incompatible,
    Protocol,
    InvalidInput,
    ReadOnly,
    ControllerBusy,
    QueueFull,
    SessionChanged,
    Internal,
}
/// A bounded diagnostic optionally correlated with a producer operation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ErrorFrame {
    pub code: ErrorCode,
    pub message: String,
    pub fatal: bool,
    #[serde(deserialize_with = "required_option")]
    pub input_id: Option<u64>,
}
/// Exactly one typed server event.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "type",
    content = "payload",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum ServerPayload {
    Hello(ServerHello),
    Output(OutputFrame),
    Snapshot(SnapshotFrame),
    Replay(ExecutionStatus),
    Error(ErrorFrame),
}
/// Socket-local delivery order, independent of game/replay tick and input id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ServerFrame {
    pub delivery_sequence: u64,
    pub frame: ServerPayload,
}

/// Borrowed output view that never clones an OSC batch or its strings.
#[derive(Debug, Clone, Copy)]
pub struct OutputBatchRef<'a> {
    pub tick: i64,
    pub bundles: &'a [OscBundle],
}
impl Serialize for OutputBatchRef<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut object = serializer.serialize_struct("OutputBatch", 2)?;
        object.serialize_field("tick", &self.tick)?;
        object.serialize_field("bundles", &WireBundlesRef::new(self.bundles))?;
        object.end()
    }
}
/// Borrowed counterpart of [`OutputFrame`].
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutputFrameRef<'a> {
    pub batch: OutputBatchRef<'a>,
    pub status: &'a ExecutionStatus,
}
/// Borrowed counterpart of [`SnapshotFrame`].
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotFrameRef<'a> {
    pub reason: SnapshotReason,
    pub batch: OutputBatchRef<'a>,
    pub status: &'a ExecutionStatus,
}
/// Borrowed counterpart of [`ServerPayload`], with identical serialization.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "type", content = "payload", rename_all = "camelCase")]
pub enum ServerPayloadRef<'a> {
    Hello(&'a ServerHello),
    Output(OutputFrameRef<'a>),
    Snapshot(SnapshotFrameRef<'a>),
    Replay(&'a ExecutionStatus),
    Error(&'a ErrorFrame),
}
/// Borrowed counterpart of [`ServerFrame`].
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerFrameRef<'a> {
    pub delivery_sequence: u64,
    pub frame: ServerPayloadRef<'a>,
}

fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
