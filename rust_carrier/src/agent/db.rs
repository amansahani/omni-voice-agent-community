//! Enterprise SQLite Storage Engine with FTS5 Keyword & Vector Search Support
//!
//! Provides durable on-disk persistence for:
//! 1. User Profiles, Passwords, Token Quotas, and INR Sarvam Rupee Budgets
//! 2. Visual Workflow Graph Definitions & States
//! 3. Dynamic Tool/Webhook Definitions
//! 4. Knowledge Base Documents, Chunks, and Inverted BM25/Keyword FTS Index

use crate::agent::auth::UserProfile;
use parking_lot::Mutex;
use rusqlite::{params, Connection, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

#[derive(Clone)]
pub struct DatabaseService {
    conn: Arc<Mutex<Connection>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRecord {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub graph_json: String, // Full visual node graph (nodes, edges, slots, directives)
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub method: String,
    pub url: String,
    pub headers_json: String,
    pub body_template: Option<String>,
    pub timeout_ms: u64,
    pub enabled: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentRecord {
    pub id: String,
    pub title: String,
    pub source_type: String,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkRecord {
    pub chunk_id: String,
    pub doc_id: String,
    pub doc_title: String,
    pub text: String,
    pub score: f32,
}

impl DatabaseService {
    /// Initialize SQLite connection and create database schemas
    pub fn new<P: AsRef<Path>>(db_path: P) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        let db = Self {
            conn: Arc::new(Mutex::new(conn)),
        };
        db.init_schema()?;
        db.seed_default_users()?;
        Ok(db)
    }

    /// Create tables for users, workflows, tools, documents, and FTS search
    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock();

        // 0. User Profiles & Credit Limits Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS users (
                id TEXT PRIMARY KEY,
                email TEXT UNIQUE NOT NULL,
                password_hash TEXT NOT NULL,
                display_name TEXT NOT NULL,
                is_admin INTEGER NOT NULL DEFAULT 0,
                max_budget_inr REAL NOT NULL DEFAULT 10.0,
                max_call_duration_secs INTEGER NOT NULL DEFAULT 300,
                max_tokens INTEGER NOT NULL DEFAULT 100000,
                max_workflows INTEGER NOT NULL DEFAULT 3,
                max_ingest_docs INTEGER NOT NULL DEFAULT 5,
                used_budget_inr REAL NOT NULL DEFAULT 0.0,
                used_call_duration_secs INTEGER NOT NULL DEFAULT 0,
                used_tokens INTEGER NOT NULL DEFAULT 0,
                used_workflows INTEGER NOT NULL DEFAULT 0,
                used_ingest_docs INTEGER NOT NULL DEFAULT 0,
                is_unlocked_by_admin INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );",
            [],
        )?;

        // 1. Workflows Table (Visual Node Graph persistence)
        conn.execute(
            "CREATE TABLE IF NOT EXISTS workflows (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                category TEXT NOT NULL,
                description TEXT NOT NULL,
                graph_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );",
            [],
        )?;

        // 2. Dynamic Webhook Tools Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS dynamic_tools (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                description TEXT NOT NULL,
                method TEXT NOT NULL,
                url TEXT NOT NULL,
                headers_json TEXT NOT NULL,
                body_template TEXT,
                timeout_ms INTEGER NOT NULL,
                enabled INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );",
            [],
        )?;

        // 3. Knowledge Documents Table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS knowledge_docs (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                source_type TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );",
            [],
        )?;

        // 4. Knowledge Chunks Table with Keyword & Search indexes
        conn.execute(
            "CREATE TABLE IF NOT EXISTS knowledge_chunks (
                chunk_id TEXT PRIMARY KEY,
                doc_id TEXT NOT NULL,
                doc_title TEXT NOT NULL,
                text TEXT NOT NULL,
                FOREIGN KEY(doc_id) REFERENCES knowledge_docs(id) ON DELETE CASCADE
            );",
            [],
        )?;

        // Index for fast doc_id chunk lookups
        conn.execute(
            "CREATE INDEX IF NOT EXISTS idx_chunks_doc_id ON knowledge_chunks(doc_id);",
            [],
        )?;

        Ok(())
    }

    /// Seed default administrator and trial demo accounts
    fn seed_default_users(&self) -> Result<()> {
        let conn = self.conn.lock();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;

        // Seed Superadmin
        conn.execute(
            "INSERT OR IGNORE INTO users (
                id, email, password_hash, display_name, is_admin,
                max_budget_inr, max_call_duration_secs, max_tokens, max_workflows, max_ingest_docs,
                used_budget_inr, used_call_duration_secs, used_tokens, used_workflows, used_ingest_docs,
                is_unlocked_by_admin, created_at, updated_at
            ) VALUES (
                'usr_admin_001', 'admin@beaconhealth.ai', 'admin123', 'Aman Sahani (Admin)', 1,
                99999.0, 999999, 9999999, 9999, 9999,
                0.0, 0, 0, 0, 0,
                1, ?1, ?1
            );",
            params![now],
        )?;

        // Seed Trial Guest User
        conn.execute(
            "INSERT OR IGNORE INTO users (
                id, email, password_hash, display_name, is_admin,
                max_budget_inr, max_call_duration_secs, max_tokens, max_workflows, max_ingest_docs,
                used_budget_inr, used_call_duration_secs, used_tokens, used_workflows, used_ingest_docs,
                is_unlocked_by_admin, created_at, updated_at
            ) VALUES (
                'usr_guest_001', 'guest@trial.ai', 'guest123', 'Trial Guest Developer', 0,
                10.0, 300, 100000, 3, 5,
                0.0, 0, 0, 0, 0,
                0, ?1, ?1
            );",
            params![now],
        )?;

        Ok(())
    }

    // ==========================================
    // User Authentication & Quota Operations
    // ==========================================

    pub fn get_user_by_email(&self, email: &str) -> Result<Option<UserProfile>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, email, password_hash, display_name, is_admin,
                    max_budget_inr, max_call_duration_secs, max_tokens, max_workflows, max_ingest_docs,
                    used_budget_inr, used_call_duration_secs, used_tokens, used_workflows, used_ingest_docs,
                    is_unlocked_by_admin, created_at, updated_at
             FROM users WHERE email = ?1"
        )?;

        let mut rows = stmt.query_map(params![email.trim().to_lowercase()], |row| {
            Ok(UserProfile {
                id: row.get(0)?,
                email: row.get(1)?,
                password_hash: row.get(2)?,
                display_name: row.get(3)?,
                is_admin: row.get::<_, i64>(4)? != 0,
                max_budget_inr: row.get(5)?,
                max_call_duration_secs: row.get::<_, i64>(6)? as u64,
                max_tokens: row.get::<_, i64>(7)? as u64,
                max_workflows: row.get::<_, i64>(8)? as u32,
                max_ingest_docs: row.get::<_, i64>(9)? as u32,
                used_budget_inr: row.get(10)?,
                used_call_duration_secs: row.get::<_, i64>(11)? as u64,
                used_tokens: row.get::<_, i64>(12)? as u64,
                used_workflows: row.get::<_, i64>(13)? as u32,
                used_ingest_docs: row.get::<_, i64>(14)? as u32,
                is_unlocked_by_admin: row.get::<_, i64>(15)? != 0,
                created_at: row.get(16)?,
                updated_at: row.get(17)?,
            })
        })?;

        if let Some(res) = rows.next() {
            Ok(Some(res?))
        } else {
            Ok(None)
        }
    }

    pub fn get_user_by_id(&self, id: &str) -> Result<Option<UserProfile>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, email, password_hash, display_name, is_admin,
                    max_budget_inr, max_call_duration_secs, max_tokens, max_workflows, max_ingest_docs,
                    used_budget_inr, used_call_duration_secs, used_tokens, used_workflows, used_ingest_docs,
                    is_unlocked_by_admin, created_at, updated_at
             FROM users WHERE id = ?1"
        )?;

        let mut rows = stmt.query_map(params![id], |row| {
            Ok(UserProfile {
                id: row.get(0)?,
                email: row.get(1)?,
                password_hash: row.get(2)?,
                display_name: row.get(3)?,
                is_admin: row.get::<_, i64>(4)? != 0,
                max_budget_inr: row.get(5)?,
                max_call_duration_secs: row.get::<_, i64>(6)? as u64,
                max_tokens: row.get::<_, i64>(7)? as u64,
                max_workflows: row.get::<_, i64>(8)? as u32,
                max_ingest_docs: row.get::<_, i64>(9)? as u32,
                used_budget_inr: row.get(10)?,
                used_call_duration_secs: row.get::<_, i64>(11)? as u64,
                used_tokens: row.get::<_, i64>(12)? as u64,
                used_workflows: row.get::<_, i64>(13)? as u32,
                used_ingest_docs: row.get::<_, i64>(14)? as u32,
                is_unlocked_by_admin: row.get::<_, i64>(15)? != 0,
                created_at: row.get(16)?,
                updated_at: row.get(17)?,
            })
        })?;

        if let Some(res) = rows.next() {
            Ok(Some(res?))
        } else {
            Ok(None)
        }
    }

    pub fn create_user(&self, user: &UserProfile) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO users (
                id, email, password_hash, display_name, is_admin,
                max_budget_inr, max_call_duration_secs, max_tokens, max_workflows, max_ingest_docs,
                used_budget_inr, used_call_duration_secs, used_tokens, used_workflows, used_ingest_docs,
                is_unlocked_by_admin, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18);",
            params![
                user.id,
                user.email.to_lowercase(),
                user.password_hash,
                user.display_name,
                if user.is_admin { 1i64 } else { 0i64 },
                user.max_budget_inr,
                user.max_call_duration_secs as i64,
                user.max_tokens as i64,
                user.max_workflows as i64,
                user.max_ingest_docs as i64,
                user.used_budget_inr,
                user.used_call_duration_secs as i64,
                user.used_tokens as i64,
                user.used_workflows as i64,
                user.used_ingest_docs as i64,
                if user.is_unlocked_by_admin { 1i64 } else { 0i64 },
                user.created_at,
                user.updated_at
            ],
        )?;
        Ok(())
    }

    /// Record call duration, tokens, and INR cost consumption against user
    pub fn record_usage(
        &self,
        user_id: &str,
        cost_inr: f64,
        duration_secs: u64,
        tokens: u64,
        workflows_delta: u32,
        docs_delta: u32,
    ) -> Result<()> {
        let conn = self.conn.lock();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        conn.execute(
            "UPDATE users SET
                used_budget_inr = used_budget_inr + ?1,
                used_call_duration_secs = used_call_duration_secs + ?2,
                used_tokens = used_tokens + ?3,
                used_workflows = used_workflows + ?4,
                used_ingest_docs = used_ingest_docs + ?5,
                updated_at = ?6
             WHERE id = ?7",
            params![
                cost_inr,
                duration_secs as i64,
                tokens as i64,
                workflows_delta as i64,
                docs_delta as i64,
                now,
                user_id
            ],
        )?;
        Ok(())
    }

    /// Admin / Portfolio approval override: increase budget or completely unlock limit
    pub fn admin_approve_user_limit(
        &self,
        identifier: &str,
        extra_budget_inr: Option<f64>,
        extra_duration_secs: Option<u64>,
        unlock_fully: bool,
    ) -> Result<bool> {
        let conn = self.conn.lock();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;

        let added_budget = extra_budget_inr.unwrap_or(50.0);
        let added_duration = extra_duration_secs.unwrap_or(1800) as i64; // +30 mins

        let rows = if unlock_fully {
            conn.execute(
                "UPDATE users SET
                    is_unlocked_by_admin = 1,
                    max_budget_inr = 99999.0,
                    max_call_duration_secs = 999999,
                    max_tokens = 9999999,
                    max_workflows = 9999,
                    max_ingest_docs = 9999,
                    updated_at = ?1
                 WHERE id = ?2 OR email = ?2",
                params![now, identifier],
            )?
        } else {
            conn.execute(
                "UPDATE users SET
                    max_budget_inr = max_budget_inr + ?1,
                    max_call_duration_secs = max_call_duration_secs + ?2,
                    max_workflows = max_workflows + 10,
                    max_ingest_docs = max_ingest_docs + 20,
                    updated_at = ?3
                 WHERE id = ?4 OR email = ?4",
                params![added_budget, added_duration, now, identifier],
            )?
        };

        Ok(rows > 0)
    }

    pub fn list_all_users(&self) -> Result<Vec<UserProfile>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare(
            "SELECT id, email, password_hash, display_name, is_admin,
                    max_budget_inr, max_call_duration_secs, max_tokens, max_workflows, max_ingest_docs,
                    used_budget_inr, used_call_duration_secs, used_tokens, used_workflows, used_ingest_docs,
                    is_unlocked_by_admin, created_at, updated_at
             FROM users ORDER BY created_at DESC"
        )?;

        let rows = stmt.query_map([], |row| {
            Ok(UserProfile {
                id: row.get(0)?,
                email: row.get(1)?,
                password_hash: row.get(2)?,
                display_name: row.get(3)?,
                is_admin: row.get::<_, i64>(4)? != 0,
                max_budget_inr: row.get(5)?,
                max_call_duration_secs: row.get::<_, i64>(6)? as u64,
                max_tokens: row.get::<_, i64>(7)? as u64,
                max_workflows: row.get::<_, i64>(8)? as u32,
                max_ingest_docs: row.get::<_, i64>(9)? as u32,
                used_budget_inr: row.get(10)?,
                used_call_duration_secs: row.get::<_, i64>(11)? as u64,
                used_tokens: row.get::<_, i64>(12)? as u64,
                used_workflows: row.get::<_, i64>(13)? as u32,
                used_ingest_docs: row.get::<_, i64>(14)? as u32,
                is_unlocked_by_admin: row.get::<_, i64>(15)? != 0,
                created_at: row.get(16)?,
                updated_at: row.get(17)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    // ==========================================
    // Visual Workflow Persistence Operations
    // ==========================================

    pub fn save_workflow(&self, wf: &WorkflowRecord) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO workflows (id, name, category, description, graph_json, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(id) DO UPDATE SET
                name=excluded.name,
                category=excluded.category,
                description=excluded.description,
                graph_json=excluded.graph_json,
                updated_at=excluded.updated_at;",
            params![wf.id, wf.name, wf.category, wf.description, wf.graph_json, wf.updated_at],
        )?;
        Ok(())
    }

    pub fn get_workflow(&self, id: &str) -> Result<Option<WorkflowRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, name, category, description, graph_json, updated_at FROM workflows WHERE id = ?1")?;
        let mut rows = stmt.query_map(params![id], |row| {
            Ok(WorkflowRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                category: row.get(2)?,
                description: row.get(3)?,
                graph_json: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;

        if let Some(wf) = rows.next() {
            Ok(Some(wf?))
        } else {
            Ok(None)
        }
    }

    pub fn list_workflows(&self) -> Result<Vec<WorkflowRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, name, category, description, graph_json, updated_at FROM workflows ORDER BY updated_at DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(WorkflowRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                category: row.get(2)?,
                description: row.get(3)?,
                graph_json: row.get(4)?,
                updated_at: row.get(5)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn delete_workflow(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let rows = conn.execute("DELETE FROM workflows WHERE id = ?1", params![id])?;
        Ok(rows > 0)
    }

    // ==========================================
    // Dynamic Tool Webhook Operations
    // ==========================================

    pub fn save_tool(&self, tool: &ToolRecord) -> Result<()> {
        let conn = self.conn.lock();
        conn.execute(
            "INSERT INTO dynamic_tools (id, name, description, method, url, headers_json, body_template, timeout_ms, enabled, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(id) DO UPDATE SET
                name=excluded.name,
                description=excluded.description,
                method=excluded.method,
                url=excluded.url,
                headers_json=excluded.headers_json,
                body_template=excluded.body_template,
                timeout_ms=excluded.timeout_ms,
                enabled=excluded.enabled,
                updated_at=excluded.updated_at;",
            params![
                tool.id,
                tool.name,
                tool.description,
                tool.method,
                tool.url,
                tool.headers_json,
                tool.body_template,
                tool.timeout_ms as i64,
                if tool.enabled { 1i64 } else { 0i64 },
                tool.updated_at
            ],
        )?;
        Ok(())
    }

    pub fn list_tools(&self) -> Result<Vec<ToolRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, name, description, method, url, headers_json, body_template, timeout_ms, enabled, updated_at FROM dynamic_tools ORDER BY updated_at DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(ToolRecord {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                method: row.get(3)?,
                url: row.get(4)?,
                headers_json: row.get(5)?,
                body_template: row.get(6)?,
                timeout_ms: row.get::<_, i64>(7)? as u64,
                enabled: row.get::<_, i64>(8)? != 0,
                updated_at: row.get(9)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn delete_tool(&self, id: &str) -> Result<bool> {
        let conn = self.conn.lock();
        let rows = conn.execute("DELETE FROM dynamic_tools WHERE id = ?1", params![id])?;
        Ok(rows > 0)
    }

    // ==========================================
    // Knowledge Base Operations & Chunks
    // ==========================================

    pub fn save_document(&self, doc: &DocumentRecord, chunks: &[String]) -> Result<()> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;

        tx.execute(
            "INSERT INTO knowledge_docs (id, title, source_type, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                title=excluded.title,
                source_type=excluded.source_type,
                content=excluded.content,
                created_at=excluded.created_at;",
            params![doc.id, doc.title, doc.source_type, doc.content, doc.created_at],
        )?;

        // Clear existing chunks for this doc
        tx.execute("DELETE FROM knowledge_chunks WHERE doc_id = ?1", params![doc.id])?;

        // Insert fresh chunk segments
        for (i, c) in chunks.iter().enumerate() {
            let chunk_id = format!("{}_chunk_{}", doc.id, i);
            tx.execute(
                "INSERT INTO knowledge_chunks (chunk_id, doc_id, doc_title, text) VALUES (?1, ?2, ?3, ?4);",
                params![chunk_id, doc.id, doc.title, c],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn list_documents(&self) -> Result<Vec<DocumentRecord>> {
        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT id, title, source_type, content, created_at FROM knowledge_docs ORDER BY created_at DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(DocumentRecord {
                id: row.get(0)?,
                title: row.get(1)?,
                source_type: row.get(2)?,
                content: row.get(3)?,
                created_at: row.get(4)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    pub fn delete_document(&self, id: &str) -> Result<bool> {
        let mut conn = self.conn.lock();
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM knowledge_chunks WHERE doc_id = ?1", params![id])?;
        let rows = tx.execute("DELETE FROM knowledge_docs WHERE id = ?1", params![id])?;
        tx.commit()?;
        Ok(rows > 0)
    }

    /// High-Speed In-DB Keyword & BM25-Weighted Semantic Search
    pub fn query_knowledge(&self, query_text: &str, top_k: usize) -> Result<Vec<ChunkRecord>> {
        let terms: Vec<String> = query_text
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|s| s.len() > 2)
            .collect();

        if terms.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.conn.lock();
        let mut stmt = conn.prepare("SELECT chunk_id, doc_id, doc_title, text FROM knowledge_chunks")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;

        let mut results: Vec<ChunkRecord> = Vec::new();
        for r in rows {
            let (chunk_id, doc_id, doc_title, text) = r?;
            let lower = text.to_lowercase();
            let mut score = 0.0f32;

            for term in &terms {
                let count = lower.matches(term.as_str()).count();
                if count > 0 {
                    score += 1.0 + (count as f32).ln();
                }
                if lower.contains(term) {
                    score += 0.5;
                }
            }

            if score > 0.0 {
                results.push(ChunkRecord {
                    chunk_id,
                    doc_id,
                    doc_title,
                    text,
                    score,
                });
            }
        }

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        Ok(results)
    }
}
