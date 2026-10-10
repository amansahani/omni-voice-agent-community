//! Enterprise User Authentication, Quota Budgets & Admin Overrides
//!
//! Provides user accounts, session tokens, exact rupee cost accounting (Sarvam / LLM / TTS / STT),
//! cumulative call duration limits, and webhook/admin authorization for external portfolio portals.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

/// Pricing Rates for Precise INR Rupee Tracking
pub const COST_PER_STT_SECOND_INR: f64 = 0.0075;     // ~45 paise per min (Sarvam Saaras)
pub const COST_PER_1K_LLM_TOKENS_INR: f64 = 0.015;    // ~1.5 paise per 1K tokens
pub const COST_PER_TTS_1K_CHARS_INR: f64 = 0.020;     // ~2 paise per 1K chars (Sarvam Bulbul)
pub const DEFAULT_TRIAL_BUDGET_INR: f64 = 10.0;       // ₹10.00 Sarvam trial credit limit
pub const DEFAULT_TRIAL_MAX_DURATION_SECS: u64 = 300; // 5 minutes max cumulative duration
pub const DEFAULT_TRIAL_MAX_WORKFLOWS: u32 = 3;       // Trial workflow creation limit
pub const DEFAULT_TRIAL_MAX_DOCS: u32 = 5;            // Trial knowledge ingestion limit

/// User profile and credit balance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub is_admin: bool,
    
    // Limits and Quotas
    pub max_budget_inr: f64,
    pub max_call_duration_secs: u64,
    pub max_tokens: u64,
    pub max_workflows: u32,
    pub max_ingest_docs: u32,
    
    // Cumulative Consumed Usage
    pub used_budget_inr: f64,
    pub used_call_duration_secs: u64,
    pub used_tokens: u64,
    pub used_workflows: u32,
    pub used_ingest_docs: u32,
    
    // Status
    pub is_unlocked_by_admin: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Active Session Token mapping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub token: String,
    pub user_id: String,
    pub email: String,
    pub is_admin: bool,
    pub created_at: i64,
    pub expires_at: i64,
}

/// Usage status summary returned to UI / client
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuotaStatus {
    pub user_id: String,
    pub email: String,
    pub is_admin: bool,
    pub is_unlocked: bool,
    
    // Budgets
    pub max_budget_inr: f64,
    pub used_budget_inr: f64,
    pub remaining_budget_inr: f64,
    pub budget_exceeded: bool,
    
    // Call Duration
    pub max_duration_secs: u64,
    pub used_duration_secs: u64,
    pub remaining_duration_secs: u64,
    pub duration_exceeded: bool,
    
    // Tokens
    pub max_tokens: u64,
    pub used_tokens: u64,
    
    // Workflows & Ingestion
    pub max_workflows: u32,
    pub used_workflows: u32,
    pub max_ingest_docs: u32,
    pub used_ingest_docs: u32,
    
    pub can_make_calls: bool,
    pub can_create_workflow: bool,
    pub can_ingest_docs: bool,
    pub message: String,
}

pub struct AuthService {
    sessions: Arc<RwLock<HashMap<String, AuthSession>>>,
    admin_secret: String,
}

impl AuthService {
    pub fn new() -> Self {
        let admin_secret = std::env::var("ADMIN_SECRET_KEY")
            .unwrap_or_else(|_| "secret-portfolio-admin-beacon-key-2026".to_string());

        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            admin_secret,
        }
    }

    /// Validate admin secret key (e.g. from portfolio portal webhook)
    pub fn verify_admin_secret(&self, secret: &str) -> bool {
        secret.trim() == self.admin_secret.trim()
    }

    /// Create or record a new session
    pub fn create_session(&self, user: &UserProfile) -> AuthSession {
        let token = format!("sess_{}_{}", user.id, rand::random::<u64>());
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        let session = AuthSession {
            token: token.clone(),
            user_id: user.id.clone(),
            email: user.email.clone(),
            is_admin: user.is_admin,
            created_at: now,
            expires_at: now + (86400 * 7), // 7 days
        };

        self.sessions.write().insert(token.clone(), session.clone());
        session
    }

    /// Retrieve session from token
    pub fn get_session(&self, token: &str) -> Option<AuthSession> {
        self.sessions.read().get(token).cloned()
    }

    /// Compute Quota Status
    pub fn check_quota(&self, user: &UserProfile) -> QuotaStatus {
        if user.is_admin || user.is_unlocked_by_admin {
            return QuotaStatus {
                user_id: user.id.clone(),
                email: user.email.clone(),
                is_admin: user.is_admin,
                is_unlocked: true,
                max_budget_inr: 99999.0,
                used_budget_inr: user.used_budget_inr,
                remaining_budget_inr: 99999.0,
                budget_exceeded: false,
                max_duration_secs: 999999,
                used_duration_secs: user.used_call_duration_secs,
                remaining_duration_secs: 999999,
                duration_exceeded: false,
                max_tokens: 9999999,
                used_tokens: user.used_tokens,
                max_workflows: 9999,
                used_workflows: user.used_workflows,
                max_ingest_docs: 9999,
                used_ingest_docs: user.used_ingest_docs,
                can_make_calls: true,
                can_create_workflow: true,
                can_ingest_docs: true,
                message: "Unlimited Enterprise / Portfolio Admin Account".to_string(),
            };
        }

        let budget_exceeded = user.used_budget_inr >= user.max_budget_inr;
        let duration_exceeded = user.used_call_duration_secs >= user.max_call_duration_secs;
        let wf_exceeded = user.used_workflows >= user.max_workflows;
        let doc_exceeded = user.used_ingest_docs >= user.max_ingest_docs;

        let can_make_calls = !budget_exceeded && !duration_exceeded;
        let can_create_workflow = !wf_exceeded && !budget_exceeded;
        let can_ingest_docs = !doc_exceeded && !budget_exceeded;

        let remaining_budget = (user.max_budget_inr - user.used_budget_inr).max(0.0);
        let remaining_duration = user.max_call_duration_secs.saturating_sub(user.used_call_duration_secs);

        let mut msg = "Trial account active.".to_string();
        if budget_exceeded {
            msg = format!("Trial budget of ₹{:.2} exhausted. Request approval from admin/portfolio to unlock higher credits.", user.max_budget_inr);
        } else if duration_exceeded {
            msg = format!("Trial call duration of {}s exhausted. Request approval from admin/portfolio to unlock more minutes.", user.max_call_duration_secs);
        }

        QuotaStatus {
            user_id: user.id.clone(),
            email: user.email.clone(),
            is_admin: false,
            is_unlocked: false,
            max_budget_inr: user.max_budget_inr,
            used_budget_inr: user.used_budget_inr,
            remaining_budget_inr: remaining_budget,
            budget_exceeded,
            max_duration_secs: user.max_call_duration_secs,
            used_duration_secs: user.used_call_duration_secs,
            remaining_duration_secs: remaining_duration,
            duration_exceeded,
            max_tokens: user.max_tokens,
            used_tokens: user.used_tokens,
            max_workflows: user.max_workflows,
            used_workflows: user.used_workflows,
            max_ingest_docs: user.max_ingest_docs,
            used_ingest_docs: user.used_ingest_docs,
            can_make_calls,
            can_create_workflow,
            can_ingest_docs,
            message: msg,
        }
    }
}
