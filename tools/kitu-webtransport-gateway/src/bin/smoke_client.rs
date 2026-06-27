use std::{env, time::Duration};

use anyhow::{Context, Result};
use kitu_osc_ir::{OscArg, OscMessage};
use kitu_transport::{
    decode_kep_stream_frames, encode_kep_stream_frame, encode_osc_packet, KepEnvelope,
    KEP_PAYLOAD_JSON,
};
use wtransport::{tls::Sha256Digest, ClientConfig, Endpoint};

const DEFAULT_URL: &str = "https://webtransport-gateway:9443";
const DEFAULT_ROUTE: &str = "/room/main";
const DEFAULT_OBJECT_ID: &str = "webtransport-smoke";

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
    let bytes = encode_kep_stream_frame(&envelope).context("encode KEP stream frame")?;

    let (mut send_stream, mut recv_stream) = connection.open_bi().await?.await?;
    send_stream.write_all(&bytes).await?;
    send_stream.finish().await?;
    let mut response = Vec::new();
    let mut chunk = [0; 4096];
    while let Some(read) = recv_stream.read(&mut chunk).await? {
        response.extend_from_slice(&chunk[..read]);
    }

    let response_envelopes =
        decode_kep_stream_frames(&response).context("decode KEP response frames")?;
    anyhow::ensure!(
        response_envelopes.len() >= 2,
        "expected at least two WebTransport KEP response envelopes, got {}",
        response_envelopes.len()
    );
    for response_envelope in &response_envelopes {
        anyhow::ensure!(
            response_envelope.payload_type == KEP_PAYLOAD_JSON,
            "expected JSON KEP response, got {}",
            response_envelope.payload_type
        );
        let response_json: serde_json::Value = serde_json::from_slice(&response_envelope.payload)
            .context("decode KEP response JSON")?;
        anyhow::ensure!(
            response_json.get("type").is_some(),
            "expected server event JSON response"
        );
    }
    connection.close(0u32.into(), b"smoke complete");
    endpoint.wait_idle().await;

    println!(
        "sent WebTransport KEP smoke OSC /admin/world/spawn for {object_id}; received {} KEP JSON responses",
        response_envelopes.len()
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
