//! Model Provider Interfaces and Implementations
//!
//! Provides high-throughput async clients with connection pooling for:
//! - STT: Local Whisper.cpp (CUDA) / Sarvam Saaras v3 (Indic)
//! - LLM: Local Qwen3-4B (CUDA) / Sarvam-105B (Indic SSE Stream)
//! - TTS: Local Kokoro (CUDA) / Sarvam Bulbul v3 (Indic)
//! - System One: Local Clef-Flash / TypeSafe Jev API

use async_trait::async_trait;
use base64::Engine;
use futures_util::StreamExt;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub stt_provider: String,
    pub system_one_provider: String,
    pub llm_provider: String,
    pub tts_provider: String,
    #[serde(skip_serializing)]
    pub sarvam_api_key: String,
    #[serde(skip_serializing)]
    pub jev_api_key: String,
    #[serde(skip_serializing)]
    pub elevenlabs_api_key: String,
    pub elevenlabs_voice_id: String,
    pub elevenlabs_model_id: String,
    pub sarvam_stt_model: String,
    pub sarvam_stt_lang: String,
    pub sarvam_llm_model: String,
    pub sarvam_tts_model: String,
    pub sarvam_tts_speaker: String,
    pub sarvam_tts_lang: String,
    pub llama_server_url: String,
    pub whisper_server_url: String,
    pub clef_server_url: String,
    pub tts_server_url: String,
    pub vad_url: String,
}

impl Default for ProviderConfig {
    fn default() -> Self {
        Self {
            stt_provider: std::env::var("STT_PROVIDER").unwrap_or_else(|_| "sarvam".to_string()),
            system_one_provider: std::env::var("SYSTEM_ONE_PROVIDER").unwrap_or_else(|_| "jev".to_string()),
            llm_provider: std::env::var("LLM_PROVIDER").unwrap_or_else(|_| "sarvam".to_string()),
            tts_provider: std::env::var("TTS_PROVIDER").unwrap_or_else(|_| "sarvam".to_string()),
            sarvam_api_key: std::env::var("SARVAM_API_KEY").unwrap_or_default(),
            jev_api_key: std::env::var("JEV_API_KEY").unwrap_or_default(),
            elevenlabs_api_key: std::env::var("ELEVENLABS_API_KEY").unwrap_or_default(),
            elevenlabs_voice_id: std::env::var("ELEVENLABS_VOICE_ID").unwrap_or_else(|_| "21m00Tcm4TlvDq8ikWAM".to_string()), // Rachel
            elevenlabs_model_id: std::env::var("ELEVENLABS_MODEL_ID").unwrap_or_else(|_| "eleven_turbo_v2_5".to_string()),
            sarvam_stt_model: std::env::var("SARVAM_STT_MODEL").unwrap_or_else(|_| "saaras:v3".to_string()),
            sarvam_stt_lang: std::env::var("SARVAM_STT_LANG").unwrap_or_else(|_| "en-IN".to_string()),
            sarvam_llm_model: std::env::var("SARVAM_LLM_MODEL").unwrap_or_else(|_| "sarvam-105b-conversations".to_string()),
            sarvam_tts_model: std::env::var("SARVAM_TTS_MODEL").unwrap_or_else(|_| "bulbul:v3".to_string()),
            sarvam_tts_speaker: std::env::var("SARVAM_TTS_SPEAKER").unwrap_or_else(|_| "shubh".to_string()),
            sarvam_tts_lang: std::env::var("SARVAM_TTS_LANG").unwrap_or_else(|_| "en-IN".to_string()),
            llama_server_url: std::env::var("LLAMA_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string()),
            whisper_server_url: std::env::var("WHISPER_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:8082".to_string()),
            clef_server_url: std::env::var("CLEF_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:8085".to_string()),
            tts_server_url: std::env::var("TTS_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:8083".to_string()),
            vad_url: std::env::var("TTS_SERVER_URL").unwrap_or_else(|_| "http://127.0.0.1:8083".to_string()),
        }
    }
}

// ==========================================
// 1. STT Provider Trait
// ==========================================

#[async_trait]
pub trait STTProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn transcribe(&self, audio_wav: &[u8]) -> Result<(String, f64), String>;
}

