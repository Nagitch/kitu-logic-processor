use std::{
    collections::VecDeque,
    env,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use futures_util::{SinkExt, StreamExt};
use kitu_transport::{
    decode_kep_envelope, encode_kep_envelope, KepEnvelope, KEP_PAYLOAD_JSON, KEP_PAYLOAD_OSC,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn};
use wtransport::{Endpoint, Identity, RecvStream, SendStream, ServerConfig};

const DEFAULT_BIND_PORT: u16 = 9443;
const DEFAULT_INTERNAL_WS_URL: &str = "ws://demo-game:8787/ws";
const MAX_STREAM_BYTES: usize = 64 * 1024;
const MAX_DATAGRAM_BYTES: usize = 1200;
const MAX_MESSAGES_PER_SECOND: usize = 240;
const MAX_DATAGRAMS_PER_SECOND: usize = 240;
const KEP_ROUTE_DATAGRAM_PROBE: &str = "/gateway/datagram/probe";
const KEP_ROUTE_DATAGRAM_ACK: &str = "/gateway/datagram/ack";

#[derive(Debug, Clone)]
struct GatewayConfig {
    bind_port: u16,
    internal_ws_url: String,
    cert_path: Option<String>,
    key_path: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    let config = GatewayConfig::from_env()?;
    let identity = load_identity(&config).await?;
    let server_config = ServerConfig::builder()
        .with_bind_default(config.bind_port)
        .with_identity(identity)
        .keep_alive_interval(Some(Duration::from_secs(5)))
        .build();
    let server = Endpoint::server(server_config)?;

    info!(
        "kitu WebTransport gateway listening on https://0.0.0.0:{} and relaying to {}",
        config.bind_port, config.internal_ws_url
    );

    loop {
        let incoming_session = server.accept().await;
        let config = config.clone();
        tokio::spawn(async move {
            match incoming_session.await {
                Ok(request) => match request.accept().await {
                    Ok(connection) => {
                        info!(
                            connection_id = connection.stable_id(),
                            remote = %connection.remote_address(),
                            "accepted WebTransport session"
                        );
                        handle_connection(connection, Arc::new(config)).await;
                    }
                    Err(err) => warn!("failed to accept WebTransport request: {err}"),
                },
                Err(err) => warn!("failed WebTransport session handshake: {err}"),
            }
        });
    }
}

impl GatewayConfig {
    fn from_env() -> Result<Self> {
        let bind_port = env::var("KITU_WT_GATEWAY_PORT")
            .ok()
            .map(|value| value.parse())
            .transpose()
            .context("parse KITU_WT_GATEWAY_PORT")?
            .unwrap_or(DEFAULT_BIND_PORT);

        Ok(Self {
            bind_port,
            internal_ws_url: env::var("KITU_GATEWAY_INTERNAL_WS_URL")
                .unwrap_or_else(|_| DEFAULT_INTERNAL_WS_URL.to_string()),
            cert_path: env::var("KITU_WT_GATEWAY_CERT").ok(),
            key_path: env::var("KITU_WT_GATEWAY_KEY").ok(),
        })
    }
}

async fn load_identity(config: &GatewayConfig) -> Result<Identity> {
    match (&config.cert_path, &config.key_path) {
        (Some(cert_path), Some(key_path)) => Identity::load_pemfiles(cert_path, key_path)
            .await
            .context("load WebTransport TLS identity"),
        (None, None) => {
            warn!(
                "using ephemeral self-signed WebTransport certificate; browsers need a trusted cert or certificate-hash configuration"
            );
            Identity::self_signed(["localhost", "127.0.0.1", "::1"])
                .context("generate self-signed WebTransport identity")
        }
        _ => anyhow::bail!(
            "KITU_WT_GATEWAY_CERT and KITU_WT_GATEWAY_KEY must be configured together"
        ),
    }
}

async fn handle_connection(connection: wtransport::Connection, config: Arc<GatewayConfig>) {
    let mut recent_messages = VecDeque::new();
    let connection_id = connection.stable_id();
    let datagram_connection = connection.clone();
    let datagram_task = tokio::spawn(async move {
        if let Err(err) = handle_datagrams(datagram_connection).await {
            info!(connection_id, "WebTransport datagram loop closed: {err:#}");
        }
    });

    loop {
        match connection.accept_bi().await {
            Ok((send_stream, recv_stream)) => {
                prune_rate_window(&mut recent_messages);
                if recent_messages.len() >= MAX_MESSAGES_PER_SECOND {
                    warn!(
                        connection_id = connection.stable_id(),
                        "dropping WebTransport stream after rate limit"
                    );
                    continue;
                }
                recent_messages.push_back(Instant::now());

                let config = Arc::clone(&config);
                tokio::spawn(async move {
                    if let Err(err) = handle_stream(send_stream, recv_stream, config).await {
                        warn!("WebTransport stream relay failed: {err:#}");
                    }
                });
            }
            Err(err) => {
                info!("WebTransport connection closed: {err}");
                break;
            }
        }
    }

    datagram_task.abort();
}

async fn handle_stream(
    mut send_stream: SendStream,
    recv_stream: RecvStream,
    config: Arc<GatewayConfig>,
) -> Result<()> {
    let bytes = read_stream(recv_stream).await?;
    let envelope = decode_kep_envelope(&bytes).context("decode KEP envelope")?;
    if envelope.payload_type != KEP_PAYLOAD_OSC {
        anyhow::bail!("unsupported KEP payload type: {}", envelope.payload_type);
    }

    if let Some(response) = relay_kep_envelope(&config.internal_ws_url, bytes).await? {
        send_stream
            .write_all(&response)
            .await
            .context("write WebTransport KEP response")?;
    }

    send_stream
        .finish()
        .await
        .context("finish WebTransport response stream")?;
    Ok(())
}

async fn handle_datagrams(connection: wtransport::Connection) -> Result<()> {
    let mut recent_datagrams = VecDeque::new();

    loop {
        let datagram = connection
            .receive_datagram()
            .await
            .context("receive WebTransport datagram")?;
        prune_rate_window(&mut recent_datagrams);
        if recent_datagrams.len() >= MAX_DATAGRAMS_PER_SECOND {
            warn!(
                connection_id = connection.stable_id(),
                "dropping WebTransport datagram after rate limit"
            );
            continue;
        }
        recent_datagrams.push_back(Instant::now());

        let bytes = datagram.payload();
        if bytes.len() > MAX_DATAGRAM_BYTES {
            warn!(
                connection_id = connection.stable_id(),
                bytes = bytes.len(),
                max_bytes = MAX_DATAGRAM_BYTES,
                "dropping oversized WebTransport KEP datagram"
            );
            continue;
        }

        let response = match handle_datagram_payload(bytes.as_ref()) {
            Ok(response) => response,
            Err(err) => {
                warn!(
                    connection_id = connection.stable_id(),
                    "dropping invalid WebTransport KEP datagram: {err:#}"
                );
                continue;
            }
        };

        if let Some(response) = response {
            if let Some(max_datagram_size) = connection.max_datagram_size() {
                if response.len() > max_datagram_size {
                    warn!(
                        connection_id = connection.stable_id(),
                        response_bytes = response.len(),
                        max_datagram_size,
                        "dropping WebTransport datagram ack larger than peer limit"
                    );
                    continue;
                }
            }
            connection
                .send_datagram(&response)
                .context("send WebTransport KEP datagram ack")?;
        }
    }
}

fn handle_datagram_payload(bytes: &[u8]) -> Result<Option<Vec<u8>>> {
    let envelope = decode_kep_envelope(bytes).context("decode KEP datagram envelope")?;
    if envelope.payload_type != KEP_PAYLOAD_JSON {
        anyhow::bail!(
            "unsupported KEP datagram payload type: {}",
            envelope.payload_type
        );
    }

    if envelope.route.as_deref() == Some(KEP_ROUTE_DATAGRAM_PROBE) {
        return Ok(Some(encode_datagram_ack(&envelope, bytes.len())?));
    }

    Ok(None)
}

fn encode_datagram_ack(request: &KepEnvelope, received_bytes: usize) -> Result<Vec<u8>> {
    let payload = serde_json::to_vec(&serde_json::json!({
        "type": "webTransportDatagramAck",
        "receivedRoute": request.route,
        "receivedBytes": received_bytes,
    }))
    .context("encode WebTransport datagram ack JSON")?;
    let mut response = KepEnvelope::json(payload);
    response.route = Some(KEP_ROUTE_DATAGRAM_ACK.to_string());
    response.correlation_id = request.correlation_id;
    response.flags = Some(0);
    encode_kep_envelope(&response).context("encode WebTransport datagram ack KEP envelope")
}

async fn read_stream(mut recv_stream: RecvStream) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];

    while let Some(read) = recv_stream.read(&mut chunk).await.context("read stream")? {
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() > MAX_STREAM_BYTES {
            anyhow::bail!("stream exceeds {MAX_STREAM_BYTES} bytes");
        }
    }

    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use kitu_transport::{encode_kep_envelope, KepEnvelope};

    use super::{
        handle_datagram_payload, KEP_ROUTE_DATAGRAM_ACK, KEP_ROUTE_DATAGRAM_PROBE,
        MAX_DATAGRAM_BYTES,
    };

    #[test]
    fn datagram_probe_returns_json_ack() {
        let mut request = KepEnvelope::json(br#"{"type":"probe"}"#.to_vec());
        request.route = Some(KEP_ROUTE_DATAGRAM_PROBE.to_string());
        request.correlation_id = Some(7);
        let bytes = encode_kep_envelope(&request).expect("encode probe envelope");

        let ack = handle_datagram_payload(&bytes)
            .expect("handle datagram")
            .expect("ack response");
        assert!(ack.len() <= MAX_DATAGRAM_BYTES);

        let envelope = kitu_transport::decode_kep_envelope(&ack).expect("decode ack envelope");
        assert_eq!(envelope.payload_type, kitu_transport::KEP_PAYLOAD_JSON);
        assert_eq!(envelope.route.as_deref(), Some(KEP_ROUTE_DATAGRAM_ACK));
        assert_eq!(envelope.correlation_id, Some(7));

        let json: serde_json::Value =
            serde_json::from_slice(&envelope.payload).expect("decode ack JSON");
        assert_eq!(json["type"], "webTransportDatagramAck");
        assert_eq!(json["receivedRoute"], KEP_ROUTE_DATAGRAM_PROBE);
    }

    #[test]
    fn datagram_json_on_non_probe_route_is_receive_only() {
        let mut request = KepEnvelope::json(br#"{"type":"telemetry"}"#.to_vec());
        request.route = Some("/client/telemetry".to_string());
        let bytes = encode_kep_envelope(&request).expect("encode telemetry envelope");

        let ack = handle_datagram_payload(&bytes).expect("handle datagram");

        assert!(ack.is_none());
    }

    #[test]
    fn datagram_rejects_reliable_osc_commands() {
        let mut request = KepEnvelope::osc(vec![0, 1, 2, 3]);
        request.route = Some("/room/main".to_string());
        let bytes = encode_kep_envelope(&request).expect("encode OSC envelope");

        let err = handle_datagram_payload(&bytes).expect_err("OSC commands stay reliable");

        assert!(err
            .to_string()
            .contains("unsupported KEP datagram payload type"));
    }
}

async fn relay_kep_envelope(internal_ws_url: &str, bytes: Vec<u8>) -> Result<Option<Vec<u8>>> {
    let (socket, _) = connect_async(internal_ws_url)
        .await
        .with_context(|| format!("connect internal WebSocket {internal_ws_url}"))?;
    let (mut writer, mut reader) = socket.split();

    tokio::time::sleep(Duration::from_millis(50)).await;
    writer
        .send(Message::Binary(bytes.into()))
        .await
        .context("send internal WebSocket KEP binary")?;

    let response = match tokio::time::timeout(Duration::from_secs(2), async {
        while let Some(message) = reader.next().await {
            match message.context("read internal WebSocket response")? {
                Message::Binary(bytes) => {
                    let response = bytes.to_vec();
                    let envelope = decode_kep_envelope(&response)
                        .context("decode internal WebSocket KEP response")?;
                    if envelope.payload_type == KEP_PAYLOAD_JSON {
                        return Ok::<Option<Vec<u8>>, anyhow::Error>(Some(response));
                    }
                    warn!(
                        payload_type = envelope.payload_type,
                        "ignoring unsupported internal WebSocket KEP response"
                    );
                }
                Message::Close(_) => return Ok::<Option<Vec<u8>>, anyhow::Error>(None),
                _ => {}
            }
        }
        Ok::<Option<Vec<u8>>, anyhow::Error>(None)
    })
    .await
    {
        Ok(result) => result?,
        Err(_) => None,
    };

    if response.is_some() {
        let _ = tokio::time::timeout(Duration::from_millis(200), async {
            while reader.next().await.is_some() {}
        })
        .await;
    }

    let _ = writer.close().await;
    Ok(response)
}

fn prune_rate_window(recent_messages: &mut VecDeque<Instant>) {
    let cutoff = Instant::now() - Duration::from_secs(1);
    while recent_messages
        .front()
        .is_some_and(|timestamp| *timestamp < cutoff)
    {
        recent_messages.pop_front();
    }
}
