# Beacon Health | Real-Time Agentic Voice Orchestration Logic

This document specifies the end-to-end **Orchestration Logic**, **Turn Lifecycle State Machine**, **Multi-Workflow Engine**, and **Concurrency Models** powering the Beacon Health Local Real-Time Agentic Voice Carrier.

---

## 1. End-to-End Turn Orchestration Flow

```mermaid
sequenceDiagram
    autonumber
    actor Caller as Caller / User
    participant VAD as Silero VAD v5 (32ms Frame)
    participant Carrier as Go Carrier Orchestrator
    participant ASR as STT Engine (Saaras / Whisper)
    participant S1 as System One (Jev / Clef)
    participant Tools as In-Memory Tool Registry (< 1 µs)
    participant WF as Multi-Workflow State Machine
    participant LLM as LLM Engine (Sarvam-105B / Qwen3-4B)
    participant TTS as TTS Engine (Bulbul / Kokoro)

    Caller->>VAD: Continuous Audio Stream (16kHz PCM)
    Note over VAD: Slices utterance upon 800ms silence pause<br/>Pre-roll buffer (800ms) prepended
    VAD->>Carrier: POST /api/voice-turn-stream (Clean WAV payload)
    
    Carrier->>ASR: Transcribe Audio (Saaras v3 / Whisper.cpp)
    ASR-->>Carrier: Text: "Can I check cardiology slots tomorrow and refill Lisinopril?"

    Carrier->>S1: Zero-Shot Multi-Intent & Slot Classification (~200ms)
    S1-->>Carrier: Intent=book_appointment, also_refill=yes, dept=Cardiology

    Note over Carrier,Tools: Dynamic Multi-Goal Resolution & Dispatch (< 1 µs)
    Carrier->>Tools: check_calendar_availability(Cardiology, Tomorrow)
    Tools-->>Carrier: Doctor: Dr. Marcus Vance, Slots: [9:30 AM, 1:15 PM, 4:00 PM] (0.61 µs)
    Carrier->>Tools: lookup_prescription(Lisinopril 10mg)
    Tools-->>Carrier: Refills: 3 remaining, Stock: Available, Copay: $12.00 (0.45 µs)

    Carrier->>WF: Update Slots & Advance Flow State
    WF-->>Carrier: Active Flow: appointment_scheduling, Node: select_time_slot
    Carrier-->>Caller: NDJSON Event: "meta" (Transcribed Text + Tools + Workflow UI)

    Note over Carrier,LLM: Pipelined Streaming: LLM Tokens -> Clause Buffer -> TTS Chunks
    Carrier->>LLM: GenerateChatStream (SSE Stream)
    LLM-->>Carrier: Tokens: "For", " cardiology", " with", " Dr.", " Vance," (TTFT ~40ms)
    Carrier-->>Caller: NDJSON Event: "text_delta" (Live typing)
    
    Note over Carrier,TTS: Clause 1 Boundary Detected -> Async TTS Chunk 1
    Carrier->>TTS: Synthesize("For cardiology with Dr. Vance,")
    TTS-->>Carrier: Audio Chunk 1 WAV Buffer (~160ms)
    Carrier-->>Caller: NDJSON Event: "audio_chunk" (Chunk 1) -> Audio Queue Starts Playing! (TTFS ~210ms)

    LLM-->>Carrier: Tokens: " we", " have", " 9:30", " AM", " and", " 1:15", " PM."
    Carrier->>TTS: Synthesize("we have 9:30 AM and 1:15 PM.")
    TTS-->>Carrier: Audio Chunk 2 WAV Buffer (Pre-buffered while Chunk 1 plays)
    Carrier-->>Caller: NDJSON Event: "audio_chunk" (Chunk 2) -> Seamless Gapless Queue Playback
    Carrier-->>Caller: NDJSON Event: "final" (Turn Metrics Summary)
```

---

## 2. Real-Time Turn Lifecycle State Machine

The orchestration pipeline transitions through six strictly ordered deterministic states per conversational turn:

