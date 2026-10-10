# Omnichannel Ingress Guide: PSTN / Telephony & WhatsApp

This guide details how to connect telephony carriers (SIP / PSTN) and WhatsApp Cloud API directly to the Rust Carrier.

---

## 1. Architecture Overview

```
                      ┌──────────────────────┐
                      │  PSTN / Telephony    │
                      │  (Twilio/Telnyx/SIP) │
                      └──────────┬───────────┘
                                 │ Bi-directional G.711 mu-law (8kHz)
                                 │ Base64 over WebSocket
                                 ▼
┌──────────────────┐  ws://host:8090/api/telephony/media-stream  ┌─────────────────────────┐
│  WhatsApp Cloud  ├─────────────────────────────────────────────►│       Rust Carrier      │
│  API (Meta)      │  HTTP Webhook / Graph API Dispatch           │                         │
└──────────────────┘  http://host:8090/api/whatsapp/webhook      │  • SIMD DSP (G.711↔PCM) │
                                                                 │  • VAD & Turn Engine    │
                                                                 │  • Sarvam / 11Labs / Jev│
                                                                 └─────────────────────────┘
```

---

## 2. Telephony / PSTN Integration (Twilio / Telnyx / FreeSWITCH)

Telephony carriers transmit voice over G.711 $\mu$-law (PCMU) at 8,000 Hz. The carrier bridges this directly to the speech pipeline via zero-allocation SIMD transcoding.

### TwiML / SIP Gateway Configuration

Point your carrier's inbound webhook URL to:
```http
POST https://<your-carrier-domain>:8090/api/telephony/inbound-call
```

The endpoint returns dynamic TwiML instructing the carrier to start bi-directional streaming:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<Response>
    <Say>Connecting you to the intelligent omnichannel agent.</Say>
    <Connect>
        <Stream url="wss://<your-carrier-domain>:8090/api/telephony/media-stream" />
    </Connect>
</Response>
```

### Direct WebSocket Media Stream
- **URL**: `ws://<your-carrier-domain>:8090/api/telephony/media-stream`
- **Supported Events**:
  - `start`: Initializes call metadata and sets session ID.
  - `media`: Transmits base64-encoded G.711 $\mu$-law chunks.
  - `stop`: Gracefully terminates the session and frees resources.

---

## 3. WhatsApp Business Cloud API Integration

### Webhook Verification (Handshake)
Configure your Meta App Developer Portal Webhook Callback:
- **Callback URL**: `https://<your-carrier-domain>:8090/api/whatsapp/webhook`
- **Verify Token**: Configured via `WHATSAPP_VERIFY_TOKEN` (defaults to `voice_agent_verify_token_2026`).

When Meta sends the verification handshake (`hub.challenge`), the Rust Carrier automatically validates the token and returns the challenge string with HTTP 200.

### Inbound Events & Audio Notes
When a user sends a WhatsApp voice note or message:
- Meta sends a POST request to `/api/whatsapp/webhook`.
- The carrier extracts the audio media ID, fetches the media stream via Graph API, and routes it directly into the speech transcription and LLM inference pipeline.

### Outbound Voice Dispatch
To trigger an automated voice call or voice note back to a WhatsApp user:
```bash
curl -X POST http://localhost:8090/api/whatsapp/dispatch-voice \
  -H "Content-Type: application/json" \
  -d '{
    "to": "+1234567890",
    "text": "Hello! Your appointment has been confirmed for tomorrow at 10 AM.",
    "voice_id": "sarvam-hindi-male"
  }'
```
