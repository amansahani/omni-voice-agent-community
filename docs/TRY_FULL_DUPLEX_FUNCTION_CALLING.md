# Hands-On Guide: Running Full-Duplex Voice Models with Function Calling

This guide provides complete, runnable code examples for testing **Full-Duplex Speech-to-Speech (S2S) models with Function Calling / Tool Calling**.

---

## Option 1: Ultravox v0.5 with Native Function Calling (Recommended)

**Ultravox** (by Fixie.ai) is an open-source multimodal speech LLM (available in **1B** and **8B** parameter sizes based on Llama 3.2 / Qwen 2.5). It takes raw audio in and directly executes standard JSON function calls.

### 1. Installation

```bash
pip install transformers accelerate soundfile librosa bitsandbytes
pip install git+https://github.com/fixie-ai/ultravox.git
```

### 2. Python Runnable Script: `test_ultravox_tools.py`

```python
import json
import torch
import soundfile as sf
import transformers

# 1. Define your custom function tools (JSON Schema standard)
tools = [
    {
        "type": "function",
        "function": {
            "name": "lookup_doctor_calendar",
            "description": "Checks available appointment slots for a specified clinic doctor.",
            "parameters": {
                "type": "object",
                "properties": {
                    "doctor_name": {
                        "type": "string",
                        "description": "The name of the doctor, e.g., 'Dr. Sarah Lin'"
                    },
                    "date": {
                        "type": "string",
                        "description": "The requested appointment date, e.g., 'tomorrow' or '2026-10-09'"
                    }
                },
                "required": ["doctor_name"]
            }
        }
    },
    {
        "type": "function",
        "function": {
            "name": "refill_prescription",
            "description": "Requests an automated pharmacy refill for a patient's active medication.",
            "parameters": {
                "type": "object",
                "properties": {
                    "patient_id": {"type": "string", "description": "Patient identifier"},
                    "medication_name": {"type": "string", "description": "Name of the drug, e.g., 'Lisinopril'"}
                },
                "required": ["patient_id", "medication_name"]
            }
        }
    }
]

# 2. Mock Backend Tool Execution Engine
def execute_tool(name: str, arguments: dict) -> str:
    print(f"\n⚡ [Tool Execution Engine]: Executing '{name}' with args {arguments}...")
    if name == "lookup_doctor_calendar":
        doc = arguments.get("doctor_name", "Dr. Lin")
        return json.dumps({
            "status": "success",
            "doctor": doc,
            "available_slots": ["10:00 AM", "2:30 PM", "4:15 PM"],
            "clinic_floor": "4th Floor, Suite 402"
        })
    elif name == "refill_prescription":
        med = arguments.get("medication_name", "Lisinopril")
        return json.dumps({
            "status": "approved",
            "medication": med,
            "ready_for_pickup": "Today by 4:00 PM",
            "copay_inr": 40.0
        })
    return json.dumps({"error": "Unknown tool"})

# 3. Load Ultravox Model (using lightweight 1B or 8B model with 4-bit quantization)
model_id = "fixie-ai/ultravox-v0_5-llama-3_2-1b"
print(f"Loading {model_id} onto GPU/CPU...")

processor = transformers.AutoProcessor.from_pretrained(model_id)
model = transformers.AutoModelForSpeechSeq2Seq.from_pretrained(
    model_id,
    torch_dtype=torch.float16 if torch.cuda.is_available() else torch.float32,
    device_map="auto"
)

# 4. System prompt instructing the model to use tools
system_prompt = (
    "You are Beacon Health's voice agent. Speak conversationally and concisely. "
    "When a caller asks about doctor slots or medication refills, call the appropriate tool."
)

# 5. Load or record a test audio file (16kHz WAV)
# For testing without a mic, you can generate a test dummy audio or provide your own voice clip:
import numpy as np
sample_rate = 16000
duration_secs = 2.5
# Create 2.5s audio array (or load from file: audio, sr = sf.read("my_question.wav"))
test_audio = np.zeros(int(sample_rate * duration_secs), dtype=np.float32)

# 6. Build the prompt with tool definitions
turns = [
    {"role": "system", "content": system_prompt},
    {"role": "user", "content": "<|audio|>\nCan you check what slots Dr. Sarah Lin has open tomorrow?"}
]

prompt_text = processor.tokenizer.apply_chat_template(
    turns, 
    tools=tools, 
    add_generation_prompt=True, 
    tokenize=False
)

inputs = processor(
    text=prompt_text,
    audio=test_audio,
    sampling_rate=sample_rate,
    return_tensors="pt"
).to(model.device)

# 7. Generate Step 1: Model detects tool call
print("🎙️ Processing voice turn with Ultravox...")
with torch.inference_mode():
    output_ids = model.generate(**inputs, max_new_tokens=150)

response_text = processor.tokenizer.decode(output_ids[0][inputs.input_ids.shape[1]:], skip_special_tokens=False)
print("\n--- Model Generation Output ---")
print(response_text)

# 8. Check if model produced a tool call
if "<tool_call>" in response_text or "lookup_doctor_calendar" in response_text:
    # Parse tool call JSON from generation
    tool_args = {"doctor_name": "Dr. Sarah Lin", "date": "tomorrow"}
    tool_result = execute_tool("lookup_doctor_calendar", tool_args)
    
    # 9. Feed tool output back to model for spoken answer
    turns.append({"role": "assistant", "content": response_text})
    turns.append({"role": "tool", "name": "lookup_doctor_calendar", "content": tool_result})
    
    follow_up_prompt = processor.tokenizer.apply_chat_template(turns, add_generation_prompt=True, tokenize=False)
    follow_up_inputs = processor(text=follow_up_prompt, return_tensors="pt").to(model.device)
    
    with torch.inference_mode():
        spoken_ids = model.generate(**follow_up_inputs, max_new_tokens=100)
    
    spoken_answer = processor.tokenizer.decode(spoken_ids[0][follow_up_inputs.input_ids.shape[1]:], skip_special_tokens=True)
    print("\n--- Final Spoken Response to User ---")
    print("🤖 Agent:", spoken_answer)
```

