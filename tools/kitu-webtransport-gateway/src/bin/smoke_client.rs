use std::{env, time::Duration};

use anyhow::{Context, Result};
use kitu_osc_ir::{OscArg, OscMessage};
use kitu_transport::{
    decode_kep_envelope, encode_kep_envelope, encode_osc_packet, KepEnvelope, KEP_PAYLOAD_JSON,
};
use wtransport::{tls::Sha256Digest, ClientConfig, Endpoint};

const DEFAULT_URL: &str = "https://webtransport-gateway:9443";
const DEFAULT_ROUTE: &str = "/room/main";
const DEFAULT_OBJECT_ID: &str = "webtransport-smoke";
const KEP_ROUTE_DATAGRAM_PROBE: &str = "/gateway/datagram/probe";
const KEP_ROUTE_DATAGRAM_ACK: &str = "/gateway/datagram/ack";

#[tokio::main]
async fn main() -> Result<()> {
    let url = env::var("KITU_WT_SMOKE_URL").unwrap_or_else(|_| DEFAULT_URL.to_string());
    let cert_hash = env::var("PUBLIC_KITU_ADMIN_WT_CERT_SHA256")
        .or_else(|_| env::var("KITU_WT_SMOKE_CERT_SHA256"))
        .context("PUBLIC_KITU_ADMIN_WT_CERT_SHA256 or KITU_WT_SMOKE_CERT_SHA256 is required")?;
    let route = env::var("KITU_WT_SMOKE_ROUTE").unwrap_or_else(|_| DEFAULT_ROUTE.to_string());
    let object_id =
        env::var("KITU_WT_SMOKE_OBJECT_ID").unwrap_or_else(|_| DEFAULT_OBJECT_ID.to_string());

    let client_config = ClientConfig::builder()
        .with_bind_default()
        .with_server_certificate_hashes([parse_sha256_digest(&cert_hash)?])
        .keep_alive_interval(Some(Duration::from_secs(2)))
        .build();
    let endpoint = Endpoint::client(client_config)?;
    let connection = endpoint
        .connect(url.as_str())
        .await
        .with_context(|| format!("connect WebTransport {url}"))?;

    let mut message = OscMessage::new("/admin/world/spawn");
    message.push_arg(OscArg::Str(object_id.clone()));
    message.push_arg(OscArg::Float(1.0));
    message.push_arg(OscArg::Float(2.0));
    message.push_arg(OscArg::Float(3.0));

    let osc_packet = encode_osc_packet(&message).context("encode OSC packet")?;
    let mut envelope = KepEnvelope::osc(osc_packet);
    envelope.route = Some(route);
    envelope.flags = Some(0);
    let bytes = encode_kep_envelope(&envelope).context("encode KEP envelope")?;

    let (mut send_stream, mut recv_stream) = connection.open_bi().await?.await?;
    send_stream.write_all(&bytes).await?;
    send_stream.finish().await?;
    let mut response = Vec::new();
    let mut chunk = [0; 4096];
    while let Some(read) = recv_stream.read(&mut chunk).await? {
        response.extend_from_slice(&chunk[..read]);
    }

    anyhow::ensure!(
        !response.is_empty(),
        "expected WebTransport KEP response envelope"
    );
    let response_envelope = decode_kep_envelope(&response).context("decode KEP response")?;
    anyhow::ensure!(
        response_envelope.payload_type == KEP_PAYLOAD_JSON,
        "expected JSON KEP response, got {}",
        response_envelope.payload_type
    );
    let response_json: serde_json::Value =
        serde_json::from_slice(&response_envelope.payload).context("decode KEP response JSON")?;
    anyhow::ensure!(
        response_json.get("type").is_some(),
        "expected server event JSON response"
    );
    send_datagram_probe(&connection).await?;
    connection.close(0u32.into(), b"smoke complete");
    endpoint.wait_idle().await;

    println!(
        "sent WebTransport KEP smoke OSC /admin/world/spawn for {object_id}; received KEP {} response and datagram ack",
        response_envelope.payload_type
    );
    Ok(())
}

async fn send_datagram_probe(connection: &wtransport::Connection) -> Result<()> {
    let mut envelope = KepEnvelope::json(br#"{"type":"webTransportDatagramProbe"}"#.to_vec());
    envelope.route = Some(KEP_ROUTE_DATAGRAM_PROBE.to_string());
    envelope.correlation_id = Some(1);
    envelope.flags = Some(0);
    let bytes = encode_kep_envelope(&envelope).context("encode KEP datagram probe")?;
    if let Some(max_datagram_size) = connection.max_datagram_size() {
        anyhow::ensure!(
            bytes.len() <= max_datagram_size,
            "KEP datagram probe is {} bytes, larger than peer limit {}",
            bytes.len(),
            max_datagram_size
        );
    }

    connection
        .send_datagram(&bytes)
        .context("send KEP datagram probe")?;

    let datagram = tokio::time::timeout(Duration::from_secs(2), connection.receive_datagram())
        .await
        .context("timeout waiting for KEP datagram ack")?
        .context("receive KEP datagram ack")?;
    let ack = decode_kep_envelope(&datagram.payload()).context("decode KEP datagram ack")?;
    anyhow::ensure!(
        ack.payload_type == KEP_PAYLOAD_JSON,
        "expected JSON KEP datagram ack, got {}",
        ack.payload_type
    );
    anyhow::ensure!(
        ack.route.as_deref() == Some(KEP_ROUTE_DATAGRAM_ACK),
        "expected KEP datagram ack route {}, got {:?}",
        KEP_ROUTE_DATAGRAM_ACK,
        ack.route
    );
    anyhow::ensure!(
        ack.correlation_id == Some(1),
        "expected KEP datagram ack correlation id 1, got {:?}",
        ack.correlation_id
    );
    let payload: serde_json::Value =
        serde_json::from_slice(&ack.payload).context("decode KEP datagram ack JSON")?;
    anyhow::ensure!(
        payload.get("type").and_then(serde_json::Value::as_str) == Some("webTransportDatagramAck"),
        "expected webTransportDatagramAck payload, got {payload}"
    );
    Ok(())
}

fn parse_sha256_digest(value: &str) -> Result<Sha256Digest> {
    let normalized = value.replace(|character: char| !character.is_ascii_hexdigit(), "");
    anyhow::ensure!(
        normalized.len() == 64,
        "certificate SHA-256 hash must be 64 hex chars"
    );

    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        let start = index * 2;
        *byte = u8::from_str_radix(&normalized[start..start + 2], 16)
            .context("parse certificate SHA-256 hash")?;
    }
    Ok(Sha256Digest::new(bytes))
}
