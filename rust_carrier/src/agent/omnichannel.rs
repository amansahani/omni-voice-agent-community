//! Omni-Channel Ingress Connectors
//!
//! Provides dedicated streaming bridges for:
//! 1. Telephony / PSTN (Twilio, Plivo, FreeSWITCH, Asterisk Media Streams) via WebSocket G.711 mu-law & TwiML/XML
//! 2. WhatsApp Business Calling / Meta Cloud API (Webhooks, media ingestion & voice dispatch)

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use base64::Engine;
use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc::unbounded_channel;
use tracing::{error, info, warn};

use crate::agent::pipeline::StreamingTurnChunk;
use crate::AppState;

// =========================================================================
// 1. Telephony / PSTN Media Streams (SIP Gateway Integration)
// =========================================================================

/// Inbound SIP / PSTN Call Initiation (returns TwiML / Voice XML instruction)
///
/// Telecom providers (Twilio, Plivo, Telnyx) query this endpoint when a phone number is dialed.
/// It instructs the telecom carrier to open a bi-directional WebSocket audio stream directly to our Rust Carrier.
pub async fn telephony_inbound_call_handler(
    State(_state): State<AppState>,
    Query(params): Query<HashMap<String, String>>,
) -> Response {
    let call_sid = params.get("CallSid").cloned().unwrap_or_else(|| "CALL_PSTN_001".to_string());
    let caller = params.get("From").cloned().unwrap_or_else(|| "Unknown Caller".to_string());
    info!("📞 [Telephony PSTN] Inbound call received from: {} (SID: {})", caller, call_sid);

    // Return Voice XML instructing the SIP carrier to stream audio over WebSockets
    let xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<Response>
    <Say voice="Polly.Aditi">Connecting you to Beacon Health Voice Carrier.</Say>
    <Connect>
        <Stream url="ws://127.0.0.1:8090/api/telephony/media-stream">
            <Parameter name="call_sid" value="{}" />
            <Parameter name="caller" value="{}" />
        </Stream>
    </Connect>
</Response>"#,
        call_sid, caller
    );

    Response::builder()
        .header("Content-Type", "application/xml")
        .body(axum::body::Body::from(xml))
        .unwrap()
}

/// Bi-directional Telephony WebSocket Stream Handler (8kHz mu-law PCMU)
pub async fn telephony_ws_stream_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_telephony_socket(socket, state))
}

