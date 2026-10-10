#!/usr/bin/env bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
mkdir -p "$SCRIPT_DIR/logs"

echo "============================================================"
echo "🚀 Starting Omni-Channel Real-Time Voice Agent & Carrier..."
echo "============================================================"

# Binary & Model Path Resolution (Override via Environment Variables)
WHISPER_BIN="${WHISPER_BIN:-whisper-server}"
WHISPER_MODEL="${WHISPER_MODEL:-./models/ggml-large-v3-turbo-q5_0.bin}"
LLAMA_BIN="${LLAMA_BIN:-llama-server}"
QWEN_MODEL="${QWEN_MODEL:-./models/Qwen3-4B-Q4_K_M.gguf}"
CLEF_MODEL="${CLEF_MODEL:-./models/Clef-Flash-Q4_K_M.gguf}"
PYTHON_BIN="${PYTHON_BIN:-python3}"

# 1. Kill any existing instances on ports 8080, 8082, 8083, 8085, 8090
echo "🧹 Cleaning up existing port listeners..."
fuser -k 8080/tcp 2>/dev/null || true
fuser -k 8082/tcp 2>/dev/null || true
fuser -k 8083/tcp 2>/dev/null || true
fuser -k 8085/tcp 2>/dev/null || true
fuser -k 8090/tcp 2>/dev/null || true
sleep 1

# 2. Start Whisper ASR Server (Port 8082)
echo "🎙️ Starting Whisper ASR Server (Port 8082)..."
"$WHISPER_BIN" \
  -m "$WHISPER_MODEL" \
  --port 8082 \
  --host 127.0.0.1 > "$SCRIPT_DIR/logs/whisper.log" 2>&1 &
PID_WHISPER=$!

# 3. Start Conversational LLM Qwen3-4B (Port 8080)
echo "🧠 Starting Qwen3-4B Conversational LLM Server (Port 8080)..."
"$LLAMA_BIN" \
  -m "$QWEN_MODEL" \
  -ngl 99 \
  -c 4096 \
  --port 8080 \
  --host 127.0.0.1 \
  --reasoning off > "$SCRIPT_DIR/logs/llm.log" 2>&1 &
PID_LLM=$!

# 4. Start Clef-Flash System One Zero-Shot Decision Engine (Port 8085)
echo "⚡ Starting Clef-Flash System One Decision Engine (Port 8085)..."
"$LLAMA_BIN" \
  -m "$CLEF_MODEL" \
  -ngl 20 \
  -c 4096 \
  -b 2048 \
  -ub 2048 \
  -np 1 \
  --port 8085 \
  --host 127.0.0.1 > "$SCRIPT_DIR/logs/clef.log" 2>&1 &
PID_CLEF=$!

# 5. Start Kokoro TTS & Server-Side VAD (Port 8083)
echo "🔊 Starting Kokoro TTS & Neural VAD Engine (Port 8083)..."
"$PYTHON_BIN" "$SCRIPT_DIR/tts_server.py" > "$SCRIPT_DIR/logs/tts.log" 2>&1 &
PID_TTS=$!

# 6. Build and Start Rust Carrier (Port 8090)
echo "🌐 Compiling & Starting Rust Carrier & Web UI (Port 8090)..."
cd "$SCRIPT_DIR/rust_carrier"
if [ ! -f "target/release/rust_carrier" ]; then
  RUSTFLAGS="-C target-cpu=native" cargo build --release
fi
./target/release/rust_carrier > "$SCRIPT_DIR/logs/carrier.log" 2>&1 &
PID_CARRIER=$!
cd "$SCRIPT_DIR"

echo ""
echo "⏳ Waiting for all 5 backend engines to initialize and pass health checks..."

wait_for_port() {
  local port=$1
  local name=$2
  local max_attempts=60
  local attempt=1

  while [ $attempt -le $max_attempts ]; do
    if curl -s "http://127.0.0.1:$port/" >/dev/null 2>&1 || curl -s "http://127.0.0.1:$port/health" >/dev/null 2>&1 || curl -s "http://127.0.0.1:$port/v1/models" >/dev/null 2>&1; then
      echo "  ✅ $name is LIVE on port $port (Attempt $attempt)"
      return 0
    fi
    sleep 0.5
    attempt=$((attempt + 1))
  done

  echo "  ❌ Timed out waiting for $name on port $port. Check $SCRIPT_DIR/logs/"
  return 1
}

wait_for_port 8082 "Whisper ASR"
wait_for_port 8080 "Qwen3-4B LLM"
wait_for_port 8085 "Clef-Flash Decision"
wait_for_port 8083 "Kokoro TTS"
wait_for_port 8090 "Rust Carrier & Web UI"

echo ""
echo "============================================================"
echo "🎉 ALL 5 LOCAL ENGINES ARE RUNNING & READY!"
echo "📡 Open Web Carrier Dashboard: http://localhost:8090"
echo "📜 Live server logs stored in: $SCRIPT_DIR/logs/"
echo "🛑 To stop all services, run:  ./stop_all.sh"
echo "============================================================"
