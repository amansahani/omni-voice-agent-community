import os
import io
import time
import torch
import numpy as np
import soundfile as sf
import onnxruntime as ort
from fastapi import FastAPI, Response, Request
from pydantic import BaseModel
from kokoro import KModel, KPipeline

app = FastAPI(title="Ultra-Low Latency Kokoro TTS & Silero VAD Service")

device = "cuda" if torch.cuda.is_available() else "cpu"
print(f"Initializing Kokoro on {device}...")

BASE_DIR = os.path.dirname(os.path.abspath(__file__))
KOKORO_CONFIG = os.environ.get("KOKORO_CONFIG", os.path.join(BASE_DIR, "models/Kokoro-82M/config.json"))
KOKORO_MODEL = os.environ.get("KOKORO_MODEL", os.path.join(BASE_DIR, "models/Kokoro-82M/kokoro-v1_0.pth"))
KOKORO_VOICE = os.environ.get("KOKORO_VOICE", os.path.join(BASE_DIR, "models/Kokoro-82M/voices/af_heart.pt"))
SILERO_VAD_MODEL = os.environ.get("SILERO_VAD_MODEL", os.path.join(BASE_DIR, "silero_vad_v5.onnx"))

if os.path.exists(KOKORO_CONFIG) and os.path.exists(KOKORO_MODEL):
    kmodel = KModel(config=KOKORO_CONFIG, model=KOKORO_MODEL).to(device).eval()
    pipeline = KPipeline(lang_code="a", model=kmodel, device=device)
    if os.path.exists(KOKORO_VOICE):
        voice_tensor = torch.load(KOKORO_VOICE, map_location=device)
        pipeline.voices["af_heart"] = voice_tensor
else:
    pipeline = KPipeline(lang_code="a", device=device)

# Warmup JIT / CUDA kernels
print("Warming up TTS model...")
try:
    for _ in pipeline("Hello, ready.", voice="af_heart"):
        pass
    print("Kokoro TTS warmed up and ready!")
except Exception as e:
    print(f"TTS Warmup notice: {e}")

# Load Silero VAD neural network (v5.0 official architecture with 64-sample context)
print("Loading Silero VAD model (v5.0 ONNX)...")
vad_session = ort.InferenceSession(SILERO_VAD_MODEL, providers=["CPUExecutionProvider"])
vad_sr = np.array(16000, dtype=np.int64)
print("Silero VAD v5 loaded!")


class TTSRequest(BaseModel):
    text: str
    voice: str = "af_heart"
    speed: float = 1.05


@app.get("/health")
def health():
    return {"status": "ok", "device": device, "vad": "silero_v5"}


@app.post("/v1/tts")
def synthesize(req: TTSRequest):
    start = time.perf_counter()
    chunks = []
    
    generator = pipeline(req.text, voice=req.voice, speed=req.speed)
    for _, _, audio in generator:
        audio_np = audio.cpu().numpy() if hasattr(audio, "cpu") else audio
        chunks.append(audio_np)
        break  # Return first phrase chunk immediately for minimal TTFB
        
    if not chunks:
        return Response(status_code=500, content="Failed to synthesize")
        
    audio_data = chunks[0]
    out_buf = io.BytesIO()
    sf.write(out_buf, audio_data, 24000, format="WAV")
    wav_bytes = out_buf.getvalue()
    
    elapsed_ms = (time.perf_counter() - start) * 1000.0
    return Response(
        content=wav_bytes,
        media_type="audio/wav",
        headers={
            "X-TTS-Latency-Ms": f"{elapsed_ms:.1f}",
            "X-Audio-Duration-Ms": f"{len(audio_data)/24.0:.1f}",
        }
    )


import scipy.signal

class VADBatchRequest(BaseModel):
    pcm16_samples: list[float]  # Float samples normalized [-1.0, 1.0]
    sample_rate: int = 16000     # Input sample rate from browser


# Continuous streaming VAD LSTM states
stream_state = np.zeros((2, 1, 128), dtype=np.float32)
stream_context = np.zeros((1, 64), dtype=np.float32)


@app.post("/v1/vad/reset")
def reset_vad_state():
    global stream_state, stream_context
    stream_state = np.zeros((2, 1, 128), dtype=np.float32)
    stream_context = np.zeros((1, 64), dtype=np.float32)
    return {"status": "vad state reset"}


@app.post("/v1/vad")
def check_vad(req: VADBatchRequest):
    """
    Run Silero VAD v5 across streaming 512-sample (32ms) windows at 16kHz.
    Preserves 64-sample context prefix + (2, 1, 128) recurrent state.
    Silero V5 completely eliminates fan noise (< 0.01) and keyboard clicks (< 0.19)
    while detecting human speech at 30-60cm desk distance (> 0.95).
    """
    global stream_state, stream_context
    raw_samples = np.array(req.pcm16_samples, dtype=np.float32)
    if len(raw_samples) == 0:
        return {"speech_prob": 0.0, "is_speech": False, "frames": 0, "max_prob": 0.0}

    # Resample to 16000 Hz if necessary
    if req.sample_rate != 16000 and req.sample_rate > 0:
        num_target = int(len(raw_samples) * 16000 / req.sample_rate)
        if num_target < 512:
            return {"speech_prob": 0.0, "is_speech": False, "frames": 0, "max_prob": 0.0}
        samples = scipy.signal.resample(raw_samples, num_target).astype(np.float32)
    else:
        samples = raw_samples

    chunk_size = 512
    if len(samples) < chunk_size:
        return {"speech_prob": 0.0, "is_speech": False, "frames": 0, "max_prob": 0.0}

    probs = []

    # Stream through 512-sample windows with 64-sample rolling context
    # If a large batch (> 16000 samples / 1s) is sent, process with a local state
    # to avoid contaminating the live stream state with whole-file scans.
    is_batch_scan = len(samples) >= 8000
    cur_state = np.zeros((2, 1, 128), dtype=np.float32) if is_batch_scan else stream_state
    cur_context = np.zeros((1, 64), dtype=np.float32) if is_batch_scan else stream_context

    for i in range(0, len(samples) - chunk_size + 1, chunk_size):
        chunk = samples[i : i + chunk_size].reshape(1, chunk_size)
        inp = np.concatenate([cur_context, chunk], axis=1)  # shape (1, 576)
        out, cur_state = vad_session.run(None, {"input": inp, "state": cur_state, "sr": vad_sr})
        cur_context = chunk[:, -64:]
        probs.append(float(out[0][0]))

    if not is_batch_scan:
        stream_state = cur_state
        stream_context = cur_context

    if not probs:
        return {"speech_prob": 0.0, "is_speech": False, "frames": 0, "max_prob": 0.0}

    max_prob = float(np.max(probs))
    mean_prob = float(np.mean(probs))
    latest_prob = float(probs[-1])
    speech_frames = sum(1 for p in probs if p >= 0.30)

    # In Silero V5:
    # Fan noise: max_prob ~0.006 (0.6%)
    # Keyboard clicks: max_prob ~0.19 (transient 1 frame)
    # Speech (including soft onset words 'yeah', 'yes', 'no'): max_prob 0.35 - 0.999
    is_speech = (max_prob >= 0.30) or (latest_prob >= 0.30)

    return {
        "is_speech": bool(is_speech),
        "max_prob": max_prob,
        "mean_prob": mean_prob,
        "latest_prob": latest_prob,
        "frames": len(probs),
    }



if __name__ == "__main__":
    import uvicorn
    uvicorn.run(app, host="127.0.0.1", port=8083, log_level="warning")