pub struct LocalWhisperSTT {
    url: String,
    client: Client,
}

impl LocalWhisperSTT {
    pub fn new(url: String, client: Client) -> Self {
        Self { url, client }
    }
}

#[async_trait]
impl STTProvider for LocalWhisperSTT {
    fn name(&self) -> &str {
        "Local Whisper.cpp (CUDA)"
    }

    async fn transcribe(&self, audio_wav: &[u8]) -> Result<(String, f64), String> {
        let start = Instant::now();
        let part = reqwest::multipart::Part::bytes(audio_wav.to_vec())
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| e.to_string())?;

        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("response_format", "json")
            .text("prompt", "Yeah, yes, no, sure, okay, hello, Dr. Marcus Vance, appointment, prescription refill.");

        let resp = self
            .client
            .post(format!("{}/inference", self.url))
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("Whisper request failed: {}", e))?;

        let val: Value = resp.json().await.map_err(|e| e.to_string())?;
        let text = val.get("text").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let lat = start.elapsed().as_micros() as f64 / 1000.0;
        Ok((text, lat))
    }
}

pub struct SarvamSTT {
    api_key: String,
    model: String,
    lang: String,
    client: Client,
}

impl SarvamSTT {
    pub fn new(api_key: String, model: String, lang: String, client: Client) -> Self {
        Self { api_key, model, lang, client }
    }
}

#[async_trait]
impl STTProvider for SarvamSTT {
    fn name(&self) -> &str {
        "Sarvam AI Saaras (Indic)"
    }

    async fn transcribe(&self, audio_wav: &[u8]) -> Result<(String, f64), String> {
        let start = Instant::now();
        if self.api_key.is_empty() {
            return Err("SARVAM_API_KEY not configured".to_string());
        }

        let part = reqwest::multipart::Part::bytes(audio_wav.to_vec())
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|e| e.to_string())?;

        let form = reqwest::multipart::Form::new()
            .part("file", part)
            .text("model", self.model.clone())
            .text("language_code", self.lang.clone())
            .text("with_diarization", "false");

        let resp = self
            .client
            .post("https://api.sarvam.ai/speech-to-text")
            .header("api-subscription-key", &self.api_key)
            .multipart(form)
            .send()
            .await
            .map_err(|e| format!("Sarvam STT failed: {}", e))?;

        let val: Value = resp.json().await.map_err(|e| e.to_string())?;
        let text = val.get("transcript").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
        let lat = start.elapsed().as_micros() as f64 / 1000.0;
        Ok((text, lat))
    }
}

// ==========================================
// 2. LLM Provider Trait
// ==========================================

#[async_trait]
pub trait LLMProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn generate_chat(&self, messages: Vec<HashMap<String, String>>, temperature: f64, max_tokens: usize) -> Result<(String, f64), String>;
    async fn generate_chat_stream(
        &self,
        messages: Vec<HashMap<String, String>>,
        temperature: f64,
        max_tokens: usize,
        token_tx: UnboundedSender<String>,
    ) -> Result<(String, f64), String>;
}

pub struct LocalLLM {
    url: String,
    client: Client,
}

impl LocalLLM {
    pub fn new(url: String, client: Client) -> Self {
        Self { url, client }
    }
}

#[async_trait]
impl LLMProvider for LocalLLM {
    fn name(&self) -> &str {
        "Local Qwen3-4B (CUDA)"
    }

