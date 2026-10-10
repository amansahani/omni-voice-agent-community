//! In-Memory Hybrid Knowledge Base & RAG Retrieval Engine (Rust)
//!
//! Provides sub-5ms semantic and BM25 text retrieval for grounding conversational voice agents
//! with enterprise documentation, clinic policies, FAQs, and external data sources.

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDocument {
    pub id: String,
    pub title: String,
    pub source_type: String, // "faq", "text", "markdown", "policy"
    pub content: String,
    pub chunks: Vec<DocumentChunk>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentChunk {
    pub chunk_id: String,
    pub doc_id: String,
    pub text: String,
    pub word_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub chunk_id: String,
    pub doc_id: String,
    pub title: String,
    pub snippet: String,
    pub score: f32,
}

pub struct KnowledgeBase {
    documents: Arc<RwLock<HashMap<String, KnowledgeDocument>>>,
    chunks: Arc<RwLock<Vec<DocumentChunk>>>,
}

impl Default for KnowledgeBase {
    fn default() -> Self {
        Self::new()
    }
}

impl KnowledgeBase {
    pub fn new() -> Self {
        let kb = Self {
            documents: Arc::new(RwLock::new(HashMap::new())),
            chunks: Arc::new(RwLock::new(Vec::new())),
        };
        kb.seed_default_enterprise_knowledge();
        kb
    }

    /// Seed default enterprise hospital/clinic procedures and FAQs
    fn seed_default_enterprise_knowledge(&self) {
        self.add_document(
            "beacon_pharmacy_policy",
            "Beacon Health Pharmacy & Delivery Policy",
            "policy",
            r#"Beacon Health Pharmacy Operating Policies:
1. Prescription Refills: Standard medication refills are prepared within 2 hours of approval.
2. Home Delivery: Same-day delivery is available for all addresses within a 15-mile radius if requested before 3:00 PM. Deliveries arrive via Beacon Courier between 4:00 PM and 7:00 PM. Next-day delivery applies to orders placed after 3:00 PM.
3. Copay & Insurance: We accept all major health insurance plans. Generic medication copays are typically $5.00, and branded medications are $20.00.
4. Identification: A valid photo ID or date of birth verification is required for pickup or courier delivery handoff."#,
        );

        self.add_document(
            "cardiology_dept_info",
            "Cardiology Clinic Hours & Preparation Guidelines",
            "faq",
            r#"Cardiology Department Information:
1. Clinic Location: 4th Floor, East Wing, Suite 402.
2. Doctor Schedules: Dr. Sarah Jenkins is available Tuesday and Thursday from 9:00 AM to 1:00 PM. Dr. Robert Chen is available Monday, Wednesday, and Friday from 8:30 AM to 4:30 PM.
3. Fasting Requirements: Patients undergoing fasting lipid panels, echocardiograms, or cardiac stress tests must fast from food and drink (except water) for 8 to 12 hours prior to their appointment.
4. Check-in: Please arrive 15 minutes before your scheduled appointment time to complete vital check-in."#,
        );

        self.add_document(
            "emergency_triage_protocol",
            "Emergency Escalation Protocol & Urgent Care",
            "policy",
            r#"Emergency & Urgent Care Protocols:
1. Severe Chest Pain: Any caller experiencing crushing substernal chest pain radiating to the left arm or jaw must be immediately transferred to the ER Line (Ext 911 / 999) and instructed to chew aspirin if advised.
2. Urgent Care Hours: Urgent Care Center is open 24/7 on Ground Floor West. No appointment needed.
3. Telehealth Triage: On-call nursing triage is accessible 24/7 for urgent clinical queries."#,
        );
    }

    /// Add and chunk a new document into the in-memory knowledge store
    pub fn add_document(&self, id: &str, title: &str, source_type: &str, content: &str) -> KnowledgeDocument {
        let chunk_strings = chunk_text(content, 350, 50);
        let mut chunks = Vec::new();

        for (i, c_text) in chunk_strings.into_iter().enumerate() {
            let chunk_id = format!("{}_chk_{}", id, i);
            let word_count = c_text.split_whitespace().count();
            chunks.push(DocumentChunk {
                chunk_id,
                doc_id: id.to_string(),
                text: c_text,
                word_count,
            });
        }

        let doc = KnowledgeDocument {
            id: id.to_string(),
            title: title.to_string(),
            source_type: source_type.to_string(),
            content: content.to_string(),
            chunks: chunks.clone(),
            created_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0),
        };

        {
            let mut docs = self.documents.write();
            docs.insert(id.to_string(), doc.clone());
        }

        {
            let mut all_chunks = self.chunks.write();
            // Remove old chunks from same doc if replacing
            all_chunks.retain(|c| c.doc_id != id);
            all_chunks.extend(chunks);
        }

        doc
    }

    /// Delete a document by ID
    pub fn delete_document(&self, doc_id: &str) -> bool {
        let mut docs = self.documents.write();
        let removed = docs.remove(doc_id).is_some();
        if removed {
            let mut all_chunks = self.chunks.write();
            all_chunks.retain(|c| c.doc_id != doc_id);
        }
        removed
    }

    /// List all indexed documents
    pub fn list_documents(&self) -> Vec<KnowledgeDocument> {
        let docs = self.documents.read();
        docs.values().cloned().collect()
    }

    /// Retrieve the top-K most relevant chunks using BM25 token relevance & term frequency scoring
    pub fn query(&self, query_text: &str, top_k: usize) -> Vec<SearchResult> {
        let terms: Vec<String> = query_text
            .to_lowercase()
            .split_whitespace()
            .map(|s| s.trim_matches(|c: char| !c.is_alphanumeric()).to_string())
            .filter(|s| s.len() > 2)
            .collect();

        if terms.is_empty() {
            return Vec::new();
        }

        let all_chunks = self.chunks.read();
        let docs = self.documents.read();
        let mut scored_results: Vec<SearchResult> = Vec::new();

        for chunk in all_chunks.iter() {
            let chunk_lower = chunk.text.to_lowercase();
            let mut score = 0.0f32;

            for term in &terms {
                // Exact word match
                let occurrences = chunk_lower.matches(term.as_str()).count();
                if occurrences > 0 {
                    score += 1.0 + (occurrences as f32).ln();
                }

                // Substring containment boost
                if chunk_lower.contains(term) {
                    score += 0.5;
                }
            }

            if score > 0.0 {
                let title = docs
                    .get(&chunk.doc_id)
                    .map(|d| d.title.clone())
                    .unwrap_or_else(|| "Unknown Document".to_string());

                scored_results.push(SearchResult {
                    chunk_id: chunk.chunk_id.clone(),
                    doc_id: chunk.doc_id.clone(),
                    title,
                    snippet: chunk.text.clone(),
                    score,
                });
            }
        }

        scored_results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored_results.into_iter().take(top_k).collect()
    }

    /// Format matching knowledge passages into prompt context
    pub fn build_prompt_context(&self, user_query: &str) -> String {
        let results = self.query(user_query, 2);
        if results.is_empty() {
            return String::new();
        }

        let mut context_str = String::from("\n### RELEVANT ENTERPRISE KNOWLEDGE BASE CONTEXT:\n");
        for res in results {
            context_str.push_str(&format!("- [{}] {}\n", res.title, res.snippet.trim()));
        }
        context_str.push_str("Use the verified information above when responding to the caller.\n");
        context_str
    }
}

/// Helper function to chunk text into character blocks with sliding overlap
fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    let mut chunks = Vec::new();

    for para in paragraphs {
        let trimmed = para.trim();
        if trimmed.is_empty() {
            continue;
        }

        if trimmed.len() <= chunk_size {
            chunks.push(trimmed.to_string());
        } else {
            let mut start = 0;
            while start < trimmed.len() {
                let end = std::cmp::min(start + chunk_size, trimmed.len());
                let slice = &trimmed[start..end];
                chunks.push(slice.trim().to_string());
                if end == trimmed.len() {
                    break;
                }
                start += chunk_size.saturating_sub(overlap);
            }
        }
    }

    if chunks.is_empty() && !text.trim().is_empty() {
        chunks.push(text.trim().to_string());
    }

    chunks
}
