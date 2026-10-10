//! High-Performance QUIC & WebTransport Media Carrier Subsystem
//!
//! Provides ultra-low latency UDP multiplexed bi-directional audio streaming
//! with 0-RTT connection resumption, independent sub-streams for ASR/TTS,
//! and zero head-of-line blocking.

use std::net::SocketAddr;
use std::sync::Arc;
use quinn::{Endpoint, ServerConfig};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tracing::{error, info, warn};

use crate::agent::pipeline::VoicePipelineEngine;

/// Generate self-signed TLS certificates for development/local carrier usage
pub fn generate_self_signed_cert() -> Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>), Box<dyn std::error::Error + Send + Sync>> {
    let key_pair = rcgen::KeyPair::generate()?;
    let subject_alt_names = vec!["localhost".to_string(), "127.0.0.1".to_string(), "0.0.0.0".to_string()];
    let mut params = rcgen::CertificateParams::new(subject_alt_names)?;
    params.distinguished_name.push(rcgen::DnType::CommonName, "OmniVoiceCarrier");
    
    let cert = params.self_signed(&key_pair)?;
    let cert_der = cert.der().to_vec();
    let key_der = key_pair.serialize_der();
    
    let cert_chain = vec![CertificateDer::from(cert_der)];
    let private_key = PrivateKeyDer::Pkcs8(key_der.into());
    
    Ok((cert_chain, private_key))
}

/// Create a Quinn QUIC server configuration with ALPN protocols for media carrier
pub fn make_quic_server_config() -> Result<ServerConfig, Box<dyn std::error::Error + Send + Sync>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let (cert_chain, private_key) = generate_self_signed_cert()?;
    
    let mut crypto = rustls::ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(cert_chain, private_key)?;
        
    crypto.alpn_protocols = vec![
        b"h3".to_vec(),
        b"omni-voice-v1".to_vec(),
        b"webtransport".to_vec(),
    ];
    
    let quic_config = quinn::crypto::rustls::QuicServerConfig::try_from(crypto)?;
    let mut server_config = ServerConfig::with_crypto(Arc::new(quic_config));
    
    let mut transport = quinn::TransportConfig::default();
    transport.max_idle_timeout(Some(std::time::Duration::from_secs(30).try_into()?));
    transport.keep_alive_interval(Some(std::time::Duration::from_secs(3)));
    transport.send_window(1024 * 1024); // 1MB buffer
    transport.receive_window((1024 * 1024u32).into());
    
    server_config.transport_config(Arc::new(transport));
    Ok(server_config)
}

/// Start the QUIC Voice Carrier listener on UDP port
pub async fn start_quic_voice_carrier(
    bind_addr: SocketAddr,
    pipeline: Arc<VoicePipelineEngine>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let server_config = make_quic_server_config()?;
    let endpoint = Endpoint::server(server_config, bind_addr)?;
    
    info!("🚀 QUIC / WebTransport Voice Carrier listening on UDP {}", bind_addr);
    
    while let Some(incoming) = endpoint.accept().await {
        let pipeline = pipeline.clone();
        tokio::spawn(async move {
            match incoming.await {
                Ok(conn) => {
                    let remote = conn.remote_address();
                    info!("⚡ QUIC voice session connected from {}", remote);
                    if let Err(e) = handle_quic_connection(conn, pipeline).await {
                        warn!("QUIC connection from {} terminated: {}", remote, e);
                    }
                }
                Err(e) => {
                    error!("QUIC incoming handshake failed: {}", e);
                }
            }
        });
    }
    
    Ok(())
}

/// Handle incoming QUIC bi-directional streams (audio uplink + telemetry/audio downlink)
async fn handle_quic_connection(
    conn: quinn::Connection,
    pipeline: Arc<VoicePipelineEngine>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    loop {
        tokio::select! {
            // Accept bi-directional audio turn stream
            stream = conn.accept_bi() => {
                match stream {
                    Ok((mut send, mut recv)) => {
                        let pipeline = pipeline.clone();
                        tokio::spawn(async move {
                            // Read full audio chunk from stream
                            match recv.read_to_end(10 * 1024 * 1024).await {
                                Ok(audio_bytes) => {
                                    info!("📦 QUIC Stream received {} audio bytes", audio_bytes.len());
                                    // Process voice turn via pipeline
                                    match pipeline.process_voice_turn(&audio_bytes).await {
                                        Ok((result, raw_audio)) => {
                                            // Write JSON metadata + response audio
                                            let json_meta = serde_json::to_vec(&result).unwrap_or_default();
                                            let meta_len = (json_meta.len() as u32).to_be_bytes();
                                            
                                            let _ = send.write_all(&meta_len).await;
                                            let _ = send.write_all(&json_meta).await;
                                            if !raw_audio.is_empty() {
                                                let _ = send.write_all(&raw_audio).await;
                                            }
                                            let _ = send.finish();
                                        }
                                        Err(e) => {
                                            let err_msg = serde_json::json!({"error": e.to_string()}).to_string();
                                            let _ = send.write_all(err_msg.as_bytes()).await;
                                            let _ = send.finish();
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("QUIC stream read failed: {}", e);
                                }
                            }
                        });
                    }
                    Err(e) => {
                        return Err(Box::new(e));
                    }
                }
            }
            // Handle datagrams for 0-latency live telemetry & VAD states
            datagram = conn.read_datagram() => {
                match datagram {
                    Ok(d) => {
                        // Immediate unacknowledged telemetry datagram
                        if let Ok(text) = std::str::from_utf8(&d) {
                            if text == "PING" {
                                let _ = conn.send_datagram(bytes::Bytes::from_static(b"PONG"));
                            }
                        }
                    }
                    Err(e) => {
                        return Err(Box::new(e));
                    }
                }
            }
        }
    }
}
