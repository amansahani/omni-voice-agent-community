# Beacon Health | Omnichannel Real-Time Voice Agent Architecture

This document details the verified **Production Architecture** of the Beacon Health Real-Time Agentic Voice Carrier and specifies the **Omnichannel Scaling Blueprint** across **PSTN Telephony (SIP)**, **WhatsApp Voice Calling**, and **Native Mobile Applications (iOS / Android)**.

---

## 1. Verified Core Architecture (Local RTX 3060 CUDA Stack)

```mermaid
flowchart TD
    subgraph ClientLayer ["1. Client & Audio DSP Layer"]
        MIC["Microphone Capture (16kHz Mono)"]
        DSP["Web Audio DSP Chain\n(85Hz HPF + Limiter + Gain)"]
        VAD["Silero Neural VAD v5 (WASM)\n32ms Frame Slicing (< 0.1ms)"]
        VIS["Live 60 FPS Visualizer Canvas"]
        SPK["Speaker Output (Muted Feedback Loop)"]
    end

    subgraph CarrierLayer ["2. Go Media Server & Agentic Carrier (Port 8090)"]
        MEDIA["Pion WebRTC v4 Media Engine\n(Zero-Copy SFU Hub)"]
        ROUTER["REST & WebSocket API Gateway\n(/api/voice-turn, /api/text-turn, /ws)"]
        TOOLS["Sub-Microsecond In-Memory Tools (< 1 µs)\n(Availability, Refills, Doctors, Triage, Bills)"]
        WF["Multi-Workflow State Engine\n(7 Flows, Slot Extraction, Auto-Transitions)"]
    end

    subgraph InferenceLayer ["3. Dual-Mode Inference Engine (Local CUDA vs Cloud Indic)"]
        subgraph LocalMode ["🖥️ Local Mode (CUDA)"]
            WHISPER["Whisper.cpp Server (Port 8082)\nggml-large-v3-turbo-q5_0"]
            CLEF["Clef-Flash System One (Port 8085)\nZero-Shot Classifier (-ngl 20)"]
            LLM["llama-server (Port 8080)\nQwen3-4B Q4_K_M"]
            TTS["Kokoro TTS Server (Port 8083)\nPyTorch CUDA TTFB (~95ms)"]
        end
        subgraph CloudMode ["🇮🇳 Sarvam AI + TypeSafe Jev"]
            SARVAM_STT["Sarvam Saaras v3 ASR\n(Multi-Indic & Code-Mixed)"]
            JEV["TypeSafe Jev System One API\n(Cloud Decision Engine ~200ms)"]
            SARVAM_LLM["Sarvam-105B Reasoning\n(Indic Conversational LLM)"]
            SARVAM_TTS["Sarvam Bulbul v3 TTS\n(30+ Indic Voices)"]
        end
    end

    MIC --> DSP --> VAD --> ROUTER
    DSP --> VIS
    ROUTER --> WHISPER --> CLEF --> TOOLS --> WF --> LLM --> TTS --> SPK
    ROUTER -.-> SARVAM_STT -.-> JEV -.-> TOOLS -.-> WF -.-> SARVAM_LLM -.-> SARVAM_TTS -.-> SPK
    MEDIA <--> ROUTER
```

### Component Breakdown
1. **Frontend Audio DSP Chain & Silero Neural VAD v5 (`web/index.html`)**:
   - **Google Meet 85Hz High-Pass Filter**: Cuts room noise and desk fan motor rumble ($20\text{--}80\,\text{Hz}$).
   - **Broadcast Dynamics Compressor & Limiter**: Normalizes sitting distance ($30\text{--}60\,\text{cm}$) to $-18\,\text{dBFS}$.
   - **In-Browser Silero V5 WASM Engine**: Evaluates $32\,\text{ms}$ frames ($512$ samples at $16\,\text{kHz}$) in $<0.1\,\text{ms}$.
   - **Acoustic Echo Isolation**: Muted zero-gain node prevents microphone feedback, and a $350\,\text{ms}$ cooldown blocks room reverberation after assistant playback.
2. **Go Carrier & Media Platform (`main.go`, `agent/voice_pipeline.go`, `agent/workflow.go`)**:
   - **Pion WebRTC v4**: Provides real-time RTP/SRTP transport and signaling.
   - **Sub-Microsecond In-Memory Tool Engine**: Executes domain tools in $0.4\text{--}0.8\,\mu\text{s}$.
   - **Multi-Workflow State Engine**: Manages 7 clinical workflows, transitions, slot extraction, and dynamic system directives.
3. **Dual Decision & Inference Stacks**:
   - **Local CUDA Stack**: `Whisper.cpp` (Port `8082`), `Clef-Flash` System One (Port `8085`), `Qwen3-4B` (Port `8080`), `Kokoro TTS` (Port `8083`).
   - **Sarvam + TypeSafe Jev Stack**: `Sarvam Saaras v3` STT ($<200\,\text{ms}$), `TypeSafe Jev System One API` ($<220\,\text{ms}$ typed judgments), `Sarvam-105B` Conversational Chat LLM, and `Sarvam Bulbul v3` TTS ($30+$ Indic voices).

---

## 2. Omnichannel Integration Architecture

The core Go Carrier and Local Inference Stack remain unified while dedicated **Channel Ingress Adapters** handle signaling, codecs, and protocol translation.

