# Omni-Channel Voice Agent | System Configuration & Runbook (`config.md`)

This configuration document contains all binary resolutions, environment variables, command-line flags, port assignments, and execution runbooks for the **Omni-Channel Real-Time Voice Agent & Carrier**.

---

## 1. Service Port & Component Registry

| Port | Component | Binary / Interpreter | Model File / Script | Key Flags |
| :--- | :--- | :--- | :--- | :--- |
| **8080** | **LLaMA Server (Qwen3-4B)** | `llama-server` (or `$LLAMA_BIN`) | `$QWEN_MODEL` (`Qwen3-4B-Q4_K_M.gguf`) | `-ngl 99 -c 4096 --reasoning off` |
| **8082** | **Whisper ASR Server** | `whisper-server` (or `$WHISPER_BIN`) | `$WHISPER_MODEL` (`ggml-large-v3-turbo-q5_0.bin`) | `--port 8082 --host 127.0.0.1` |
| **8083** | **Kokoro TTS Server** | `python3` (or `$PYTHON_BIN`) | `tts_server.py` | `af_heart`, PyTorch CUDA |
| **8085** | **Local Clef-Flash System One** | `llama-server` (or `$LLAMA_BIN`) | `$CLEF_MODEL` (`Clef-Flash-Q4_K_M.gguf`) | `-ngl 20 -c 4096 -b 2048 -ub 2048 -np 1` |
| **Cloud** | **TypeSafe Jev System One** | `https://api.typesafe.ai/v1/systemone` | `jev-latest` (TypeSafe API) | Cloud System One Decision Engine (~200ms) |
| **Cloud** | **Sarvam Indic Cloud Stack** | `https://api.sarvam.ai` | `saaras:v3`, `sarvam-105b`, `bulbul:v3` | 10+ Indic Languages & 30+ Voices |
| **8090** | **Go Agentic Carrier Hub** | `./media_server` | Built from `main.go` | Pion WebRTC v4 + In-Memory Tools + Workflows |

---

## 2. Service Startup Commands

### 1. Whisper ASR Server (Port 8082)
- **Binary**: `whisper-server`
- **Model**: `models/ggml-large-v3-turbo-q5_0.bin`
```bash
whisper-server \
  -m models/ggml-large-v3-turbo-q5_0.bin \
  --port 8082 \
  --host 127.0.0.1
```

### 2. Conversational LLM (Qwen3-4B on Port 8080)
- **Binary**: `llama-server`
- **Model**: `models/Qwen3-4B-Q4_K_M.gguf`
```bash
llama-server \
  -m models/Qwen3-4B-Q4_K_M.gguf \
  -ngl 99 \
  -c 4096 \
  --port 8080 \
  --host 127.0.0.1 \
  --reasoning off
```

### 3. Clef-Flash System One Decision Engine (Port 8085)
- **Binary**: `llama-server`
- **Model**: `models/Clef-Flash-Q4_K_M.gguf`
```bash
llama-server \
  -m models/Clef-Flash-Q4_K_M.gguf \
  -ngl 20 \
  -c 4096 \
  -b 2048 \
  -ub 2048 \
  -np 1 \
  --port 8085 \
  --host 127.0.0.1
```

### 4. Kokoro TTS & Server-Side VAD (Port 8083)
- **Interpreter**: `python3`
- **Script**: `tts_server.py`
```bash
python3 tts_server.py
```

### 5. Go Agentic Media Carrier (Port 8090)
- **Build & Run**:
```bash
go build -o media_server main.go
./media_server
```

---

## 3. Automated One-Click Shell Scripts

Two executable shell scripts are provided in the root directory:

- **`start_all.sh`**: Automatically stops any stale port bindings, launches all 5 backend engines with exact parameters in the background, waits for health checks to pass on all 5 ports, and provides live status.
- **`stop_all.sh`**: Safely terminates all 5 backend servers and cleans up port bindings (`8080`, `8082`, `8083`, `8085`, `8090`).

### Quick Usage:
```bash
# To start everything:
./start_all.sh

# To stop everything:
./stop_all.sh
```

---

## 4. Hardware Resource Allocation (NVIDIA RTX 12GB VRAM Profile)

```
+-----------------------------------------------------------------------------+
| NVIDIA-SMI GPU 12GB VRAM Budget Profile                                     |
|=============================================================================|
| Process                          | Port | VRAM Usage | CPU / Execution Mode |
|----------------------------------+------+------------+----------------------|
| whisper-server (large-v3-turbo)  | 8082 | ~1.8 GB    | CUDA cuBLAS          |
| llama-server (Clef-Flash Q4_K_M) | 8085 | ~2.1 GB    | CUDA (-ngl 20)       |
| llama-server (Qwen3-4B Q4_K_M)   | 8080 | ~2.6 GB    | CUDA (-ngl 99)       |
| tts_server.py (Kokoro af_heart)  | 8083 | ~1.2 GB    | PyTorch CUDA         |
| media_server (Pion WebRTC + SFU) | 8090 | ~0.1 GB    | Go In-Memory RAM     |
|----------------------------------+------+------------+----------------------|
| Total VRAM Allocated             |      | ~7.8 GB    | Safe within 12GB!    |
+-----------------------------------------------------------------------------+
```
