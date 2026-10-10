//! End-to-End Audio / Text Pipeline Engine with Pipelined Clause-to-TTS Streaming

use crate::agent::providers::*;
use crate::agent::tools::{ToolExecutionResult, ToolRegistry};
use crate::agent::workflow::{ConsolidatedWorkflowEngine, UnifiedSessionState};
use crate::dsp::simd;
use base64::Engine;
use parking_lot::RwLock;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EndToEndVoiceResult {
    pub input_audio_bytes: usize,
    pub transcribed_text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stt_provider_name: Option<String>,
    pub asr_latency_ms: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub system_one_provider_name: Option<String>,
    pub clef_decision_latency_ms: f64,
    pub decisions: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_state: Option<UnifiedSessionState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_executed: Option<ToolExecutionResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub llm_provider_name: Option<String>,
    pub llm_latency_ms: f64,
    pub generated_speech: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tts_provider_name: Option<String>,
    pub tts_latency_ms: f64,
    pub total_pipeline_latency_ms: f64,
    pub output_audio_bytes: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_base64: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub estimated_cost_inr: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quota_status: Option<crate::agent::auth::QuotaStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamingTurnChunk {
    #[serde(rename = "type")]
    pub chunk_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delta_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub clause_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audio_base64: Option<String>,
    pub chunk_index: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttft_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttfs_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tts_latency_ms: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_state: Option<UnifiedSessionState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<EndToEndVoiceResult>,
}

pub struct VoicePipelineEngine {
    pub config: Arc<RwLock<ProviderConfig>>,
    pub workflow: Arc<ConsolidatedWorkflowEngine>,
    pub tools: Arc<ToolRegistry>,
    pub knowledge: Arc<crate::agent::knowledge::KnowledgeBase>,
    pub dynamic_tools: Arc<crate::agent::dynamic_tools::DynamicToolRegistry>,
    pub db: Arc<crate::agent::db::DatabaseService>,
    pub auth: Arc<crate::agent::auth::AuthService>,
    pub history: Arc<RwLock<Vec<HashMap<String, String>>>>,
    pub stt: Arc<RwLock<Arc<dyn STTProvider>>>,
    pub system_one: Arc<RwLock<Arc<dyn SystemOneProvider>>>,
    pub llm: Arc<RwLock<Arc<dyn LLMProvider>>>,
    pub tts: Arc<RwLock<Arc<dyn TTSProvider>>>,
    client: Client,
}

impl VoicePipelineEngine {
    pub fn new(cfg: ProviderConfig) -> Self {
        let client = Client::builder()
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .tcp_keepalive(std::time::Duration::from_secs(30))
            .build()
            .unwrap_or_default();

        let db = Arc::new(
            crate::agent::db::DatabaseService::new("carrier_enterprise.db")
                .unwrap_or_else(|_| crate::agent::db::DatabaseService::new(":memory:").expect("DB init failed"))
        );

        let auth = Arc::new(crate::agent::auth::AuthService::new());
        let workflow = Arc::new(ConsolidatedWorkflowEngine::new());
        let tools = Arc::new(ToolRegistry::new());
        let knowledge = Arc::new(crate::agent::knowledge::KnowledgeBase::new());
        let dynamic_tools = Arc::new(crate::agent::dynamic_tools::DynamicToolRegistry::new());
        let history = Arc::new(RwLock::new(Vec::new()));
        let config = Arc::new(RwLock::new(cfg.clone()));

        Self {
            config: config.clone(),
            workflow,
            tools,
            knowledge,
            dynamic_tools,
            db,
            auth,
            history,
            stt: Arc::new(RwLock::new(Self::init_stt(&cfg, client.clone()))),
            system_one: Arc::new(RwLock::new(Self::init_system_one(&cfg, client.clone()))),
            llm: Arc::new(RwLock::new(Self::init_llm(&cfg, client.clone()))),
            tts: Arc::new(RwLock::new(Self::init_tts(&cfg, client.clone()))),
            client,
        }
    }

    fn init_stt(cfg: &ProviderConfig, client: Client) -> Arc<dyn STTProvider> {
        if cfg.stt_provider == "sarvam" && !cfg.sarvam_api_key.is_empty() {
            Arc::new(SarvamSTT::new(
                cfg.sarvam_api_key.clone(),
                cfg.sarvam_stt_model.clone(),
                cfg.sarvam_stt_lang.clone(),
                client,
            ))
        } else {
            Arc::new(LocalWhisperSTT::new(cfg.whisper_server_url.clone(), client))
        }
    }

    fn init_system_one(cfg: &ProviderConfig, client: Client) -> Arc<dyn SystemOneProvider> {
        if cfg.system_one_provider == "jev" && !cfg.jev_api_key.is_empty() {
            Arc::new(TypeSafeJevSystemOne::new(cfg.jev_api_key.clone(), "jev-latest".to_string(), client))
        } else {
            Arc::new(LocalClefSystemOne::new(cfg.clef_server_url.clone(), client))
        }
    }

    fn init_llm(cfg: &ProviderConfig, client: Client) -> Arc<dyn LLMProvider> {
        if cfg.llm_provider == "sarvam" && !cfg.sarvam_api_key.is_empty() {
            Arc::new(SarvamLLM::new(
                cfg.sarvam_api_key.clone(),
                cfg.sarvam_llm_model.clone(),
                client,
            ))
        } else {
            Arc::new(LocalLLM::new(cfg.llama_server_url.clone(), client))
        }
    }

    fn init_tts(cfg: &ProviderConfig, client: Client) -> Arc<dyn TTSProvider> {
        if cfg.tts_provider == "elevenlabs" && !cfg.elevenlabs_api_key.is_empty() {
            Arc::new(ElevenLabsTTS::new(
                cfg.elevenlabs_api_key.clone(),
                cfg.elevenlabs_voice_id.clone(),
                cfg.elevenlabs_model_id.clone(),
                client,
            ))
        } else if cfg.tts_provider == "sarvam" && !cfg.sarvam_api_key.is_empty() {
            Arc::new(SarvamTTS::new(
                cfg.sarvam_api_key.clone(),
                cfg.sarvam_tts_speaker.clone(),
                cfg.sarvam_tts_lang.clone(),
                cfg.sarvam_tts_model.clone(),
                client,
            ))
        } else {
            Arc::new(LocalKokoroTTS::new(
                cfg.tts_server_url.clone(),
                "af_heart".to_string(),
                client,
            ))
        }
    }

    pub fn update_config(&self, new_cfg: ProviderConfig) {
        *self.config.write() = new_cfg.clone();
        *self.stt.write() = Self::init_stt(&new_cfg, self.client.clone());
        *self.system_one.write() = Self::init_system_one(&new_cfg, self.client.clone());
        *self.llm.write() = Self::init_llm(&new_cfg, self.client.clone());
        *self.tts.write() = Self::init_tts(&new_cfg, self.client.clone());
    }

    pub fn reset_history(&self) {
        self.history.write().clear();
        self.workflow.reset_session(Some("beacon_reception_triage"));
    }

    pub async fn process_voice_turn(&self, audio_data: &[u8]) -> Result<(EndToEndVoiceResult, Vec<u8>), String> {
        let total_start = Instant::now();

        // SIMD 85Hz highpass filter & clean WAV encapsulation
        let preprocessed = simd::preprocess_audio_pcm16_or_wav(audio_data);

        // 1. SIMD ASR Transcribe
        let (transcribed_text, asr_lat) = {
            let stt = self.stt.read().clone();
            stt.transcribe(&preprocessed).await.unwrap_or_else(|e| {
                tracing::warn!("STT transcription fallback triggered: {}", e);
                ("".to_string(), 0.0)
            })
        };

        if transcribed_text.trim().is_empty() {
            let snap = self.workflow.get_snapshot();
            let res = EndToEndVoiceResult {
                input_audio_bytes: audio_data.len(),
                transcribed_text: "".to_string(),
                stt_provider_name: Some(self.stt.read().name().to_string()),
                asr_latency_ms: asr_lat,
                system_one_provider_name: None,
                clef_decision_latency_ms: 0.0,
                decisions: HashMap::new(),
                workflow_state: Some(snap),
                tool_executed: None,
                llm_provider_name: None,
                llm_latency_ms: 0.0,
                generated_speech: "".to_string(),
                tts_provider_name: None,
                tts_latency_ms: 0.0,
                total_pipeline_latency_ms: total_start.elapsed().as_micros() as f64 / 1000.0,
                output_audio_bytes: 0,
                audio_base64: None,
                estimated_cost_inr: Some(0.0),
                quota_status: None,
            };
            return Ok((res, Vec::new()));
        }

        self.process_semantic_turn(&transcribed_text, audio_data.len(), asr_lat, total_start).await
    }

    pub async fn process_text_turn(&self, text: &str) -> Result<(EndToEndVoiceResult, Vec<u8>), String> {
        let total_start = Instant::now();
        self.process_semantic_turn(text, 0, 0.0, total_start).await
    }

    async fn process_semantic_turn(
        &self,
        transcribed_text: &str,
        input_audio_bytes: usize,
        asr_lat: f64,
        total_start: Instant,
    ) -> Result<(EndToEndVoiceResult, Vec<u8>), String> {
        let questions = Self::build_system_one_questions();

        let (mut decisions, clef_lat) = {
            let sys_one = self.system_one.read().clone();
            match sys_one.decide(transcribed_text, questions).await {
                Ok(res) => res,
                Err(e) => {
                    tracing::warn!("System One error, using fast heuristic: {}", e);
                    (Self::fast_semantic_extract(transcribed_text), 0.1)
                }
            }
        };

        // Fallback checks
        if decisions.get("intent").map(|s| s.as_str()).unwrap_or("conversational_ack") == "conversational_ack" {
            let heuristic = Self::fast_semantic_extract(transcribed_text);
            if heuristic.get("intent").map(|s| s.as_str()) == Some("call_completion") {
                decisions.insert("intent".to_string(), "call_completion".to_string());
            }
        }

        let (tool_res, tool_context) = self.execute_business_routing(transcribed_text, &decisions);
        let latest_snap = self.workflow.get_snapshot();

        let kb_context = self.knowledge.build_prompt_context(transcribed_text);

        // LLM Generation
        let system_prompt = format!(
            "You are Aria, an empathetic, conversational real-time phone receptionist at Beacon Health clinic.\n\
             Always be polite, warm, and concise (1-2 short natural spoken sentences max).\n\
             CRITICAL SPOKEN RULE: NEVER begin your response with 'Hello', 'Hi', 'Hi there', 'Hey', 'Welcome to Beacon Health', or 'I\\'m Aria' unless this is the very first turn of the conversation. On all ongoing turns, jump directly into answering the caller's request.\n\
             When the caller asks multiple questions, address all their requests in your single response.\n\
             Never use bullet points, markdown formatting, or numbered lists.\n\n\
             {}\n{}",
            self.workflow.get_llm_directives(),
            kb_context
        );

        let mut messages = Vec::new();
        messages.push([("role".to_string(), "system".to_string()), ("content".to_string(), system_prompt)].into_iter().collect());

        {
            let hist = self.history.read();
            let start = if hist.len() > 6 { hist.len() - 6 } else { 0 };
            for msg in &hist[start..] {
                messages.push(msg.clone());
            }
        }

        let user_content = if !tool_context.is_empty() {
            format!("Caller: \"{}\"\nClinic System Execution: {}\nRespond to caller:", transcribed_text, tool_context)
        } else {
            format!("Caller: \"{}\"\nRespond to caller:", transcribed_text)
        };
        messages.push([("role".to_string(), "user".to_string()), ("content".to_string(), user_content)].into_iter().collect());

        let (speech_text, llm_lat) = {
            let llm = self.llm.read().clone();
            llm.generate_chat(messages, 0.4, 90).await.unwrap_or_else(|_| {
                ("I understand. How can I assist you with your clinic request today?".to_string(), 0.0)
            })
        };

        // Record history
        {
            let mut hist = self.history.write();
            hist.push([("role".to_string(), "user".to_string()), ("content".to_string(), transcribed_text.to_string())].into_iter().collect());
            hist.push([("role".to_string(), "assistant".to_string()), ("content".to_string(), speech_text.clone())].into_iter().collect());
        }

        // TTS Synthesis
        let (out_wav, tts_lat) = {
            let tts = self.tts.read().clone();
            tts.synthesize(&speech_text, None, 1.05).await.unwrap_or_default()
        };

        let total_ms = total_start.elapsed().as_micros() as f64 / 1000.0;
        let b64 = if !out_wav.is_empty() {
            Some(format!("data:audio/wav;base64,{}", base64::engine::general_purpose::STANDARD.encode(&out_wav)))
        } else {
            None
        };

        let stt_name = self.stt.read().name().to_string();
        let sys_name = self.system_one.read().name().to_string();
        let llm_name = self.llm.read().name().to_string();
        let tts_name = self.tts.read().name().to_string();

        // Calculate Cost in INR
        let duration_secs = if input_audio_bytes > 0 { (input_audio_bytes as f64 / 32000.0).max(1.0) } else { 2.0 };
        let stt_cost = if stt_name.contains("Sarvam") { duration_secs * crate::agent::auth::COST_PER_STT_SECOND_INR } else { 0.0 };
        let est_tokens = (transcribed_text.len() + speech_text.len()) as u64 / 4;
        let llm_cost = (est_tokens as f64 / 1000.0) * crate::agent::auth::COST_PER_1K_LLM_TOKENS_INR;
        let tts_chars = speech_text.len();
        let tts_cost = if tts_name.contains("Sarvam") { (tts_chars as f64 / 1000.0) * crate::agent::auth::COST_PER_TTS_1K_CHARS_INR } else { 0.0 };
        let turn_cost_inr = stt_cost + llm_cost + tts_cost;

        // Record usage against default guest user or active session
        let default_user = self.db.get_user_by_id("usr_guest_001").ok().flatten();
        let quota_status = if let Some(ref u) = default_user {
            let _ = self.db.record_usage(&u.id, turn_cost_inr, duration_secs as u64, est_tokens, 0, 0);
            if let Ok(Some(ref updated_u)) = self.db.get_user_by_id(&u.id) {
                Some(self.auth.check_quota(updated_u))
            } else {
                Some(self.auth.check_quota(u))
            }
        } else {
            None
        };

        let result = EndToEndVoiceResult {
            input_audio_bytes,
            transcribed_text: transcribed_text.to_string(),
            stt_provider_name: Some(stt_name),
            asr_latency_ms: asr_lat,
            system_one_provider_name: Some(sys_name),
            clef_decision_latency_ms: clef_lat,
            decisions,
            workflow_state: Some(latest_snap),
            tool_executed: tool_res,
            llm_provider_name: Some(llm_name),
            llm_latency_ms: llm_lat,
            generated_speech: speech_text,
            tts_provider_name: Some(tts_name),
            tts_latency_ms: tts_lat,
            total_pipeline_latency_ms: total_ms,
            output_audio_bytes: out_wav.len(),
            audio_base64: b64,
            estimated_cost_inr: Some(turn_cost_inr),
            quota_status,
        };

        Ok((result, out_wav))
    }

    pub async fn process_turn_stream(
        &self,
        text_opt: Option<&str>,
        audio_opt: Option<&[u8]>,
        chunk_tx: UnboundedSender<StreamingTurnChunk>,
    ) -> Result<EndToEndVoiceResult, String> {
        let total_start = Instant::now();

        // 1. Transcribe or use provided text
        let (transcribed_text, asr_lat) = if let Some(text) = text_opt {
            (text.to_string(), 0.0)
        } else if let Some(audio_bytes) = audio_opt {
            let stt = self.stt.read().clone();
            stt.transcribe(audio_bytes).await.unwrap_or_else(|e| {
                tracing::warn!("Streaming STT error: {}", e);
                ("".to_string(), 0.0)
            })
        } else {
            return Err("No text or audio provided".to_string());
        };

        if transcribed_text.trim().is_empty() {
            let snap = self.workflow.get_snapshot();
            let stt_name = self.stt.read().name().to_string();
            let res = EndToEndVoiceResult {
                input_audio_bytes: audio_opt.map(|a| a.len()).unwrap_or(0),
                transcribed_text: "".to_string(),
                stt_provider_name: Some(stt_name),
                asr_latency_ms: asr_lat,
                system_one_provider_name: None,
                clef_decision_latency_ms: 0.0,
                decisions: HashMap::new(),
                workflow_state: Some(snap),
                tool_executed: None,
                llm_provider_name: None,
                llm_latency_ms: 0.0,
                generated_speech: "".to_string(),
                tts_provider_name: None,
                tts_latency_ms: 0.0,
                total_pipeline_latency_ms: total_start.elapsed().as_micros() as f64 / 1000.0,
                output_audio_bytes: 0,
                audio_base64: None,
                estimated_cost_inr: Some(0.0),
                quota_status: None,
            };
            let _ = chunk_tx.send(StreamingTurnChunk {
                chunk_type: "final".to_string(),
                delta_text: None,
                clause_text: None,
                audio_base64: None,
                chunk_index: 0,
                ttft_ms: None,
                ttfs_ms: None,
                tts_latency_ms: None,
                workflow_state: res.workflow_state.clone(),
                result: Some(res.clone()),
            });
            return Ok(res);
        }

        let questions = Self::build_system_one_questions();
        let (mut decisions, clef_lat) = {
            let sys_one = self.system_one.read().clone();
            match sys_one.decide(&transcribed_text, questions).await {
                Ok(res) => res,
                Err(e) => {
                    tracing::warn!("Stream System One error, fast fallback: {}", e);
                    (Self::fast_semantic_extract(&transcribed_text), 0.1)
                }
            }
        };

        if decisions.get("intent").map(|s| s.as_str()).unwrap_or("conversational_ack") == "conversational_ack" {
            let heuristic = Self::fast_semantic_extract(&transcribed_text);
            if heuristic.get("intent").map(|s| s.as_str()) == Some("call_completion") {
                decisions.insert("intent".to_string(), "call_completion".to_string());
            }
        }

        let (tool_res, tool_context) = self.execute_business_routing(&transcribed_text, &decisions);
        let latest_snap = self.workflow.get_snapshot();

        let stt_name = self.stt.read().name().to_string();
        let sys_name = self.system_one.read().name().to_string();
        let llm_name = self.llm.read().name().to_string();
        let tts_name = self.tts.read().name().to_string();

        let initial_meta = EndToEndVoiceResult {
            input_audio_bytes: audio_opt.map(|a| a.len()).unwrap_or(0),
            transcribed_text: transcribed_text.clone(),
            stt_provider_name: Some(stt_name),
            asr_latency_ms: asr_lat,
            system_one_provider_name: Some(sys_name),
            clef_decision_latency_ms: clef_lat,
            decisions: decisions.clone(),
            workflow_state: Some(latest_snap.clone()),
            tool_executed: tool_res.clone(),
            llm_provider_name: Some(llm_name),
            llm_latency_ms: 0.0,
            generated_speech: "".to_string(),
            tts_provider_name: Some(tts_name),
            tts_latency_ms: 0.0,
            total_pipeline_latency_ms: 0.0,
            output_audio_bytes: 0,
            audio_base64: None,
            estimated_cost_inr: None,
            quota_status: None,
        };

        let _ = chunk_tx.send(StreamingTurnChunk {
            chunk_type: "meta".to_string(),
            delta_text: None,
            clause_text: None,
            audio_base64: None,
            chunk_index: 0,
            ttft_ms: None,
            ttfs_ms: None,
            tts_latency_ms: None,
            workflow_state: Some(latest_snap),
            result: Some(initial_meta),
        });

        // 4. Construct Prompt Messages with RAG Grounding
        let kb_context = self.knowledge.build_prompt_context(&transcribed_text);
        let system_prompt = format!(
            "You are Aria, an empathetic, conversational real-time phone receptionist at Beacon Health clinic.\n\
             Always be polite, warm, and concise (1-2 short natural spoken sentences max).\n\
             CRITICAL SPOKEN RULE: NEVER begin your response with 'Hello', 'Hi', 'Hi there', 'Hey', 'Welcome to Beacon Health', or 'I\\'m Aria' unless this is the very first turn of the conversation. On all ongoing turns, jump directly into answering the caller's request.\n\
             When the caller asks multiple questions, address all their requests in your single response.\n\
             Never use bullet points, markdown formatting, or numbered lists.\n\n\
             {}\n{}",
            self.workflow.get_llm_directives(),
            kb_context
        );

        let mut messages = Vec::new();
        messages.push([("role".to_string(), "system".to_string()), ("content".to_string(), system_prompt)].into_iter().collect());

        {
            let hist = self.history.read();
            let start = if hist.len() > 6 { hist.len() - 6 } else { 0 };
            for msg in &hist[start..] {
                messages.push(msg.clone());
            }
        }

        let user_content = if !tool_context.is_empty() {
            format!("Caller: \"{}\"\nClinic System Execution: {}\nRespond to caller:", transcribed_text, tool_context)
        } else {
            format!("Caller: \"{}\"\nRespond to caller:", transcribed_text)
        };
        messages.push([("role".to_string(), "user".to_string()), ("content".to_string(), user_content)].into_iter().collect());

        // 5. Pipelined Streaming: LLM Tokens -> Clause Buffer -> Async TTS Chunk Synthesis
        let (token_tx, mut token_rx) = unbounded_channel::<String>();
        let llm_arc = self.llm.read().clone();
        let llm_handle = {
            let messages_clone = messages.clone();
            tokio::spawn(async move {
                llm_arc.generate_chat_stream(messages_clone, 0.4, 90, token_tx).await
            })
        };

        let mut clause_buf = String::with_capacity(128);
        let mut full_text = String::with_capacity(256);
        let mut chunk_idx = 0;
        let mut first_token_time: Option<Instant> = None;
        let mut ttft_ms = 0.0;
        let mut total_tts_lat = 0.0;

        while let Some(tok) = token_rx.recv().await {
            if first_token_time.is_none() {
                first_token_time = Some(Instant::now());
                ttft_ms = total_start.elapsed().as_micros() as f64 / 1000.0;
            }

            full_text.push_str(&tok);
            clause_buf.push_str(&tok);

            let _ = chunk_tx.send(StreamingTurnChunk {
                chunk_type: "text_delta".to_string(),
                delta_text: Some(tok.clone()),
                clause_text: None,
                audio_base64: None,
                chunk_index: chunk_idx,
                ttft_ms: Some(ttft_ms),
                ttfs_ms: None,
                tts_latency_ms: None,
                workflow_state: None,
                result: None,
            });

            // Clause boundary detection
            let current_clause = clause_buf.trim();
            let words: Vec<&str> = current_clause.split_whitespace().collect();
            let is_terminal = tok.contains('.') || tok.contains('?') || tok.contains('!') || tok.contains('\n');
            let is_clause = words.len() >= 4 && (tok.contains(',') || tok.contains(';') || tok.contains(':') || tok.contains('-'));

            if (is_terminal || is_clause) && words.len() >= 2 {
                let clause_to_synthesize = current_clause.to_string();
                clause_buf.clear();

                let (wav_chunk, chunk_tts_lat) = {
                    let tts = self.tts.read().clone();
                    tts.synthesize(&clause_to_synthesize, None, 1.05).await.unwrap_or_default()
                };

                if !wav_chunk.is_empty() {
                    total_tts_lat += chunk_tts_lat;
                    chunk_idx += 1;
                    let ttfs_ms = total_start.elapsed().as_micros() as f64 / 1000.0;
                    let b64 = format!("data:audio/wav;base64,{}", base64::engine::general_purpose::STANDARD.encode(&wav_chunk));

                    let _ = chunk_tx.send(StreamingTurnChunk {
                        chunk_type: "audio_chunk".to_string(),
                        delta_text: None,
                        clause_text: Some(clause_to_synthesize),
                        audio_base64: Some(b64),
                        chunk_index: chunk_idx,
                        ttft_ms: Some(ttft_ms),
                        ttfs_ms: Some(ttfs_ms),
                        tts_latency_ms: Some(chunk_tts_lat),
                        workflow_state: None,
                        result: None,
                    });
                }
            }
        }

        let (final_speech_text, llm_lat) = match llm_handle.await {
            Ok(Ok((txt, lat))) => {
                let chosen = if !full_text.is_empty() { full_text.clone() } else { txt };
                (chosen, lat)
            }
            _ => (
                if !full_text.is_empty() { full_text.clone() } else { "I understand. How can I assist you with your appointment today?".to_string() },
                0.0,
            ),
        };

        // Synthesize remaining clause buffer
        let rem = clause_buf.trim().to_string();
        if !rem.is_empty() {
            let (wav_chunk, chunk_tts_lat) = {
                let tts = self.tts.read().clone();
                tts.synthesize(&rem, None, 1.05).await.unwrap_or_default()
            };

            if !wav_chunk.is_empty() {
                total_tts_lat += chunk_tts_lat;
                chunk_idx += 1;
                let ttfs_ms = total_start.elapsed().as_micros() as f64 / 1000.0;
                let b64 = format!("data:audio/wav;base64,{}", base64::engine::general_purpose::STANDARD.encode(&wav_chunk));

                let _ = chunk_tx.send(StreamingTurnChunk {
                    chunk_type: "audio_chunk".to_string(),
                    delta_text: None,
                    clause_text: Some(rem),
                    audio_base64: Some(b64),
                    chunk_index: chunk_idx,
                    ttft_ms: Some(ttft_ms),
                    ttfs_ms: Some(ttfs_ms),
                    tts_latency_ms: Some(chunk_tts_lat),
                    workflow_state: None,
                    result: None,
                });
            }
        }

        // Record history
        {
            let mut hist = self.history.write();
            hist.push([("role".to_string(), "user".to_string()), ("content".to_string(), transcribed_text.to_string())].into_iter().collect());
            hist.push([("role".to_string(), "assistant".to_string()), ("content".to_string(), final_speech_text.clone())].into_iter().collect());
        }

        let stt_final_name = self.stt.read().name().to_string();
        let sys_final_name = self.system_one.read().name().to_string();
        let llm_final_name = self.llm.read().name().to_string();
        let tts_final_name = self.tts.read().name().to_string();

        let in_bytes = audio_opt.map(|a| a.len()).unwrap_or(0);
        let duration_secs = if in_bytes > 0 { (in_bytes as f64 / 32000.0).max(1.0) } else { 2.0 };
        let stt_cost = if stt_final_name.contains("Sarvam") { duration_secs * crate::agent::auth::COST_PER_STT_SECOND_INR } else { 0.0 };
        let est_tokens = (transcribed_text.len() + final_speech_text.len()) as u64 / 4;
        let llm_cost = (est_tokens as f64 / 1000.0) * crate::agent::auth::COST_PER_1K_LLM_TOKENS_INR;
        let tts_chars = final_speech_text.len();
        let tts_cost = if tts_final_name.contains("Sarvam") { (tts_chars as f64 / 1000.0) * crate::agent::auth::COST_PER_TTS_1K_CHARS_INR } else { 0.0 };
        let turn_cost_inr = stt_cost + llm_cost + tts_cost;

        let default_user = self.db.get_user_by_id("usr_guest_001").ok().flatten();
        let quota_status = if let Some(ref u) = default_user {
            let _ = self.db.record_usage(&u.id, turn_cost_inr, duration_secs as u64, est_tokens, 0, 0);
            if let Ok(Some(ref updated_u)) = self.db.get_user_by_id(&u.id) {
                Some(self.auth.check_quota(updated_u))
            } else {
                Some(self.auth.check_quota(u))
            }
        } else {
            None
        };

        let total_ms = total_start.elapsed().as_micros() as f64 / 1000.0;
        let final_result = EndToEndVoiceResult {
            input_audio_bytes: in_bytes,
            transcribed_text: transcribed_text.clone(),
            stt_provider_name: Some(stt_final_name),
            asr_latency_ms: asr_lat,
            system_one_provider_name: Some(sys_final_name),
            clef_decision_latency_ms: clef_lat,
            decisions,
            workflow_state: Some(self.workflow.get_snapshot()),
            tool_executed: tool_res,
            llm_provider_name: Some(llm_final_name),
            llm_latency_ms: llm_lat,
            generated_speech: final_speech_text,
            tts_provider_name: Some(tts_final_name),
            tts_latency_ms: total_tts_lat,
            total_pipeline_latency_ms: total_ms,
            output_audio_bytes: 0,
            audio_base64: None,
            estimated_cost_inr: Some(turn_cost_inr),
            quota_status,
        };

        let _ = chunk_tx.send(StreamingTurnChunk {
            chunk_type: "final".to_string(),
            delta_text: None,
            clause_text: None,
            audio_base64: None,
            chunk_index: chunk_idx,
            ttft_ms: Some(ttft_ms),
            ttfs_ms: None,
            tts_latency_ms: Some(total_tts_lat),
            workflow_state: final_result.workflow_state.clone(),
            result: Some(final_result.clone()),
        });

        Ok(final_result)
    }

    fn execute_business_routing(
        &self,
        _transcribed_text: &str,
        decisions: &HashMap<String, String>,
    ) -> (Option<ToolExecutionResult>, String) {
        let detected_intent = decisions.get("intent").map(|s| s.as_str()).unwrap_or("conversational_ack");
        let dept = decisions.get("department").map(|s| s.as_str()).unwrap_or("None");
        let chosen_slot = decisions.get("time_slot").map(|s| s.as_str()).unwrap_or("None");
        let fulfillment = decisions.get("fulfillment").map(|s| s.as_str()).unwrap_or("None");
        let pay_bill = decisions.get("pay_bill").map(|s| s.as_str()) == Some("yes");

        let curr_flow = self.workflow.get_snapshot().active_workflow_id;
        let is_call_completion = detected_intent == "call_completion";
        let is_doctor_inquiry = detected_intent == "doctor_credentials";
        let is_services_inquiry = detected_intent == "services_catalog";
        let is_refill_inquiry = detected_intent == "refill_medication" || decisions.get("also_refill").map(|s| s.as_str()) == Some("yes");
        let is_lab_inquiry = detected_intent == "check_lab_results";
        let is_billing_inquiry = detected_intent == "billing_payment";
        let is_reschedule_inquiry = detected_intent == "reschedule_cancel";
        let is_urgent_inquiry = detected_intent == "urgent_symptoms";
        let is_booking_inquiry = detected_intent == "book_appointment" || decisions.get("also_booking").map(|s| s.as_str()) == Some("yes");
        let is_greeting = detected_intent == "greeting_initial" && self.history.read().is_empty();

        let mut tool_executed: Option<ToolExecutionResult> = None;
        let mut tool_context_parts: Vec<String> = Vec::new();

        if is_call_completion {
            self.workflow.transition("call_completed");
            tool_context_parts.push("CALL COMPLETION DIRECTIVE: Caller confirmed no further help is needed or said goodbye. Directives: Warmly thank the caller for choosing Beacon Health, wish them a wonderful day, and conclude politely. STRICT SPOKEN RULE: Do NOT ask any questions, do NOT request names, and do NOT bring up appointments or pending tasks.".to_string());
        }

        if is_doctor_inquiry {
            let doc_query = if dept == "Dermatology" {
                "Dr. Sarah Lin"
            } else if dept == "Dental" {
                "Dr. Arthur Pendelton"
            } else if dept == "Pediatrics" {
                "Dr. Maya Patel"
            } else if dept == "General" {
                "Dr. Elena Rostova"
            } else {
                "Dr. Marcus Vance"
            };

            let mut p = HashMap::new();
            p.insert("doctor_name".to_string(), json!(doc_query));
            let t_res = self.tools.execute("lookup_doctor_profile", p);
            if tool_executed.is_none() {
                tool_executed = Some(t_res);
            }
            tool_context_parts.push(format!("Doctor Profile: {}.", doc_query));
        }

        if is_services_inquiry {
            let t_res = self.tools.execute("list_clinic_services", HashMap::new());
            if tool_executed.is_none() {
                tool_executed = Some(t_res);
            }
            tool_context_parts.push("Services Offered: Cardiology, Dermatology, Dental Care, Pediatrics, and General Medicine.".to_string());
        }

        if is_refill_inquiry {
            if curr_flow != "prescription_refill" {
                self.workflow.switch_workflow("prescription_refill");
            }
            self.workflow.set_slot("medication", json!("Lipitor (Atorvastatin 20mg)"));

            let mut p = HashMap::new();
            p.insert("patient_name".to_string(), json!("Caller"));
            p.insert("medication".to_string(), json!("Lipitor (Atorvastatin 20mg)"));
            p.insert("fulfillment".to_string(), json!(fulfillment));
            let t_res = self.tools.execute("verify_prescription_refill", p);
            if tool_executed.is_none() {
                tool_executed = Some(t_res);
            }

            if fulfillment == "Delivery" {
                self.workflow.set_slot("fulfillment_method", json!("Express Home Delivery"));
                self.workflow.transition("refill_confirmed");
                tool_context_parts.push("Prescription Refill: Lipitor 20mg approved for overnight home delivery. Arrives tomorrow.".to_string());
            } else if fulfillment == "Pickup" {
                self.workflow.set_slot("fulfillment_method", json!("Pharmacy Counter Pickup"));
                self.workflow.transition("refill_confirmed");
                tool_context_parts.push("Prescription Refill: Lipitor 20mg approved for same-day pickup at Beacon Pharmacy.".to_string());
            } else {
                self.workflow.transition("fulfillment_selection");
                tool_context_parts.push("Prescription Refill: Lipitor 20mg is approved. Express home delivery and same-day pharmacy pickup available.".to_string());
            }
        }

        if is_lab_inquiry {
            if curr_flow != "lab_results" {
                self.workflow.switch_workflow("lab_results");
            }
            self.workflow.set_slot("panel_type", json!("Lipid Panel"));
            self.workflow.transition("review_results");

            let mut p = HashMap::new();
            p.insert("patient_name".to_string(), json!("Caller"));
            p.insert("panel_type".to_string(), json!("Lipid Panel"));
            let t_res = self.tools.execute("get_recent_lab_results", p);
            if tool_executed.is_none() {
                tool_executed = Some(t_res);
            }
            tool_context_parts.push("Lab Results: Fasting Lipid Panel normal. Total Cholesterol 178 mg/dL, Triglycerides 124 mg/dL.".to_string());
        }

        if is_billing_inquiry {
            if curr_flow != "billing_inquiry" {
                self.workflow.switch_workflow("billing_inquiry");
            }
            self.workflow.set_slot("balance", json!("$35.00"));

            let mut p = HashMap::new();
            p.insert("patient_name".to_string(), json!("Caller"));
            p.insert("pay_now".to_string(), json!(pay_bill));
            let t_res = self.tools.execute("check_billing_balance", p);
            if tool_executed.is_none() {
                tool_executed = Some(t_res);
            }

            if pay_bill {
                self.workflow.transition("billing_completed");
                tool_context_parts.push("Billing: Outstanding copay of $35.00 settled successfully. Receipt SMS dispatched.".to_string());
            } else {
                self.workflow.transition("payment_review");
                tool_context_parts.push("Billing: Current balance is $35.00 for office visit copay.".to_string());
            }
        }

        if is_urgent_inquiry {
            if curr_flow != "clinical_triage" {
                self.workflow.switch_workflow("clinical_triage");
            }
            self.workflow.transition("urgent_routing");

            let mut p = HashMap::new();
            p.insert("symptom".to_string(), json!("Acute triage request"));
            let t_res = self.tools.execute("triage_symptom_urgency", p);
            if tool_executed.is_none() {
                tool_executed = Some(t_res);
            }
            tool_context_parts.push("URGENT Clinical Triage: Transferring to on-call triage RN.".to_string());
        }

        if (is_booking_inquiry
            || (curr_flow == "appointment_scheduling"
                && !is_services_inquiry
                && !is_doctor_inquiry
                && !is_refill_inquiry
                && !is_lab_inquiry
                && !is_billing_inquiry
                && !is_reschedule_inquiry
                && !is_urgent_inquiry))
            && !is_call_completion
        {
            if self.workflow.get_snapshot().active_workflow_id != "appointment_scheduling" {
                self.workflow.switch_workflow("appointment_scheduling");
            }

            let mut effective_dept = if dept != "None" { dept.to_string() } else { "".to_string() };
            let mut effective_slot = if chosen_slot != "None" { chosen_slot.to_string() } else { "".to_string() };

            let snap = self.workflow.get_snapshot();
            if effective_dept.is_empty() {
                if let Some(d) = snap.slots.get("department").and_then(|v| v.as_str()) {
                    effective_dept = d.to_string();
                }
            }
            if effective_slot.is_empty() {
                if let Some(s) = snap.slots.get("time_slot").and_then(|v| v.as_str()) {
                    effective_slot = s.to_string();
                }
            }

            if !effective_dept.is_empty() {
                self.workflow.set_slot("department", json!(effective_dept));
                self.workflow.set_slot("date", json!("Tomorrow"));

                let mut p = HashMap::new();
                p.insert("department".to_string(), json!(effective_dept));
                p.insert("date".to_string(), json!("Tomorrow"));
                let t_res = self.tools.execute("check_calendar_availability", p);
                let doctor = t_res.result["doctor"].as_str().unwrap_or("Dr. Marcus Vance");
                self.workflow.set_slot("doctor", json!(doctor));

                if !effective_slot.is_empty() {
                    self.workflow.set_slot("time_slot", json!(effective_slot));
                    self.workflow.transition("patient_intake");
                    tool_context_parts.push(format!("Time slot {} for {} reserved. Ask caller for patient name to confirm.", effective_slot, effective_dept));
                } else {
                    self.workflow.transition("select_time_slot");
                    tool_context_parts.push(format!("Available slots for {} tomorrow are 9:30 AM, 1:15 PM, and 4:00 PM. Ask caller which they prefer.", effective_dept));
                }
            } else {
                self.workflow.transition("select_department");
                tool_context_parts.push("Appointment Scheduling: Cardiology, Dermatology, Dental Care, Pediatrics, and General Medicine available.".to_string());
            }
        }

        let mut tool_context = tool_context_parts.join("\n");
        if tool_context.is_empty() && is_greeting {
            tool_context = "Greeting: Caller is saying hello. Directives: Greet warmly, confirm you hear clearly, and ask how you can help at Beacon Health today.".to_string();
        }

        (tool_executed, tool_context)
    }

    fn build_system_one_questions() -> HashMap<String, Value> {
        let mut q = HashMap::new();
        q.insert(
            "intent".to_string(),
            json!({
                "type": "choice",
                "instructions": "What is the primary action or topic requested by the caller?",
                "criteria": {
                    "book_appointment": "Schedule or book an appointment, check available time slots, or inquire about open times",
                    "doctor_credentials": "Inquire about doctor credentials, background, profile, or qualifications",
                    "services_catalog": "Inquire about what services, departments, or offerings exist",
                    "refill_medication": "Prescription refill, pharmacy inquiry, or home delivery / pickup fulfillment",
                    "check_lab_results": "Lab test, blood work, or diagnostic results",
                    "billing_payment": "Invoice, bill, copay payment, or insurance",
                    "reschedule_cancel": "Cancel, reschedule, or change an existing appointment",
                    "urgent_symptoms": "Report acute pain, fever, chest pain, or medical emergency",
                    "conversational_ack": "Providing a patient name, saying yes/no/okay/sure, or answering a question",
                    "greeting_initial": "Initial hello, checking audio connection (can you hear me), or opening greeting",
                    "call_completion": "Saying no more help is needed, that is all, nothing else, goodbye, bye, or concluding the call"
                }
            }),
        );
        q.insert(
            "also_refill".to_string(),
            json!({
                "type": "choice",
                "instructions": "Did the caller also request a prescription refill?",
                "criteria": { "yes": "Refill requested", "no": "No refill" }
            }),
        );
        q.insert(
            "also_booking".to_string(),
            json!({
                "type": "choice",
                "instructions": "Did the caller also request appointment booking or checking slots?",
                "criteria": { "yes": "Booking requested", "no": "No booking" }
            }),
        );
        q.insert(
            "department".to_string(),
            json!({
                "type": "choice",
                "instructions": "Which clinic department was mentioned or implied?",
                "criteria": {
                    "Cardiology": "Cardiology, heart clinic, or Dr. Marcus Vance",
                    "Dermatology": "Dermatology, skin clinic, or Dr. Sarah Lin",
                    "Dental": "Dental, teeth clinic, or Dr. Arthur Pendelton",
                    "Pediatrics": "Pediatrics, children clinic, or Dr. Maya Patel",
                    "General": "General family practice or Dr. Elena Rostova",
                    "None": "No specific department mentioned"
                }
            }),
        );
        q.insert(
            "time_slot".to_string(),
            json!({
                "type": "choice",
                "instructions": "Which appointment time was requested?",
                "criteria": {
                    "9:30 AM": "9:30 AM or morning",
                    "1:15 PM": "1:15 PM or afternoon",
                    "4:00 PM": "4:00 PM or evening",
                    "None": "No time slot specified"
                }
            }),
        );
        q.insert(
            "fulfillment".to_string(),
            json!({
                "type": "choice",
                "instructions": "Was delivery or counter pickup chosen?",
                "criteria": {
                    "Delivery": "Home delivery, mail, or overnight",
                    "Pickup": "Pickup in clinic, counter, or pharmacy pickup",
                    "None": "No method mentioned"
                }
            }),
        );
        q.insert(
            "pay_bill".to_string(),
            json!({
                "type": "choice",
                "instructions": "Did the caller ask to pay or settle the bill now?",
                "criteria": { "yes": "Pay now", "no": "Just checking balance" }
            }),
        );
        q.insert(
            "sentiment".to_string(),
            json!({
                "type": "choice",
                "instructions": "What is the caller's emotional sentiment / state?",
                "criteria": {
                    "positive": "Friendly, polite, thankful, or satisfied",
                    "neutral": "Routine inquiry, calm informational request",
                    "urgent": "Distressed, in pain, requesting rapid help, emergency",
                    "frustrated": "Annoyed, impatient, complaining, or dissatisfied"
                }
            }),
        );

        q
    }

    pub fn fast_semantic_extract(text: &str) -> HashMap<String, String> {
        let lower = text.to_lowercase();
        let mut d = HashMap::new();
        d.insert("intent".to_string(), "conversational_ack".to_string());
        d.insert("also_refill".to_string(), "no".to_string());
        d.insert("also_booking".to_string(), "no".to_string());
        d.insert("department".to_string(), "None".to_string());
        d.insert("time_slot".to_string(), "None".to_string());
        d.insert("fulfillment".to_string(), "None".to_string());
        d.insert("pay_bill".to_string(), "no".to_string());
        d.insert("sentiment".to_string(), "neutral".to_string());

        if lower.contains("thank") || lower.contains("great") || lower.contains("awesome") || lower.contains("perfect") || lower.contains("good") {
            d.insert("sentiment".to_string(), "positive".to_string());
        } else if lower.contains("urgent") || lower.contains("emergency") || lower.contains("pain") || lower.contains("severe") || lower.contains("bleeding") {
            d.insert("sentiment".to_string(), "urgent".to_string());
        } else if lower.contains("frustrat") || lower.contains("annoy") || lower.contains("angry") || lower.contains("terrible") || lower.contains("bad") {
            d.insert("sentiment".to_string(), "frustrated".to_string());
        }

        if lower.contains("cardio") || lower.contains("heart") || lower.contains("vance") {
            d.insert("department".to_string(), "Cardiology".to_string());
        } else if lower.contains("derm") || lower.contains("skin") || lower.contains("lin") {
            d.insert("department".to_string(), "Dermatology".to_string());
        } else if lower.contains("dent") || lower.contains("teeth") || lower.contains("pendelton") {
            d.insert("department".to_string(), "Dental".to_string());
        } else if lower.contains("pedia") || lower.contains("child") || lower.contains("patel") {
            d.insert("department".to_string(), "Pediatrics".to_string());
        } else if lower.contains("general") || lower.contains("family") || lower.contains("rostova") {
            d.insert("department".to_string(), "General".to_string());
        }

        if lower.contains("9:30") || lower.contains("9 30") || lower.contains("morning") {
            d.insert("time_slot".to_string(), "9:30 AM".to_string());
        } else if lower.contains("1:15") || lower.contains("1 15") || lower.contains("afternoon") {
            d.insert("time_slot".to_string(), "1:15 PM".to_string());
        } else if lower.contains("4:00") || lower.contains("4 pm") || lower.contains("evening") {
            d.insert("time_slot".to_string(), "4:00 PM".to_string());
        }

        if lower.contains("deliver") || lower.contains("home") || lower.contains("mail") {
            d.insert("fulfillment".to_string(), "Delivery".to_string());
        } else if lower.contains("pickup") || lower.contains("pick up") || lower.contains("pharmacy") {
            d.insert("fulfillment".to_string(), "Pickup".to_string());
        }

        if lower.contains("pay") || lower.contains("card") || lower.contains("settle") {
            d.insert("pay_bill".to_string(), "yes".to_string());
        }

        if lower.contains("refill") || lower.contains("prescription") || lower.contains("lipitor") {
            d.insert("also_refill".to_string(), "yes".to_string());
        }
        if lower.contains("book") || lower.contains("schedule") || lower.contains("appointment") {
            d.insert("also_booking".to_string(), "yes".to_string());
        }

        if lower.contains("no help")
            || lower.contains("no thanks")
            || lower.contains("nothing else")
            || lower.contains("that is all")
            || lower.contains("that's all")
            || lower.contains("all good")
            || lower.contains("all set")
            || lower.contains("goodbye")
            || lower.contains("bye")
            || lower.contains("already assisted")
            || lower.contains("no thank you")
        {
            d.insert("intent".to_string(), "call_completion".to_string());
        } else if lower.contains("emergency") || lower.contains("chest pain") || lower.contains("severe") {
            d.insert("intent".to_string(), "urgent_symptoms".to_string());
        } else if lower.contains("doctor") || lower.contains("credential") || lower.contains("who is") {
            d.insert("intent".to_string(), "doctor_credentials".to_string());
        } else if lower.contains("service") || lower.contains("what do you offer") {
            d.insert("intent".to_string(), "services_catalog".to_string());
        } else if lower.contains("lab") || lower.contains("blood test") || lower.contains("lipid") {
            d.insert("intent".to_string(), "check_lab_results".to_string());
        } else if lower.contains("bill") || lower.contains("copay") || lower.contains("invoice") {
            d.insert("intent".to_string(), "billing_payment".to_string());
        } else if lower.contains("cancel") || lower.contains("reschedule") {
            d.insert("intent".to_string(), "reschedule_cancel".to_string());
        } else if lower.contains("refill") || lower.contains("prescription") {
            d.insert("intent".to_string(), "refill_medication".to_string());
        } else if lower.contains("appointment") || lower.contains("schedule") || lower.contains("book") {
            d.insert("intent".to_string(), "book_appointment".to_string());
        } else if lower.contains("hello") || lower.contains("hi") || lower.contains("hey") {
            d.insert("intent".to_string(), "greeting_initial".to_string());
        }

        d
    }
}
