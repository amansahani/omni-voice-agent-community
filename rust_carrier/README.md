# Rust Omni-Channel Voice Agent Carrier (`rust_carrier`)

High-performance, SIMD-accelerated async voice agent carrier and multi-flow state machine migrated from Go for maximum CPU efficiency, minimum memory footprint, and low-latency audio DSP.

---

## ⚡ Key Highlights & Architecture

### 1. Vectorized SIMD Audio DSP Primitives (`src/dsp/simd.rs`)
- **AVX2 / FMA Vectorized Normalization**: Vectorized 16-bit signed PCM $\leftrightarrow$ normalized 32-bit Float (`[-1.0, 1.0]`) conversion processing 8 audio samples per clock cycle.
- **8-Way Unrolled RMS Power & dBFS**: Sub-microsecond RMS computation for silence / energy gating.
- **Biquad Butterworth 85Hz High-Pass Filter**: Direct Form II real-time filter to suppress low-frequency microphone rumble and AC hum.
- **Zero-Copy WAV Container**: In-place header framing using `bytes::BytesMut`.

### 2. Sub-Microsecond In-Memory Clinical Tool Dispatcher (`src/agent/tools.rs`)
- Instant zero-allocation execution of clinic tools (`< 10 µs`):
  - `book_appointment_slot`
  - `verify_prescription_refill`
  - `query_lab_results`
  - `query_billing_balance`
  - `reschedule_or_cancel_slot`
  - `fetch_available_doctors`

### 3. Dynamic Multi-Flow Business State Machine (`src/agent/workflow.rs`)
- Seven modular clinical and administrative workflows:
  1. `reception_dispatcher` (General Reception & Service Inquiries)
  2. `appointment_scheduling` (Doctor Booking & Slot Locking)
  3. `prescription_refill` (Pharmacy & Delivery Dispatch)
  4. `lab_results` (Diagnostic Panels & Fasting Labs)
  5. `billing_inquiry` (Copay, Invoices & Receipt Dispatch)
  6. `reschedule_cancel` (Booking Adjustments)
  7. `clinical_triage` (Urgent Care Gating)
- Dynamic intent switching and clean call completion directives.

### 4. Dual AI Orchestration & Pipelined Token Streaming (`src/agent/providers.rs`, `src/agent/pipeline.rs`)
- **Local CUDA Daemons**: Local Whisper.cpp (`8082`), Clef-Flash (`8085`), Qwen3-4B (`8080`), Kokoro TTS (`8083`).
- **Cloud Indic APIs**: Sarvam Saaras v3 ASR, TypeSafe Jev System One, Sarvam-105B SSE LLM, Sarvam Bulbul v3 TTS.
- **Pipelined Clause Buffering $\to$ TTS Chunk Streaming**: Streams tokens directly from LLM, detects natural clause boundaries (punctuation/conjunctions), and starts chunked TTS synthesis concurrently (TTFS $< 250\,\text{ms}$).

---

## 🛠️ Build & Run

### Prerequisites
- Rust 1.80+ (Nightly or Stable)
- `pkg-config` / `libssl-dev`

### Native SIMD Release Build
```bash
# Enable native host CPU vector extensions (AVX2, AVX-512, FMA)
RUSTFLAGS="-C target-cpu=native" cargo build --release
```

### Running the Carrier
```bash
# Export environment configuration
export STT_PROVIDER="sarvam"         # or "local"
export SYSTEM_ONE_PROVIDER="jev"     # or "clef"
export LLM_PROVIDER="sarvam"        # or "local"
export TTS_PROVIDER="sarvam"        # or "local"
export SARVAM_API_KEY="your_key"
export JEV_API_KEY="your_key"

./target/release/rust_carrier
```

The Axum server binds by default to `http://0.0.0.0:8090`.

---

## 📡 REST & NDJSON Streaming Endpoints

| Method | Path | Description |
| :--- | :--- | :--- |
| `GET` | `/health` | Carrier health status, active engines, and sanitized config |
| `GET` | `/api/providers` | Active STT, LLM, TTS, and System One providers |
| `POST` | `/api/providers` | Dynamic runtime provider switching |
| `GET` | `/api/workflow` | Active workflow node state, slots, and completed status |
| `POST` | `/api/workflow/switch` | Force switch workflow (`?flow=prescription_refill`) |
| `POST` | `/api/session/reset` | Reset conversational context and workflow history |
| `POST` | `/api/voice-turn` | Audio WAV binary ingestion $\to$ Single JSON result with Base64 audio |
| `POST` | `/api/text-turn` | Text turn JSON ingestion $\to$ Single JSON result |
| `POST` | `/api/voice-turn-stream`| Audio WAV binary ingestion $\to$ Chunked NDJSON stream |
| `POST` | `/api/text-turn-stream` | Text turn JSON ingestion $\to$ Chunked NDJSON stream |
