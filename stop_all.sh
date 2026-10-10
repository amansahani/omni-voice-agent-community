#!/usr/bin/env bash

echo "============================================================"
echo "🛑 Stopping all Beacon Health Voice Agent & Carrier processes..."
echo "============================================================"

fuser -k 8080/tcp 2>/dev/null && echo "  ✔ Stopped Qwen3-4B LLM (Port 8080)" || true
fuser -k 8082/tcp 2>/dev/null && echo "  ✔ Stopped Whisper ASR (Port 8082)" || true
fuser -k 8083/tcp 2>/dev/null && echo "  ✔ Stopped Kokoro TTS (Port 8083)" || true
fuser -k 8085/tcp 2>/dev/null && echo "  ✔ Stopped Clef-Flash Decision (Port 8085)" || true
fuser -k 8090/tcp 2>/dev/null && echo "  ✔ Stopped Go Media Carrier (Port 8090)" || true

echo ""
echo "✅ All 5 backend engines and ports successfully cleaned up."