    async fn generate_chat(&self, messages: Vec<HashMap<String, String>>, temperature: f64, max_tokens: usize) -> Result<(String, f64), String> {
        let start = Instant::now();
        let payload = json!({
            "messages": messages,
            "temperature": temperature,
            "max_tokens": max_tokens
        });

        let resp = self
            .client
            .post(format!("{}/v1/chat/completions", self.url))
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Local LLM request failed: {}", e))?;

        let val: Value = resp.json().await.map_err(|e| e.to_string())?;
        let text = val["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("I understand. How can I assist you today?")
            .trim()
            .to_string();
        let lat = start.elapsed().as_micros() as f64 / 1000.0;
        Ok((text, lat))
    }

    async fn generate_chat_stream(
        &self,
        messages: Vec<HashMap<String, String>>,
        temperature: f64,
        max_tokens: usize,
        token_tx: UnboundedSender<String>,
    ) -> Result<(String, f64), String> {
        let start = Instant::now();
        let payload = json!({
            "messages": messages,
            "temperature": temperature,
            "max_tokens": max_tokens,
            "stream": true
        });

        let resp = self
            .client
            .post(format!("{}/v1/chat/completions", self.url))
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Local LLM stream failed: {}", e))?;

        let mut stream = resp.bytes_stream();
        let mut full_text = String::with_capacity(256);
        let mut line_buffer = String::new();

        while let Some(item) = stream.next().await {
            if let Ok(bytes) = item {
                if let Ok(chunk_str) = std::str::from_utf8(&bytes) {
                    line_buffer.push_str(chunk_str);
                    while let Some(pos) = line_buffer.find('\n') {
                        let line = line_buffer[..pos].trim().to_string();
                        line_buffer = line_buffer[pos + 1..].to_string();

                        if line.starts_with("data: ") {
                            let json_str = line.trim_start_matches("data: ").trim();
                            if json_str == "[DONE]" {
                                break;
                            }
                            if let Ok(v) = serde_json::from_str::<Value>(json_str) {
                                if let Some(tok) = v["choices"][0]["delta"]["content"].as_str() {
                                    full_text.push_str(tok);
                                    let _ = token_tx.send(tok.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        let lat = start.elapsed().as_micros() as f64 / 1000.0;
        Ok((full_text, lat))
    }
}

pub struct SarvamLLM {
    api_key: String,
    model: String,
    client: Client,
}

impl SarvamLLM {
    pub fn new(api_key: String, model: String, client: Client) -> Self {
        Self { api_key, model, client }
    }
}

#[async_trait]
impl LLMProvider for SarvamLLM {
    fn name(&self) -> &str {
        "Sarvam AI (sarvam-105b-conversations)"
    }

    async fn generate_chat(&self, messages: Vec<HashMap<String, String>>, temperature: f64, max_tokens: usize) -> Result<(String, f64), String> {
        let start = Instant::now();
        if self.api_key.is_empty() {
            return Err("SARVAM_API_KEY not set".to_string());
        }

        let payload = json!({
            "model": self.model,
            "messages": messages,
            "temperature": temperature,
            "max_tokens": max_tokens
        });

        let resp = self
            .client
            .post("https://api.sarvam.ai/v1/chat/completions")
            .header("api-subscription-key", &self.api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Sarvam LLM failed: {}", e))?;

        let val: Value = resp.json().await.map_err(|e| e.to_string())?;
        let text = val["choices"][0]["message"]["content"]
            .as_str()
            .unwrap_or("I understand. How can I assist you today?")
            .trim()
            .to_string();
        let lat = start.elapsed().as_micros() as f64 / 1000.0;
        Ok((text, lat))
    }

    async fn generate_chat_stream(
        &self,
        messages: Vec<HashMap<String, String>>,
        temperature: f64,
        max_tokens: usize,
        token_tx: UnboundedSender<String>,
    ) -> Result<(String, f64), String> {
        let start = Instant::now();
        if self.api_key.is_empty() {
            return Err("SARVAM_API_KEY not set".to_string());
        }

        let payload = json!({
            "model": self.model,
            "messages": messages,
            "temperature": temperature,
            "max_tokens": max_tokens,
            "stream": true
        });

        let resp = self
            .client
            .post("https://api.sarvam.ai/v1/chat/completions")
            .header("api-subscription-key", &self.api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Sarvam LLM stream failed: {}", e))?;

        let mut stream = resp.bytes_stream();
        let mut full_text = String::with_capacity(256);
        let mut line_buffer = String::new();

        while let Some(item) = stream.next().await {
            if let Ok(bytes) = item {
                if let Ok(chunk_str) = std::str::from_utf8(&bytes) {
                    line_buffer.push_str(chunk_str);
                    while let Some(pos) = line_buffer.find('\n') {
                        let line = line_buffer[..pos].trim().to_string();
                        line_buffer = line_buffer[pos + 1..].to_string();

                        if line.starts_with("data: ") {
                            let json_str = line.trim_start_matches("data: ").trim();
                            if json_str == "[DONE]" {
                                break;
                            }
                            if let Ok(v) = serde_json::from_str::<Value>(json_str) {
                                if let Some(tok) = v["choices"][0]["delta"]["content"].as_str() {
                                    full_text.push_str(tok);
                                    let _ = token_tx.send(tok.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        let lat = start.elapsed().as_micros() as f64 / 1000.0;
        Ok((full_text, lat))
    }
}

// ==========================================
// 3. TTS Provider Trait
// ==========================================

#[async_trait]
pub trait TTSProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn synthesize(&self, text: &str, voice: Option<&str>, speed: f64) -> Result<(Vec<u8>, f64), String>;
}

pub struct LocalKokoroTTS {
    url: String,
    voice: String,
    client: Client,
}

impl LocalKokoroTTS {
    pub fn new(url: String, voice: String, client: Client) -> Self {
        Self { url, voice, client }
    }
}

#[async_trait]
impl TTSProvider for LocalKokoroTTS {
    fn name(&self) -> &str {
        "Local Kokoro TTS (af_heart)"
    }

    async fn synthesize(&self, text: &str, voice: Option<&str>, speed: f64) -> Result<(Vec<u8>, f64), String> {
        let start = Instant::now();
        let payload = json!({
            "text": text,
            "voice": voice.unwrap_or(&self.voice),
            "speed": if speed <= 0.0 { 1.05 } else { speed }
        });

        let resp = self
            .client
            .post(format!("{}/v1/tts", self.url))
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Kokoro TTS failed: {}", e))?;

        let bytes = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
        let lat = start.elapsed().as_micros() as f64 / 1000.0;
        Ok((bytes, lat))
    }
}

pub struct SarvamTTS {
    api_key: String,
    speaker: String,
    lang: String,
    model: String,
    client: Client,
}

impl SarvamTTS {
    pub fn new(api_key: String, speaker: String, lang: String, model: String, client: Client) -> Self {
        Self { api_key, speaker, lang, model, client }
    }
}

#[async_trait]
impl TTSProvider for SarvamTTS {
    fn name(&self) -> &str {
        "Sarvam Bulbul (Indic)"
    }

    async fn synthesize(&self, text: &str, speaker: Option<&str>, speed: f64) -> Result<(Vec<u8>, f64), String> {
        let start = Instant::now();
        if self.api_key.is_empty() {
            return Err("SARVAM_API_KEY not set".to_string());
        }

        let payload = json!({
            "inputs": [text],
            "target_language_code": self.lang,
            "speaker": speaker.unwrap_or(&self.speaker),
            "pitch": 0,
            "pace": if speed <= 0.0 { 1.05 } else { speed },
            "loudness": 1.2,
            "speech_sample_rate": 16000,
            "enable_preprocessing": true,
            "model": self.model
        });

        let resp = self
            .client
            .post("https://api.sarvam.ai/text-to-speech")
            .header("api-subscription-key", &self.api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Sarvam TTS failed: {}", e))?;

        let val: Value = resp.json().await.map_err(|e| e.to_string())?;
        let b64_audio = val["audios"][0]
            .as_str()
            .or_else(|| val["audio"].as_str())
            .ok_or_else(|| "Sarvam TTS returned no audio data".to_string())?;

        let wav = base64::engine::general_purpose::STANDARD
            .decode(b64_audio)
            .map_err(|e| format!("Base64 decode error: {}", e))?;

        let lat = start.elapsed().as_micros() as f64 / 1000.0;
        Ok((wav, lat))
    }
}

pub struct ElevenLabsTTS {
    api_key: String,
    voice_id: String,
    model_id: String,
    client: Client,
}

impl ElevenLabsTTS {
    pub fn new(api_key: String, voice_id: String, model_id: String, client: Client) -> Self {
        Self { api_key, voice_id, model_id, client }
    }
}

#[async_trait]
impl TTSProvider for ElevenLabsTTS {
    fn name(&self) -> &str {
        "ElevenLabs High-Fidelity Voice"
    }

    async fn synthesize(&self, text: &str, voice: Option<&str>, speed: f64) -> Result<(Vec<u8>, f64), String> {
        let start = Instant::now();
        if self.api_key.is_empty() {
            return Err("ELEVENLABS_API_KEY not set".to_string());
        }

        let voice_id = voice.unwrap_or(&self.voice_id);
        let url = format!(
            "https://api.elevenlabs.io/v1/text-to-speech/{}?output_format=pcm_16000",
            voice_id
        );

        let payload = json!({
            "text": text,
            "model_id": self.model_id,
            "voice_settings": {
                "stability": 0.5,
                "similarity_boost": 0.8,
                "style": 0.0,
                "use_speaker_boost": true,
                "speed": if speed <= 0.0 { 1.0 } else { speed }
            }
        });

        let resp = self
            .client
            .post(&url)
            .header("xi-api-key", &self.api_key)
            .header(CONTENT_TYPE, "application/json")
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("ElevenLabs TTS network error: {}", e))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_txt = resp.text().await.unwrap_or_default();
            return Err(format!("ElevenLabs error {}: {}", status, err_txt));
        }

        let pcm_bytes = resp.bytes().await.map_err(|e| e.to_string())?.to_vec();
        let wav = crate::dsp::simd::encode_wav(&pcm_bytes, 16000, 1).to_vec();
        let lat = start.elapsed().as_micros() as f64 / 1000.0;
        Ok((wav, lat))
    }
}

// ==========================================
// 4. System One (Decision Engine) Provider Trait
// ==========================================

#[async_trait]
pub trait SystemOneProvider: Send + Sync {
    fn name(&self) -> &str;
    async fn decide(&self, state: &str, questions: HashMap<String, Value>) -> Result<(HashMap<String, String>, f64), String>;
}

pub struct LocalClefSystemOne {
    url: String,
    client: Client,
}

impl LocalClefSystemOne {
    pub fn new(url: String, client: Client) -> Self {
        Self { url, client }
    }
}

#[async_trait]
impl SystemOneProvider for LocalClefSystemOne {
    fn name(&self) -> &str {
        "Local Clef-Flash (Port 8085)"
    }

    async fn decide(&self, state: &str, questions: HashMap<String, Value>) -> Result<(HashMap<String, String>, f64), String> {
        let start = Instant::now();
        let payload = json!({
            "state": state,
            "questions": questions
        });

        let resp = self
            .client
            .post(format!("{}/v1/systemone", self.url))
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Clef Decision failed: {}", e))?;

        let val: Value = resp.json().await.map_err(|e| e.to_string())?;
        let mut decisions = HashMap::new();

        if let Some(answers) = val["answers"].as_object() {
            for (k, v) in answers {
                if let Some(choice) = v["choice"].as_str() {
                    decisions.insert(k.clone(), choice.to_string());
                }
            }
        }

        let lat = start.elapsed().as_micros() as f64 / 1000.0;
        Ok((decisions, lat))
    }
}

pub struct TypeSafeJevSystemOne {
    api_key: String,
    model: String,
    client: Client,
}

impl TypeSafeJevSystemOne {
    pub fn new(api_key: String, model: String, client: Client) -> Self {
        Self { api_key, model, client }
    }
}

#[async_trait]
impl SystemOneProvider for TypeSafeJevSystemOne {
    fn name(&self) -> &str {
        "TypeSafe Jev (Cloud)"
    }

    async fn decide(&self, state: &str, questions: HashMap<String, Value>) -> Result<(HashMap<String, String>, f64), String> {
        let start = Instant::now();
        if self.api_key.is_empty() {
            return Err("JEV_API_KEY not set".to_string());
        }

        let payload = json!({
            "state": state,
            "model": self.model,
            "questions": questions
        });

        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", self.api_key)).map_err(|e| e.to_string())?,
        );
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        let resp = self
            .client
            .post("https://api.typesafe.ai/v1/systemone")
            .headers(headers)
            .json(&payload)
            .send()
            .await
            .map_err(|e| format!("Jev Decision failed: {}", e))?;

        let val: Value = resp.json().await.map_err(|e| e.to_string())?;
        let mut decisions = HashMap::new();

        if let Some(answers) = val["answers"].as_object() {
            for (k, v) in answers {
                if let Some(choice) = v["choice"].as_str() {
                    decisions.insert(k.clone(), choice.to_string());
                }
            }
        }

        let lat = start.elapsed().as_micros() as f64 / 1000.0;
        Ok((decisions, lat))
    }
}