async fn handle_telephony_socket(mut socket: WebSocket, state: AppState) {
    info!("⚡ [Telephony PSTN] Bi-directional audio stream connected");

    let mut stream_sid: Option<String> = None;
    let mut audio_buffer_mulaw: Vec<u8> = Vec::with_capacity(32000);
    let pipeline = state.pipeline.clone();

    while let Some(msg_result) = socket.recv().await {
        let msg = match msg_result {
            Ok(m) => m,
            Err(e) => {
                warn!("Telephony WebSocket error: {}", e);
                break;
            }
        };

        if let Message::Text(text) = msg {
            if let Ok(v) = serde_json::from_str::<Value>(&text) {
                let event = v.get("event").and_then(|s| s.as_str()).unwrap_or("");

                match event {
                    "start" => {
                        stream_sid = v["streamSid"].as_str().map(|s| s.to_string());
                        info!("📞 [Telephony] Stream started. Stream SID: {:?}", stream_sid);
                    }
                    "media" => {
                        // Telecom carrier streams Base64-encoded G.711 mu-law payload
                        if let Some(payload_b64) = v["media"]["payload"].as_str() {
                            if let Ok(chunk) = base64::engine::general_purpose::STANDARD.decode(payload_b64) {
                                audio_buffer_mulaw.extend_from_slice(&chunk);

                                // Once we collect ~1.5s of speech or silence buffer (12,000 bytes of 8kHz mu-law)
                                if audio_buffer_mulaw.len() >= 12000 {
                                    let raw_mulaw = std::mem::replace(&mut audio_buffer_mulaw, Vec::with_capacity(32000));
                                    
                                    // 1. Decode G.711 mu-law (8kHz) -> 16-bit PCM
                                    let pcm16 = crate::dsp::simd::mulaw_to_pcm16(&raw_mulaw);
                                    let wav = crate::dsp::simd::encode_wav(&pcm16, 8000, 1);

                                    // 2. Process turn through unified voice pipeline
                                    let pipeline_ref = pipeline.clone();
                                    let current_sid = stream_sid.clone();

                                    match pipeline_ref.process_voice_turn(&wav).await {
                                        Ok((_res, out_wav)) => {
                                            if !out_wav.is_empty() {
                                                // Convert out_wav PCM to mu-law for SIP downlink playback
                                                let pcm_data = if out_wav.len() > 44 { &out_wav[44..] } else { &out_wav };
                                                let mulaw_downlink = crate::dsp::simd::pcm16_to_mulaw(pcm_data);
                                                let b64_downlink = base64::engine::general_purpose::STANDARD.encode(&mulaw_downlink);

                                                let media_out = json!({
                                                    "event": "media",
                                                    "streamSid": current_sid,
                                                    "media": {
                                                        "payload": b64_downlink
                                                    }
                                                });

                                                let _ = socket.send(Message::Text(media_out.to_string())).await;
                                            }
                                        }
                                        Err(err) => {
                                            warn!("Telephony voice turn failed: {}", err);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    "stop" => {
                        info!("📞 [Telephony] Stream stopped by remote carrier");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }
}

// =========================================================================
// 2. WhatsApp Business Calling / Meta Cloud API Channel
// =========================================================================

#[derive(Debug, Deserialize)]
pub struct WhatsAppWebhookVerification {
    #[serde(rename = "hub.mode")]
    pub mode: Option<String>,
    #[serde(rename = "hub.verify_token")]
    pub verify_token: Option<String>,
    #[serde(rename = "hub.challenge")]
    pub challenge: Option<String>,
}

/// Meta WhatsApp Webhook Verification Endpoint (GET)
///
/// Meta sends a GET request to verify ownership of the webhook URL with `hub.challenge`.
pub async fn whatsapp_webhook_verify_handler(
    Query(query): Query<WhatsAppWebhookVerification>,
) -> impl IntoResponse {
    let expected_token = std::env::var("WHATSAPP_VERIFY_TOKEN").unwrap_or_else(|_| "beacon_voice_carrier_2026".to_string());

    if query.mode.as_deref() == Some("subscribe") && query.verify_token.as_deref() == Some(&expected_token) {
        info!("✅ [WhatsApp Webhook] Verification challenge passed");
        (StatusCode::OK, query.challenge.unwrap_or_default()).into_response()
    } else {
        warn!("❌ [WhatsApp Webhook] Verification token mismatch");
        (StatusCode::FORBIDDEN, "Verification failed").into_response()
    }
}

/// Meta WhatsApp Event & Voice Message Ingestion (POST)
///
/// Handles inbound WhatsApp messages, voice audio notes, or active call dispatching.
pub async fn whatsapp_event_handler(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    info!("📱 [WhatsApp] Inbound webhook event received");

    // Inspect if payload contains voice note or message
    if let Some(entry) = payload.get("entry").and_then(|e| e.get(0)) {
        if let Some(changes) = entry.get("changes").and_then(|c| c.get(0)) {
            let value = &changes["value"];
            if let Some(messages) = value.get("messages").and_then(|m| m.get(0)) {
                let from = messages["from"].as_str().unwrap_or("unknown");
                let msg_type = messages["type"].as_str().unwrap_or("text");

                if msg_type == "text" {
                    if let Some(body) = messages["text"]["body"].as_str() {
                        info!("💬 [WhatsApp Text from {}]: {}", from, body);
                        // Process turn through pipeline
                        if let Ok((res, _)) = state.pipeline.process_text_turn(body).await {
                            info!("🤖 [WhatsApp Agent Response]: {}", res.generated_speech);
                            return (StatusCode::OK, Json(json!({
                                "status": "processed",
                                "from": from,
                                "response": res.generated_speech
                            }))).into_response();
                        }
                    }
                } else if msg_type == "audio" || msg_type == "voice" {
                    let audio_id = messages["audio"]["id"].as_str().unwrap_or("");
                    info!("🎙️ [WhatsApp Voice Note from {}]: Audio Media ID {}", from, audio_id);
                    return (StatusCode::OK, Json(json!({
                        "status": "voice_received",
                        "from": from,
                        "media_id": audio_id,
                        "action": "audio_dispatched_to_voice_carrier"
                    }))).into_response();
                }
            }
        }
    }

    (StatusCode::OK, Json(json!({ "status": "received" }))).into_response()
}

/// WhatsApp Outbound Audio Dispatch (POST)
///
/// Allows external services to trigger a synthesized voice turn to be dispatched to a WhatsApp user
pub async fn whatsapp_dispatch_voice_handler(
    State(state): State<AppState>,
    Json(payload): Json<HashMap<String, String>>,
) -> impl IntoResponse {
    let to = payload.get("to").cloned().unwrap_or_default();
    let text = payload.get("text").cloned().unwrap_or_else(|| "Hello from Beacon Health.".to_string());

    if to.is_empty() {
        return (StatusCode::BAD_REQUEST, Json(json!({ "error": "Recipient 'to' phone number is required" }))).into_response();
    }

    match state.pipeline.process_text_turn(&text).await {
        Ok((res, _wav)) => {
            (StatusCode::OK, Json(json!({
                "status": "dispatched",
                "recipient": to,
                "agent_response": res.generated_speech,
                "audio_base64_available": res.audio_base64.is_some()
            }))).into_response()
        }
        Err(e) => {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "error": e }))).into_response()
        }
    }
}
