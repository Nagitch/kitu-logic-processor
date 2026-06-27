use std::{
    collections::VecDeque,
    env,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use futures_util::{SinkExt, Stream, StreamExt};
use kitu_transport::{
    decode_kep_envelope, decode_kep_stream_frames, encode_kep_envelope,
    encode_kep_stream_frame_bytes, KEP_PAYLOAD_JSON, KEP_PAYLOAD_OSC,
};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{info, warn};
use wtransport::{Endpoint, Identity, RecvStream, SendStream, ServerConfig};

const DEFAULT_BIND_PORT: u16 = 9443;
const DEFAULT_INTERNAL_WS_URL: &str = "ws://demo-game:8787/ws";
const MAX_STREAM_BYTES: usize = 64 * 1024;
const MAX_MESSAGES_PER_SECOND: usize = 240;

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
                        handle_connection(connection, config).await;
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

async fn handle_connection(connection: wtransport::Connection, config: GatewayConfig) {
    let mut recent_messages = VecDeque::new();

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

                let config = config.clone();
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
}

async fn handle_stream(
    mut send_stream: SendStream,
    recv_stream: RecvStream,
    config: GatewayConfig,
) -> Result<()> {
    let bytes = read_stream(recv_stream).await?;
    let mut envelopes =
        decode_kep_stream_frames(&bytes).context("decode WebTransport KEP frames")?;
    anyhow::ensure!(
        envelopes.len() == 1,
        "expected exactly one KEP request frame, got {}",
        envelopes.len()
    );
    let envelope = envelopes.pop().expect("frame count checked");
    if envelope.payload_type != KEP_PAYLOAD_OSC {
        anyhow::bail!("unsupported KEP payload type: {}", envelope.payload_type);
    }
    let request_bytes = encode_kep_envelope(&envelope).context("encode internal WebSocket KEP")?;

    for response in relay_kep_envelope(&config.internal_ws_url, request_bytes).await? {
        let frame =
            encode_kep_stream_frame_bytes(&response).context("encode WebTransport KEP frame")?;
        send_stream
            .write_all(&frame)
            .await
            .context("write WebTransport KEP response frame")?;
    }

    send_stream
        .finish()
        .await
        .context("finish WebTransport response stream")?;
    Ok(())
}

async fn read_stream(mut recv_stream: RecvStream) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut chunk = [0; 4096];

    while let Some(read) = recv_stream.read(&mut chunk).await.context("read stream")? {
        bytes.extend_from_slice(&chunk[..read]);
        if bytes.len() > MAX_STREAM_BYTES {
            anyhow::bail!("stream exceeds {} bytes", MAX_STREAM_BYTES);
        }
    }

    Ok(bytes)
}

async fn relay_kep_envelope(internal_ws_url: &str, bytes: Vec<u8>) -> Result<Vec<Vec<u8>>> {
    let (socket, _) = connect_async(internal_ws_url)
        .await
        .with_context(|| format!("connect internal WebSocket {internal_ws_url}"))?;
    let (mut writer, mut reader) = socket.split();

    tokio::time::sleep(Duration::from_millis(50)).await;
    writer
        .send(Message::Binary(bytes.into()))
        .await
        .context("send internal WebSocket KEP binary")?;

    let mut responses = Vec::new();
    match tokio::time::timeout(
        Duration::from_secs(2),
        read_next_json_kep_response(&mut reader),
    )
    .await
    {
        Ok(Some(response)) => responses.push(response?),
        Ok(None) | Err(_) => {}
    }

    while !responses.is_empty() {
        match tokio::time::timeout(
            Duration::from_millis(200),
            read_next_json_kep_response(&mut reader),
        )
        .await
        {
            Ok(Some(response)) => responses.push(response?),
            Ok(None) | Err(_) => break,
        }
    }

    let _ = writer.close().await;
    Ok(responses)
}

async fn read_next_json_kep_response<R>(reader: &mut R) -> Option<Result<Vec<u8>>>
where
    R: Stream<Item = std::result::Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    while let Some(message) = reader.next().await {
        match message.context("read internal WebSocket response") {
            Ok(Message::Binary(bytes)) => {
                let response = bytes.to_vec();
                match decode_kep_envelope(&response)
                    .context("decode internal WebSocket KEP response")
                {
                    Ok(envelope) if envelope.payload_type == KEP_PAYLOAD_JSON => {
                        return Some(Ok(response));
                    }
                    Ok(envelope) => {
                        warn!(
                            payload_type = envelope.payload_type,
                            "ignoring unsupported internal WebSocket KEP response"
                        );
                    }
                    Err(err) => return Some(Err(err)),
                }
            }
            Ok(Message::Close(_)) => return None,
            Ok(_) => {}
            Err(err) => return Some(Err(err)),
        }
    }
    None
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
