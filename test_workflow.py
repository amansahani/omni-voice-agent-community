import requests
import json
import time

TTS_URL = "http://127.0.0.1:8083/v1/tts"
VOICE_TURN_URL = "http://127.0.0.1:8090/api/voice-turn"
RESET_URL = "http://127.0.0.1:8090/api/reset-session"

def synthesize_wav(text: str) -> bytes:
    resp = requests.post(TTS_URL, json={"text": text, "voice": "af_heart", "speed": 1.0})
    if resp.status_code != 200:
        raise RuntimeError(f"TTS synthesis failed: {resp.text}")
    return resp.content

def run_turn(text: str):
    print(f"\n=======================================================")
    print(f"🎙️  CALLER: \"{text}\"")
    print(f"=======================================================")
    wav_bytes = synthesize_wav(text)
    
    t0 = time.time()
    resp = requests.post(VOICE_TURN_URL, data=wav_bytes, headers={"Content-Type": "audio/wav"})
    elapsed = (time.time() - t0) * 1000
    
    if resp.status_code != 200:
        print(f"❌ Error: {resp.status_code} - {resp.text}")
        return None
        
    data = resp.json()
    wf = data.get("workflow_state", {})
    tool = data.get("tool_executed")
    
    print(f"🦻 Whisper ASR ({data.get('asr_latency_ms', 0):.1f} ms): \"{data.get('transcribed_text')}\"")
    print(f"🧠 Intent Decision: {data.get('decisions')}")
    if tool:
        print(f"⚡ Tool Executed: {tool.get('tool_name')} ({tool.get('duration_microseconds')} µs)")
        print(f"   Tool Output: {json.dumps(tool.get('result'))[:140]}...")
    else:
        print(f"⚡ Tool Executed: (None)")
        
    print(f"📍 State Machine:")
    print(f"   Flow: {wf.get('workflow_name')} [{wf.get('active_workflow_id')}]")
    print(f"   Node: {wf.get('current_node_title')} [{wf.get('current_node_id')}]")
    print(f"   Slots: {wf.get('slots')}")
    print(f"🤖 Aria Spoken Response ({data.get('llm_latency_ms', 0):.1f} ms LLM, {data.get('tts_latency_ms', 0):.1f} ms TTS):")
    print(f"   \"{data.get('generated_speech')}\"")
    print(f"⏱️ Total Turn Latency: {data.get('total_pipeline_latency_ms', 0):.1f} ms (Round-Trip: {elapsed:.1f} ms)")
    return data

def main():
    print("Resetting session...")
    requests.get(RESET_URL)
    
    # Test 1: Service Catalog Inquiry
    run_turn("What services do you have?")
    
    # Test 2: Doctor Qualifications & Credentials Inquiry
    run_turn("Can you let me know about Dr. Marcus Vance's profile and board certifications?")
    
    # Test 3: Booking Request (No Department specified)
    run_turn("I would like to book a doctor appointment.")
    
    # Test 4: Department Selection (Dermatology)
    run_turn("I need dermatology please.")
    
    # Test 5: Time Slot Selection
    run_turn("1:15 PM works for me.")
    
    # Test 6: Patient Intake
    run_turn("My name is Alex Johnson.")
    
    # Test 7: Cancellation / Reschedule Inquiry
    run_turn("Can you cancel my appointment with Dr. Marcus Vance?")

if __name__ == "__main__":
    main()