```mermaid
stateDiagram-v2
    [*] --> IdleListening: Call Connected
    
    IdleListening --> HumanSpeechDetected: Silero VAD Prob >= 0.22
    HumanSpeechDetected --> HumanSpeechDetected: Accumulate Audio Frames
    HumanSpeechDetected --> SilencePauseDetected: Silence >= 800ms
    
    SilencePauseDetected --> PipelineProcessing: Utterance >= 60ms & Valid Audio
    SilencePauseDetected --> IdleListening: Ambient Noise (< 60ms)
    
    state PipelineProcessing {
        [*] --> WhisperASR
        WhisperASR --> ClefSystemOneDecision: Text != ""
        ClefSystemOneDecision --> MicrosecondToolDispatch: Intent & Slot Classification
        MicrosecondToolDispatch --> WorkflowStateAdvancement: Execute In-Memory Tools
        WorkflowStateAdvancement --> LLMSpokenGeneration: Synthesize Directives
        LLMSpokenGeneration --> KokoroTTSSynthesis: Concise Spoken Text
        KokoroTTSSynthesis --> [*]
    }
    
    PipelineProcessing --> AgentSpeaking: Dispatch Audio & Playback
    
    state AgentSpeaking {
        [*] --> NormalPlayback
        NormalPlayback --> BargeInTriggered: Caller Voice Prob >= 0.70 (3 frames)
        BargeInTriggered --> [*]: Immediate Cutoff & Reset
    }
    
    AgentSpeaking --> EchoCooldown: Audio Playback Ended
    EchoCooldown --> IdleListening: Cooldown Elapsed (350ms)
```

---

## 3. Algorithmic Orchestration Breakdown

### Step 1: VAD Slicing & Pre-Roll Buffering
- **Frame Size**: $512$ samples ($32\,\text{ms}$ at $16\,\text{kHz}$).
- **Pre-Roll Circular Buffer**: Stores the preceding $800\,\text{ms}$ ($12,800$ samples) in memory.
- **Speech Onset Trigger**: Activated when Silero VAD human voice probability $P \ge 0.22$ (or single-frame debounce $P \ge 0.18$).
- **Turn Assembly**: Onset immediately prepends the pre-roll buffer to prevent losing initial short affirmative words (*"Yeah"*, *"Yes"*, *"No"*, *"Sure"*).
- **Silence Debounce**: Finalizes the utterance after $800\,\text{ms}$ of continuous silence ($P < 0.18$).

### Step 2: System One Fast Semantic Classification
Instead of slow recursive LLM tool-calling roundtrips, incoming text is evaluated in a single forward pass by **Clef-Flash System One** (Port `8085`):
```json
{
  "state": "The caller says: \"Can I check cardiology slots tomorrow and refill Lisinopril?\"",
  "questions": {
    "intent": { "type": "choice", "criteria": { "book_appointment": "...", "refill_medication": "...", ... } },
    "also_refill": { "type": "choice", "criteria": { "yes": "Refill requested", "no": "No refill" } },
    "also_booking": { "type": "choice", "criteria": { "yes": "Booking requested", "no": "No booking" } },
    "department": { "type": "choice", "criteria": { "Cardiology": "...", "Dermatology": "...", ... } },
    "time_slot": { "type": "choice", "criteria": { "9:30 AM": "...", "1:15 PM": "...", "4:00 PM": "..." } },
    "fulfillment": { "type": "choice", "criteria": { "Delivery": "...", "Pickup": "..." } },
    "pay_bill": { "type": "choice", "criteria": { "yes": "Pay bill", "no": "Not paying" } }
  }
}
```

### Step 3: Sub-Microsecond In-Memory Tool Execution
The orchestrator dynamically dispatches mapped domain tools in Go memory:
- `check_calendar_availability`: Looks up physician schedule in $\approx 0.61\,\mu\text{s}$.
- `lookup_prescription`: Verifies active medication, copay, and remaining refills in $\approx 0.45\,\mu\text{s}$.
- `lookup_doctor_profile`: Fetches board certification and background in $\approx 0.51\,\mu\text{s}$.
- `confirm_appointment_booking`: Issues verified reservation ID in $\approx 0.82\,\mu\text{s}$.
- `process_copay_payment`: Authorizes credit card transaction in $\approx 0.75\,\mu\text{s}$.
- `clinical_symptom_triage`: Classifies urgent acuity levels in $\approx 0.40\,\mu\text{s}$.