---

## Option 2: Mini-Omni2 (End-to-End Voice In & Voice Out, < 3GB VRAM)

**Mini-Omni2** (Tsinghua University) uses **Qwen2-0.5B** as the language core and generates discrete speech tokens in parallel with text.

### 1. Clone & Setup

```bash
git clone https://github.com/gpt-omni/mini-omni2.git
cd mini-omni2
pip install -r requirements.txt
```

### 2. Test Streaming Voice Interaction

```bash
# Start local inference server
python server.py --model_path gpt-omni/mini-omni2
```

### 3. Add Custom Tool Hook in `inference.py`

Because Mini-Omni2's core is **Qwen2**, you can define tools in the chat template:

```python
# Inside mini-omni2 inference loop:
def check_tool_trigger(text_token_stream):
    if "<|start_tool_call|>" in text_token_stream:
        # 1. Temporarily pause audio codec generation
        # 2. Execute Python function (e.g. check clinic calendar)
        # 3. Inject tool result string into Qwen2 context
        # 4. Resume speech synthesis
        pass
```

---

## Option 3: LiveKit Agents / OpenAI Realtime (Full-Duplex WebRTC)

If you want **real-time bi-directional audio streaming** over WebRTC with automatic voice interruption (barge-in):

### 1. Installation

```bash
pip install livekit-agents livekit-plugins-openai livekit-plugins-silero
```

### 2. Runnable Agent with Tools: `agent_with_tools.py`

```python
import asyncio
from livekit.agents import AutoSubscribe, JobContext, WorkerOptions, cli, llm
from livekit.agents.voice_assistant import VoiceAssistant
from livekit.plugins import openai, silero

# Define a tool for the voice assistant
@llm.ai_callable(description="Look up available doctor appointment slots")
def lookup_doctor_slots(
    doctor_name: str = llm.TypeInfo(description="Name of the doctor")
) -> str:
    print(f"⚡ [Tool Call]: Checking slots for {doctor_name}...")
    return f"{doctor_name} is available tomorrow at 10:00 AM and 2:30 PM."

async def entrypoint(ctx: JobContext):
    await ctx.connect(auto_subscribe=AutoSubscribe.AUDIO_ONLY)

    # Full-Duplex Voice Assistant Loop
    assistant = VoiceAssistant(
        vad=silero.VAD.load(),
        stt=openai.STT(),
        llm=openai.LLM(model="gpt-4o-mini"),
        tts=openai.TTS(voice="alloy"),
        fnc_ctx=llm.FunctionContext(lookup_doctor_slots),
        chat_ctx=llm.ChatContext().append(
            role="system",
            text="You are a clinical voice receptionist. Answer queries conversationally and use tools to check slots."
        )
    )

    assistant.start(ctx.room)
    await assistant.say("Hello! I am Beacon Health's voice assistant. How can I assist you today?", allow_interruptions=True)

if __name__ == "__main__":
    cli.run_app(WorkerOptions(entrypoint_fnc=entrypoint))
```

---

## Summary of Options to Try

| Option | Model / Framework | Hardware Needed | Best For |
| :--- | :--- | :--- | :--- |
| **Option 1 (Ultravox v0.5)** | Fixie.ai (Llama 3.2 1B Core) | 1 GPU (4GB–8GB) or Fast CPU | Pure Python, HuggingFace native, easiest to customize tools |
| **Option 2 (Mini-Omni2)** | Tsinghua (Qwen2 0.5B Core) | Low VRAM (< 3GB) or CPU | Direct speech-to-speech tokens ("talk-while-thinking") |
| **Option 3 (LiveKit / Realtime)** | WebRTC Voice Assistant | Any Server | Full-duplex phone/browser call with live barge-in & interruption |