```mermaid
flowchart TD
    subgraph Channels ["Omnichannel Ingress Channels"]
        PSTN["📞 PSTN Telephony (Cellular / Landline)"]
        WAPP["💬 WhatsApp Voice (Meta Cloud Voice API)"]
        MAPP["📱 Mobile Apps (iOS CallKit / Android Telecom)"]
        WEB["💻 Browser WebRTC Client (Desktop/Mobile Web)"]
    end

    subgraph Gateways ["Ingress Gateways & Session Border Controllers"]
        SIP_GW["SIP SBC / FreeSWITCH / Kamailio\n(G.711 / G.722 / Opus Transcoding)"]
        WA_GW["WhatsApp Voice SIP Gateway\n(SIP over TLS + SRTP)"]
        WRTC_GW["Pion WebRTC Gateway Hub\n(SDP Signaling + ICE / STUN / TURN)"]
    end

    subgraph CoreCarrier ["Go Agentic Carrier Engine (Port 8090)"]
        SESSION["Session & Call Control Manager\n(Caller ID, State, Channel Context)"]
        VAD_HUB["Silero Neural VAD Stream Ingestion\n(Continuous Frame Slicing)"]
        WORKFLOW["Multi-Workflow State Engine\n(Shared Clinical Slot Registry)"]
        TOOLS_CORE["In-Memory Sub-Microsecond Tools\n(< 1 µs Domain Execution)"]
    end

    subgraph AI_Engine ["Unified Local CUDA Inference Pipeline"]
        ASR_MOD["Whisper ASR (Large-v3-Turbo)"]
        CLEF_MOD["Clef-Flash System One Decision"]
        LLM_MOD["Qwen3-4B Spoken Conversational Model"]
        TTS_MOD["Kokoro Real-Time Audio Synthesizer"]
    end

    PSTN --> SIP_GW --> SESSION
    WAPP --> WA_GW --> SESSION
    MAPP --> WRTC_GW --> SESSION
    WEB --> WRTC_GW --> SESSION

    SESSION --> VAD_HUB --> ASR_MOD --> CLEF_MOD --> TOOLS_CORE --> WORKFLOW --> LLM_MOD --> TTS_MOD
    TTS_MOD -. Spoken Audio Stream .-> SESSION
```

---

## 3. Channel Specifications & Protocols

### A. PSTN Telephony (SIP Trunking)
- **Carrier Providers**: Twilio SIP Trunking, Telnyx, Bandwidth.com, or local E1/PRI SIP Trunks.
- **Session Border Controller (SBC)**: FreeSWITCH or Kamailio acting as SIP registrar and media proxy.
- **Codec Negotiation**: G.711 $\mu$-law (PCMU 8kHz), G.711 A-law (PCMA 8kHz), or Opus (16kHz). SBC resamples to 16kHz mono PCM stream piped to the Go Carrier.
- **Features**: Automatic Caller ID binding to patient record, DTMF keypad support, and SIP `REFER` warm transfer to human nurse/doctor.

### B. WhatsApp Voice Calling
- **Interface**: Meta Cloud API for WhatsApp Voice Calling.
- **Security & Protocol**: SIP over TLS (Port 5061) with SRTP (AES-128/256) and Opus Fullband audio (16–48kHz).
- **Features**: Simultaneous in-call rich messaging (dispatching appointment passes or prescription barcodes directly into WhatsApp chat during call).

### C. Native Mobile Applications (iOS & Android)
- **SDKs**: WebRTC Native SDK or LiveKit Client SDK.
- **Audio Hardware Processing**:
  - **iOS**: `AVAudioEngine` with `VoiceProcessingIO` (hardware AEC & AGC).
  - **Android**: `AudioManager.MODE_IN_COMMUNICATION` with native `AcousticEchoCanceler`.
- **Background Ringing**: Apple PushKit + CallKit on iOS; FCM High-Priority Push + Android Telecom ConnectionService on Android.

---

## 4. Latency & Resource Budget

| Layer | Component | Target Latency | Tech Stack | Resource |
| :--- | :--- | :--- | :--- | :--- |
| **Ingress** | Network & Transport | $10\text{--}30\,\text{ms}$ | RTP / WebRTC / SRTP | Go Pion / FreeSWITCH |
| **VAD** | Frame Evaluation | $0.1\,\text{ms}$ | Silero VAD v5 ONNX | WASM / CPU |
| **ASR** | Speech-to-Text | $180\text{--}280\,\text{ms}$ | Whisper.cpp `large-v3-turbo` | CUDA (~1.8 GB VRAM) |
| **Decision** | Zero-Shot Classifier | $30\text{--}60\,\text{ms}$ | Clef-Flash Q4_K_M | CUDA (~2.1 GB VRAM) |
| **Tools** | In-Memory Registry | $< 1\,\mu\text{s}$ | Direct Go Maps | CPU RAM |
| **LLM** | Conversational Response | $80\text{--}150\,\text{ms}$ | Qwen3-4B Q4_K_M | CUDA (~2.5 GB VRAM) |
| **TTS** | Spoken Synthesis | $70\text{--}110\,\text{ms}$ | Kokoro TTS PyTorch | CUDA (~1.2 GB VRAM) |
| **Total** | **End-to-End Turn** | **$380\text{--}650\,\text{ms}$** | **All Local Pipeline** | **12GB RTX 3060** |