### Step 4: Multi-Goal Compound Directive Synthesis
When multiple requests are identified in a single caller utterance, the orchestrator generates a unified multi-task directive header:
```
MULTI-GOAL UTTERANCE (2 distinct requests detected: (1) Doctor Appointment Scheduling, (2) Prescription Refill Order).
Directives: You MUST address ALL 2 requests in your single spoken response using the system execution results below.
- Appointment Availability: Available slots for Cardiology tomorrow with Dr. Marcus Vance are 9:30 AM, 1:15 PM, and 4:00 PM.
- Prescription Found: Lisinopril 10mg has 3 refills in stock ($12.00 copay). Home delivery or pickup available.
```

### Step 5: Conversational Spoken LLM Generation
- **Model**: Qwen3-4B running on `llama-server` (Port `8080`).
- **Spoken Negative Constraints**:
  - `CRITICAL SPOKEN RULE: NEVER begin your response with 'Hello', 'Hi', 'Hi there', 'Hey', 'Welcome to Beacon Health', or 'I'm Aria' unless this is the very first turn of the conversation.`
  - `Always prioritize answering the caller's LATEST utterance without repeating previously completed tasks.`
  - `Concise natural spoken response: 1-2 sentences maximum, zero markdown bullet points or numbers.`

### Step 6: Acoustic Echo Suppression & Cooldown
- **Muted DSP Loop**: The Web Audio ScriptProcessor output is routed through a zero-gain `muteNode` (`gain.value = 0.0`) to destination, preventing physical speaker feedback.
- **Playback Muting**: All microphone frame accumulation is suppressed while `isAgentSpeaking === true`.
- **$350\,\text{ms}$ Reverberation Cooldown**: When audio playback finishes, a $350\,\text{ms}$ lock (`agentSpeakingCooldownUntil`) absorbs lingering room acoustic decay.

---

## 4. Multi-Workflow State Machine Matrix

The orchestrator maintains 7 business workflows with slot persistence across conversational turns:

| Workflow ID | Workflow Name | Initial Node | Key Extracted Slots | Terminal Node |
| :--- | :--- | :--- | :--- | :--- |
| `reception_dispatcher` | Clinic Front Desk Reception | `welcome_reception` | `inquiry_type`, `department` | `routed_to_service` |
| `appointment_scheduling` | Doctor Appointment Scheduling | `select_department` | `department`, `date`, `time_slot`, `patient_name`, `booking_id` | `appointment_confirmed` |
| `prescription_refill` | Prescription Refill & Pharmacy | `lookup_rx` | `medication`, `rx_number`, `fulfillment_method`, `refill_order_id` | `refill_confirmed` |
| `lab_results` | Diagnostic Lab & Blood Panel | `patient_verification` | `patient_id`, `test_status`, `reviewed_by` | `review_results` |
| `billing_inquiry` | Billing Statement & Copay | `invoice_lookup` | `invoice_id`, `copay_due`, `auth_code`, `balance` | `billing_completed` |
| `reschedule_cancel` | Manage Existing Appointment | `lookup_booking` | `booking_id`, `cancellation_reason`, `refund_status` | `management_complete` |
| `clinical_triage` | Acute Symptom Triage | `symptom_intake` | `symptoms`, `acuity`, `triage_level`, `recommendation` | `urgent_routing` |

---

## 5. Concurrency & Thread Safety Model

```go
type VoicePipelineEngine struct {
    WhisperURL string
    ClefURL    string
    TTSURL     string
    LLMURL     string
    Tools      *ToolRegistry
    Workflow   *MultiWorkflowEngine
    LLM        *LLMClient
    client     *http.Client
    mu         sync.Mutex          // Guards conversational history and multi-turn state
    history    []map[string]string // Rolling conversation memory (bounded to last 6 turns)
}
```

1. **State Isolation**: Every incoming turn acquires a thread-safe mutex lock `p.mu.Lock()` when querying or updating rolling dialogue history.
2. **Context Bounding**: Conversation history is capped at the last 6 turns (`history[len-6:]`) to maintain high GPU throughput and avoid context-bloat latency degradation.
3. **Stateless Tool Dispatch**: All tools in `agent.DefaultTools` execute pure in-memory state lookups without disk I/O bottlenecks.
