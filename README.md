# Omni-Channel Voice Agent (Community Edition) 🎙️⚡

An ultra-low latency, real-time omni-channel voice platform featuring dual AI engines:
1. **Local CUDA Stack** *(Whisper.cpp + Clef + Qwen3-4B + Kokoro TTS)* — 100% Free & Local
2. **Cloud Indic & HD Stack** *(Sarvam AI Indic Saaras/Bulbul/105B + ElevenLabs Turbo v2.5)* — Bring Your Own Key (BYOK)

Built on **Rust** (Axum async carrier + AVX2/AVX-512 SIMD audio DSP), in-browser Silero Neural VAD v5 (WebAssembly), sub-microsecond in-memory clinical tool execution, hierarchical workflow studio designer, and pipelined streaming token-to-speech clause synthesis.

---

## 🚀 Key Architecture & Capabilities

* **Dual AI Orchestration Engine**:
  * **Local Stack**: Whisper.cpp (CUDA STT) $\to$ Clef-Flash (System One Decision Engine) $\to$ Qwen3-4B (Chat Completion) $\to$ Kokoro TTS.
  * **Cloud Indic Stack**: Sarvam Saaras v3 (STT) $\to$ TypeSafe Jev API (System One Decision Engine) $\to$ Sarvam-105B (Indic LLM) $\to$ Sarvam Bulbul v3 (TTS).
  * **Ultra-HD Voice**: ElevenLabs Turbo v2.5 streaming TTS.
* **Bring Your Own Key (BYOK) Support**:
  * Users can enter their own Sarvam AI, ElevenLabs, or TypeSafe Jev API keys directly in the frontend UI modal.
  * Keys are stored securely in browser `localStorage` and never saved to a database or logged on the server.
* **Vectorized SIMD Audio DSP**:
  * Vectorized PCM16 $\leftrightarrow$ Float32 normalization, 8-way RMS dBFS, and real-time 85Hz high-pass filter.
* **Streaming Audio Pipeline**:
  * Token-by-token Server-Sent streaming (NDJSON) through natural punctuation clause buffer.
  * Time-to-First-Sound (TTFS) $< 250\,\text{ms}$.
* **In-Browser Silero VAD v5 (WebAssembly)**:
  * Neural voice activity detection directly in the browser with 85Hz high-pass filter, digital preamp gain control, and broadcast dynamics compression.
  * Automatic filtering of keyboard clicks, fan noise, and background breathing.
* **Sub-Microsecond In-Memory Clinical Tool Dispatch**:
  * Zero-allocation in-memory tools for appointment scheduling, prescription refills, lab results inquiry, copay processing, and clinical triage ($< 1\,\mu\text{s}$).
* **Studio Designer & Knowledge Base**:
  * Visual node-based workflow designer (inspired by ElevenLabs / Gnani.ai).
  * Integrated sub-5ms BM25/FTS knowledge ingestion and dynamic webhook registration.

---

## 🛠️ Quick Start

### 1. Configure Environment (Optional)

Copy the example environment file:

```bash
cp .env.example .env
```

If you prefer to configure keys via environment variables instead of the frontend BYOK dialog:

```env
STT_PROVIDER=local          # "sarvam" or "local"
SYSTEM_ONE_PROVIDER=clef     # "jev" or "clef"
LLM_PROVIDER=local          # "sarvam" or "local"
TTS_PROVIDER=local          # "sarvam", "elevenlabs", or "local"

# Cloud Keys (Leave blank to use free local stack or enter via UI)
SARVAM_API_KEY=
ELEVENLABS_API_KEY=
JEV_API_KEY=
```

### 2. Start Local CUDA Servers (If using Local Stack)

```bash
chmod +x start_all.sh stop_all.sh
./start_all.sh
```

### 3. Build & Run Rust Carrier

```bash
cd rust_carrier
RUSTFLAGS="-C target-cpu=native" cargo build --release
./target/release/rust_carrier
```

Open your browser at `http://localhost:8090` to start voice calls or visit `http://localhost:8090/studio.html` to design agentic workflows.

---

## 📁 Repository Structure

```
├── rust_carrier/
│   ├── src/
│   │   ├── agent/
│   │   │   ├── pipeline.rs      # Semantic routing & pipelined token streaming
│   │   │   ├── providers.rs     # STT/LLM/TTS/System One clients (Local CUDA & Cloud)
│   │   │   ├── tools.rs         # Sub-microsecond in-memory clinical tool dispatch
│   │   │   ├── workflow.rs      # 7-flow dynamic state machine & directives
│   │   │   ├── knowledge.rs     # Sub-5ms BM25 keyword RAG search
│   │   │   └── dynamic_tools.rs # Dynamic HTTP webhook connectors
│   │   ├── dsp/
│   │   │   └── simd.rs          # AVX2/AVX-512 vectorized audio DSP & 85Hz filter
│   │   └── main.rs              # Axum async HTTP & NDJSON streaming carrier
│   └── Cargo.toml               # Rust dependencies & release profile optimizations
├── web/
│   ├── index.html               # Real-time voice console & Silero VAD v5 WebAssembly
│   └── studio.html              # Enterprise visual agent workflow & knowledge studio
├── docs/
│   ├── FULL_DUPLEX_VOICE_MODELS_RESEARCH.md # S2S models (Moshi, Mini-Omni2, Ultravox)
│   ├── TRY_FULL_DUPLEX_FUNCTION_CALLING.md  # Runnable code guide for S2S tool calling
│   └── TELEMETRY_AND_APPROVAL_ARCHITECTURE.md
├── start_all.sh                 # Launch script for local CUDA servers & Rust carrier
└── stop_all.sh                  # Teardown script for background daemon processes
```

---

## 🔒 Privacy & Open Source Notice

* **Zero Tracking**: Your API keys entered into the UI are stored exclusively in your browser's `localStorage` and sent over your own private local carrier session.
* **No Database Sync Required**: The application runs completely stand-alone out-of-the-box.
* **MIT / Apache-2.0 License**: Feel free to adapt for personal, research, or enterprise voice solutions.
