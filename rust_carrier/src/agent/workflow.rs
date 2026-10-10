//! Modular Hierarchical Agent Graph Engine (Gnani.ai / ElevenLabs Architecture)
//!
//! Visualizes workflows hierarchically:
//! Workflows consist of Execution Nodes (Entry Node -> Action/Triage Nodes -> Subagents -> End Nodes).
//! Each Node contains organized "Action Modules" executing sequentially or conditionally:
//! - Conversational Step Directives Module
//! - Knowledge Grounding (RAG) Module
//! - Tool/Webhook Dispatch Module
//! - Entity Slot Extraction & Validation Module
//! - Dynamic Branching / Transition Rules Module

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// Entity Slot definition for information extraction & validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySlot {
    pub name: String,
    pub description: String,
    pub slot_type: String, // "text", "date", "number", "choice", "phone"
    pub required: bool,
    pub validation_regex: Option<String>,
}

/// Conditional branch transition to the next node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EdgeTransition {
    pub target_node_id: String,
    pub condition_intent: Option<String>, // e.g., "confirm", "deny", "refill", "book"
    pub condition_expression: Option<String>, // e.g., "slots.confirmed == true"
    pub label: String,
}

/// Type of Action Module embedded inside a Node
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ModuleType {
    Directives,
    KnowledgeRAG,
    ToolDispatch,
    SlotExtraction,
    Branching,
}

/// Action Module inside a Node
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeActionModule {
    pub id: String,
    pub module_type: ModuleType,
    pub title: String,
    pub description: String,
    pub enabled: bool,
    
    // Directives configuration
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt_text: Option<String>,

    // Knowledge RAG configuration
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attached_doc_ids: Vec<String>,

    // Tool Dispatch configuration
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub bound_tool_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_execute_on_entry: Option<bool>,

    // Entity Slot extraction configuration
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub slots: Vec<EntitySlot>,

    // Branching transitions
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transitions: Vec<EdgeTransition>,
}

/// A Unified Conversational Node with modular components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedAgentNode {
    pub id: String,
    pub title: String,
    pub node_type: String, // "entry", "action", "subagent", "triage", "end"
    
    // 1. Scoped Agent Directives
    pub instructions: String,
    
    // 2. Scoped Knowledge Base Grounding (Doc IDs attached to this specific step)
    pub attached_knowledge_doc_ids: Vec<String>,
    
    // 3. Bound Webhook Tools (Executed automatically or on caller request)
    pub bound_tool_ids: Vec<String>,
    
    // 4. Entity Extraction Slots
    pub slots: Vec<EntitySlot>,
    
    // 5. Branching Edge Transitions
    pub transitions: Vec<EdgeTransition>,

    // 6. Configured Action Modules (Hierarchical View)
    #[serde(default)]
    pub modules: Vec<NodeActionModule>,
    
    // Canvas 2D Position for Visual DAG Designer
    pub position_x: f64,
    pub position_y: f64,
}

/// Complete Consolidated Workflow Definition
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedWorkflowGraph {
    pub id: String,
    pub name: String,
    pub category: String,
    pub description: String,
    pub global_system_prompt: String,
    pub initial_node_id: String,
    pub nodes: HashMap<String, UnifiedAgentNode>,
    pub created_at: u64,
    pub updated_at: u64,
}

/// Active Caller Execution State traversing the Unified Workflow Graph
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedSessionState {
    pub active_workflow_id: String,
    pub workflow_name: String,
    pub current_node_id: String,
    pub current_node_title: String,
    pub instructions: String,
    pub slots: HashMap<String, Value>,
    pub active_tools: Vec<String>,
    pub attached_docs: Vec<String>,
    pub active_modules: Vec<String>,
    pub history: Vec<String>,
    pub completed: bool,
    pub available_flows: Vec<String>,
}

pub struct ConsolidatedWorkflowEngine {
    workflows: Arc<RwLock<HashMap<String, UnifiedWorkflowGraph>>>,
    session: Arc<RwLock<UnifiedSessionState>>,
}

impl Default for ConsolidatedWorkflowEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsolidatedWorkflowEngine {
    pub fn new() -> Self {
        let mut flows = HashMap::new();
        flows.insert("beacon_reception_triage".to_string(), Self::seed_unified_reception_flow());
        flows.insert("appointment_booking_dag".to_string(), Self::seed_unified_appointment_flow());
        flows.insert("prescription_fulfillment_dag".to_string(), Self::seed_unified_prescription_flow());

        let default_flow = flows.get("beacon_reception_triage").unwrap();
        let init_node = default_flow.nodes.get(&default_flow.initial_node_id).unwrap();

        let available_flows: Vec<String> = flows
            .iter()
            .map(|(id, f)| format!("{} ({})", f.name, id))
            .collect();

        let active_modules = init_node.modules.iter().map(|m| format!("{}: {}", m.title, m.description)).collect();

        let session = UnifiedSessionState {
            active_workflow_id: default_flow.id.clone(),
            workflow_name: default_flow.name.clone(),
            current_node_id: init_node.id.clone(),
            current_node_title: init_node.title.clone(),
            instructions: init_node.instructions.clone(),
            slots: HashMap::new(),
            active_tools: init_node.bound_tool_ids.clone(),
            attached_docs: init_node.attached_knowledge_doc_ids.clone(),
            active_modules,
            history: vec![init_node.id.clone()],
            completed: false,
            available_flows,
        };

        Self {
            workflows: Arc::new(RwLock::new(flows)),
            session: Arc::new(RwLock::new(session)),
        }
    }

    /// Seed: Comprehensive Reception & Triage Unified Flow with Action Modules
    fn seed_unified_reception_flow() -> UnifiedWorkflowGraph {
        let mut nodes = HashMap::new();

        // Node 1: ENTRY NODE - Front Desk Greeting & Intent Gating
        let entry_modules = vec![
            NodeActionModule {
                id: "mod_welcome_prompt".to_string(),
                module_type: ModuleType::Directives,
                title: "Conversational Welcome Directive".to_string(),
                description: "Greet caller warmly to Beacon Health and screen initial inquiry.".to_string(),
                enabled: true,
                prompt_text: Some("Warmly welcome caller to Beacon Health. Identify primary intent: appointments, refills, lab results, or acute triage.".to_string()),
                attached_doc_ids: vec![],
                bound_tool_ids: vec![],
                auto_execute_on_entry: None,
                slots: vec![],
                transitions: vec![],
            },
            NodeActionModule {
                id: "mod_rag_clinic_info".to_string(),
                module_type: ModuleType::KnowledgeRAG,
                title: "Knowledge Grounding Module".to_string(),
                description: "Ground responses in Pharmacy Hours & Cardiology Clinic policies.".to_string(),
                enabled: true,
                prompt_text: None,
                attached_doc_ids: vec!["beacon_pharmacy_policy".to_string(), "cardiology_dept_info".to_string()],
                bound_tool_ids: vec![],
                auto_execute_on_entry: None,
                slots: vec![],
                transitions: vec![],
            },
            NodeActionModule {
                id: "mod_tool_service_lookup".to_string(),
                module_type: ModuleType::ToolDispatch,
                title: "Microsecond Service & Doctor Tool Dispatch".to_string(),
                description: "Provides real-time clinic catalog and doctor profiles upon caller question.".to_string(),
                enabled: true,
                prompt_text: None,
                attached_doc_ids: vec![],
                bound_tool_ids: vec!["list_clinic_services".to_string(), "lookup_doctor_profile".to_string()],
                auto_execute_on_entry: Some(false),
                slots: vec![],
                transitions: vec![],
            },
            NodeActionModule {
                id: "mod_branch_intent".to_string(),
                module_type: ModuleType::Branching,
                title: "Agentic Intent Router Module".to_string(),
                description: "Routes dynamically to specialized subagents based on detected intent.".to_string(),
                enabled: true,
                prompt_text: None,
                attached_doc_ids: vec![],
                bound_tool_ids: vec![],
                auto_execute_on_entry: None,
                slots: vec![],
                transitions: vec![
                    EdgeTransition { target_node_id: "schedule_appointment_subagent".to_string(), condition_intent: Some("book_appointment".to_string()), condition_expression: None, label: "Booking Intent ➔ Scheduling Subagent".to_string() },
                    EdgeTransition { target_node_id: "refill_prescription_subagent".to_string(), condition_intent: Some("refill_medication".to_string()), condition_expression: None, label: "Refill Intent ➔ Pharmacy Subagent".to_string() },
                    EdgeTransition { target_node_id: "urgent_triage_subagent".to_string(), condition_intent: Some("urgent_symptoms".to_string()), condition_expression: None, label: "Urgent Symptoms ➔ Emergency Protocol".to_string() },
                ],
            },
        ];

        nodes.insert(
            "welcome_intent_gate".to_string(),
            UnifiedAgentNode {
                id: "welcome_intent_gate".to_string(),
                title: "1. Entry Node: Welcome & Intent Dispatch".to_string(),
                node_type: "entry".to_string(),
                instructions: "Warmly welcome caller to Beacon Health. Identify primary intent: appointments, refills, lab results, or acute triage.".to_string(),
                attached_knowledge_doc_ids: vec!["beacon_pharmacy_policy".to_string(), "cardiology_dept_info".to_string()],
                bound_tool_ids: vec!["list_clinic_services".to_string(), "lookup_doctor_profile".to_string()],
                slots: vec![
                    EntitySlot { name: "caller_intent".to_string(), description: "Identified caller goal".to_string(), slot_type: "choice".to_string(), required: true, validation_regex: None }
                ],
                transitions: vec![
                    EdgeTransition { target_node_id: "schedule_appointment_subagent".to_string(), condition_intent: Some("book_appointment".to_string()), condition_expression: None, label: "Booking Intent ➔ Scheduling".to_string() },
                    EdgeTransition { target_node_id: "refill_prescription_subagent".to_string(), condition_intent: Some("refill_medication".to_string()), condition_expression: None, label: "Refill Intent ➔ Pharmacy".to_string() },
                    EdgeTransition { target_node_id: "urgent_triage_subagent".to_string(), condition_intent: Some("urgent_symptoms".to_string()), condition_expression: None, label: "Urgent Symptoms ➔ Triage".to_string() },
                ],
                modules: entry_modules,
                position_x: 100.0,
                position_y: 200.0,
            },
        );

        // Node 2: SCHEDULING SUBAGENT
        let sched_modules = vec![
            NodeActionModule {
                id: "mod_sched_prompt".to_string(),
                module_type: ModuleType::Directives,
                title: "Specialist Booking Directives".to_string(),
                description: "Guide caller through selecting clinic department and picking available time slot.".to_string(),
                enabled: true,
                prompt_text: Some("Help caller pick department and appointment slot. Confirm physician name and time slot.".to_string()),
                attached_doc_ids: vec![],
                bound_tool_ids: vec![],
                auto_execute_on_entry: None,
                slots: vec![],
                transitions: vec![],
            },
            NodeActionModule {
                id: "mod_sched_slots".to_string(),
                module_type: ModuleType::SlotExtraction,
                title: "Patient Slot Extraction Module".to_string(),
                description: "Extracts and validates specialty, preferred time, and caller identity.".to_string(),
                enabled: true,
                prompt_text: None,
                attached_doc_ids: vec![],
                bound_tool_ids: vec![],
                auto_execute_on_entry: None,
                slots: vec![
                    EntitySlot { name: "department".to_string(), description: "Specialty clinic".to_string(), slot_type: "choice".to_string(), required: true, validation_regex: None },
                    EntitySlot { name: "time_slot".to_string(), description: "Chosen time slot".to_string(), slot_type: "text".to_string(), required: true, validation_regex: None },
                    EntitySlot { name: "patient_name".to_string(), description: "Full patient name".to_string(), slot_type: "text".to_string(), required: true, validation_regex: None }
                ],
                transitions: vec![],
            },
            NodeActionModule {
                id: "mod_sched_tools".to_string(),
                module_type: ModuleType::ToolDispatch,
                title: "Calendar & Confirmation Webhooks".to_string(),
                description: "Real-time calendar slot lookups and durable appointment creation.".to_string(),
                enabled: true,
                prompt_text: None,
                attached_doc_ids: vec![],
                bound_tool_ids: vec!["check_calendar".to_string(), "confirm_booking".to_string()],
                auto_execute_on_entry: Some(true),
                slots: vec![],
                transitions: vec![],
            },
        ];

        nodes.insert(
            "schedule_appointment_subagent".to_string(),
            UnifiedAgentNode {
                id: "schedule_appointment_subagent".to_string(),
                title: "2. Action Node: Appointment Scheduling".to_string(),
                node_type: "action".to_string(),
                instructions: "Help caller pick department and appointment slot. Confirm physician name and time slot.".to_string(),
                attached_knowledge_doc_ids: vec!["cardiology_dept_info".to_string()],
                bound_tool_ids: vec!["check_calendar".to_string(), "confirm_booking".to_string()],
                slots: vec![
                    EntitySlot { name: "department".to_string(), description: "Specialty clinic".to_string(), slot_type: "choice".to_string(), required: true, validation_regex: None },
                    EntitySlot { name: "time_slot".to_string(), description: "Chosen time slot".to_string(), slot_type: "text".to_string(), required: true, validation_regex: None },
                    EntitySlot { name: "patient_name".to_string(), description: "Full patient name".to_string(), slot_type: "text".to_string(), required: true, validation_regex: None }
                ],
                transitions: vec![
                    EdgeTransition { target_node_id: "booking_confirmation_node".to_string(), condition_intent: Some("confirm".to_string()), condition_expression: None, label: "Slots Filled ➔ Confirmation".to_string() }
                ],
                modules: sched_modules,
                position_x: 450.0,
                position_y: 80.0,
            },
        );

        // Node 3: REFILL PRESCRIPTION SUBAGENT
        let refill_modules = vec![
            NodeActionModule {
                id: "mod_refill_prompt".to_string(),
                module_type: ModuleType::Directives,
                title: "Pharmacy Refill Directives".to_string(),
                description: "Verify medication name and offer delivery vs counter pickup.".to_string(),
                enabled: true,
                prompt_text: Some("Verify medication name. Offer Express Home Delivery or Counter Pickup.".to_string()),
                attached_doc_ids: vec![],
                bound_tool_ids: vec![],
                auto_execute_on_entry: None,
                slots: vec![],
                transitions: vec![],
            },
            NodeActionModule {
                id: "mod_refill_tools".to_string(),
                module_type: ModuleType::ToolDispatch,
                title: "EHR Prescription & Courier Dispatch Tools".to_string(),
                description: "Queries active prescription and dispatches courier webhook upon request.".to_string(),
                enabled: true,
                prompt_text: None,
                attached_doc_ids: vec![],
                bound_tool_ids: vec!["lookup_prescription".to_string(), "dispatch_courier".to_string()],
                auto_execute_on_entry: Some(false),
                slots: vec![],
                transitions: vec![],
            },
        ];

        nodes.insert(
            "refill_prescription_subagent".to_string(),
            UnifiedAgentNode {
                id: "refill_prescription_subagent".to_string(),
                title: "3. Action Node: Prescription Refill & Delivery".to_string(),
                node_type: "action".to_string(),
                instructions: "Verify medication name. Offer Express Home Delivery or Counter Pickup.".to_string(),
                attached_knowledge_doc_ids: vec!["beacon_pharmacy_policy".to_string()],
                bound_tool_ids: vec!["lookup_prescription".to_string(), "dispatch_courier".to_string()],
                slots: vec![
                    EntitySlot { name: "medication".to_string(), description: "Medication name".to_string(), slot_type: "text".to_string(), required: true, validation_regex: None },
                    EntitySlot { name: "fulfillment".to_string(), description: "Delivery or Pickup".to_string(), slot_type: "choice".to_string(), required: true, validation_regex: None }
                ],
                transitions: vec![
                    EdgeTransition { target_node_id: "refill_confirmation_node".to_string(), condition_intent: Some("confirm".to_string()), condition_expression: None, label: "Refill Approved ➔ Order Wrap".to_string() }
                ],
                modules: refill_modules,
                position_x: 450.0,
                position_y: 280.0,
            },
        );

        // Node 4: URGENT TRIAGE SUBAGENT
        let triage_modules = vec![
            NodeActionModule {
                id: "mod_triage_prompt".to_string(),
                module_type: ModuleType::Directives,
                title: "Acute Triage Directives".to_string(),
                description: "Screen for red-flag symptoms and route immediately to 24/7 Urgent Care.".to_string(),
                enabled: true,
                prompt_text: Some("Screen for emergency red flags (chest pain, acute bleeding). Route to Walk-in Urgent Care Suite 100.".to_string()),
                attached_doc_ids: vec![],
                bound_tool_ids: vec![],
                auto_execute_on_entry: None,
                slots: vec![],
                transitions: vec![],
            },
            NodeActionModule {
                id: "mod_triage_rag".to_string(),
                module_type: ModuleType::KnowledgeRAG,
                title: "Emergency Protocols Knowledge".to_string(),
                description: "Hospital emergency chest pain and triage escalation documentation.".to_string(),
                enabled: true,
                prompt_text: None,
                attached_doc_ids: vec!["emergency_triage_protocol".to_string()],
                bound_tool_ids: vec![],
                auto_execute_on_entry: None,
                slots: vec![],
                transitions: vec![],
            },
        ];

        nodes.insert(
            "urgent_triage_subagent".to_string(),
            UnifiedAgentNode {
                id: "urgent_triage_subagent".to_string(),
                title: "4. Action Node: Urgent Clinical Triage".to_string(),
                node_type: "triage".to_string(),
                instructions: "Screen for emergency red flags (chest pain, acute bleeding). Route to Walk-in Urgent Care Suite 100.".to_string(),
                attached_knowledge_doc_ids: vec!["emergency_triage_protocol".to_string()],
                bound_tool_ids: vec![],
                slots: vec![
                    EntitySlot { name: "symptoms".to_string(), description: "Acute symptoms reported".to_string(), slot_type: "text".to_string(), required: true, validation_regex: None }
                ],
                transitions: vec![],
                modules: triage_modules,
                position_x: 450.0,
                position_y: 480.0,
            },
        );

        // Node 5: BOOKING CONFIRMATION
        nodes.insert(
            "booking_confirmation_node".to_string(),
            UnifiedAgentNode {
                id: "booking_confirmation_node".to_string(),
                title: "5. End Node: Appointment Confirmed".to_string(),
                node_type: "end".to_string(),
                instructions: "Provide booking confirmation number and clinic address. Thank caller warmly and conclude.".to_string(),
                attached_knowledge_doc_ids: vec![],
                bound_tool_ids: vec![],
                slots: vec![],
                transitions: vec![],
                modules: vec![],
                position_x: 800.0,
                position_y: 80.0,
            },
        );

        // Node 6: REFILL CONFIRMATION
        nodes.insert(
            "refill_confirmation_node".to_string(),
            UnifiedAgentNode {
                id: "refill_confirmation_node".to_string(),
                title: "6. End Node: Refill Dispatched".to_string(),
                node_type: "end".to_string(),
                instructions: "Provide delivery tracking number, copay amount ($12.00), and arrival timeframe. Conclude call.".to_string(),
                attached_knowledge_doc_ids: vec![],
                bound_tool_ids: vec![],
                slots: vec![],
                transitions: vec![],
                modules: vec![],
                position_x: 800.0,
                position_y: 280.0,
            },
        );

        UnifiedWorkflowGraph {
            id: "beacon_reception_triage".to_string(),
            name: "Beacon Master Reception & Dynamic DAG".to_string(),
            category: "Enterprise Master Orchestration".to_string(),
            description: "Unified master graph routing seamlessly between triage, appointments, prescriptions, and emergencies.".to_string(),
            global_system_prompt: "You are Aria, an empathetic, conversational real-time phone receptionist at Beacon Health clinic. Keep responses concise and natural.".to_string(),
            initial_node_id: "welcome_intent_gate".to_string(),
            nodes,
            created_at: 1791462500,
            updated_at: 1791462500,
        }
    }

    fn seed_unified_appointment_flow() -> UnifiedWorkflowGraph {
        let mut nodes = HashMap::new();
        nodes.insert(
            "dept_intake".to_string(),
            UnifiedAgentNode {
                id: "dept_intake".to_string(),
                title: "Specialty Clinic Selection".to_string(),
                node_type: "entry".to_string(),
                instructions: "Ask caller which department (Cardiology, Dermatology, Dental, Pediatrics, General Medicine) they need.".to_string(),
                attached_knowledge_doc_ids: vec!["cardiology_dept_info".to_string()],
                bound_tool_ids: vec!["list_clinic_services".to_string()],
                slots: vec![EntitySlot { name: "department".to_string(), description: "Specialty".to_string(), slot_type: "choice".to_string(), required: true, validation_regex: None }],
                transitions: vec![EdgeTransition { target_node_id: "slot_choice".to_string(), condition_intent: None, condition_expression: None, label: "Dept Chosen ➔ Slots".to_string() }],
                modules: vec![],
                position_x: 100.0,
                position_y: 150.0,
            }
        );
        nodes.insert(
            "slot_choice".to_string(),
            UnifiedAgentNode {
                id: "slot_choice".to_string(),
                title: "Time Slot Selection".to_string(),
                node_type: "action".to_string(),
                instructions: "Present available appointment times (9:30 AM, 1:15 PM, 4:00 PM) and ask caller for their preference.".to_string(),
                attached_knowledge_doc_ids: vec![],
                bound_tool_ids: vec!["check_calendar".to_string()],
                slots: vec![EntitySlot { name: "time_slot".to_string(), description: "Time slot".to_string(), slot_type: "text".to_string(), required: true, validation_regex: None }],
                transitions: vec![EdgeTransition { target_node_id: "patient_name_intake".to_string(), condition_intent: None, condition_expression: None, label: "Slot Chosen ➔ Name".to_string() }],
                modules: vec![],
                position_x: 450.0,
                position_y: 150.0,
            }
        );
        nodes.insert(
            "patient_name_intake".to_string(),
            UnifiedAgentNode {
                id: "patient_name_intake".to_string(),
                title: "Patient Identity Intake".to_string(),
                node_type: "end".to_string(),
                instructions: "Collect patient full name and confirm appointment booking details.".to_string(),
                attached_knowledge_doc_ids: vec![],
                bound_tool_ids: vec!["confirm_booking".to_string()],
                slots: vec![EntitySlot { name: "patient_name".to_string(), description: "Patient name".to_string(), slot_type: "text".to_string(), required: true, validation_regex: None }],
                transitions: vec![],
                modules: vec![],
                position_x: 800.0,
                position_y: 150.0,
            }
        );

        UnifiedWorkflowGraph {
            id: "appointment_booking_dag".to_string(),
            name: "Appointment Scheduling DAG".to_string(),
            category: "Clinical Consultations".to_string(),
            description: "Direct specialist booking pipeline with calendar integration.".to_string(),
            global_system_prompt: "You are the Beacon Health scheduling assistant.".to_string(),
            initial_node_id: "dept_intake".to_string(),
            nodes,
            created_at: 1791462500,
            updated_at: 1791462500,
        }
    }

    fn seed_unified_prescription_flow() -> UnifiedWorkflowGraph {
        let mut nodes = HashMap::new();
        nodes.insert(
            "rx_lookup_node".to_string(),
            UnifiedAgentNode {
                id: "rx_lookup_node".to_string(),
                title: "Prescription Verification".to_string(),
                node_type: "entry".to_string(),
                instructions: "Identify caller medication and verify refill active status.".to_string(),
                attached_knowledge_doc_ids: vec!["beacon_pharmacy_policy".to_string()],
                bound_tool_ids: vec!["lookup_prescription".to_string()],
                slots: vec![EntitySlot { name: "medication".to_string(), description: "Medication name".to_string(), slot_type: "text".to_string(), required: true, validation_regex: None }],
                transitions: vec![EdgeTransition { target_node_id: "rx_fulfill_node".to_string(), condition_intent: None, condition_expression: None, label: "Rx Verified ➔ Fulfillment".to_string() }],
                modules: vec![],
                position_x: 100.0,
                position_y: 200.0,
            }
        );
        nodes.insert(
            "rx_fulfill_node".to_string(),
            UnifiedAgentNode {
                id: "rx_fulfill_node".to_string(),
                title: "Fulfillment & Courier Dispatch".to_string(),
                node_type: "end".to_string(),
                instructions: "Confirm delivery vs pickup and dispatch courier booking.".to_string(),
                attached_knowledge_doc_ids: vec!["beacon_pharmacy_policy".to_string()],
                bound_tool_ids: vec!["dispatch_courier".to_string()],
                slots: vec![EntitySlot { name: "fulfillment".to_string(), description: "Delivery method".to_string(), slot_type: "choice".to_string(), required: true, validation_regex: None }],
                transitions: vec![],
                modules: vec![],
                position_x: 500.0,
                position_y: 200.0,
            }
        );

        UnifiedWorkflowGraph {
            id: "prescription_fulfillment_dag".to_string(),
            name: "Prescription Refill & Delivery DAG".to_string(),
            category: "Dispensary & Pharmacy".to_string(),
            description: "Pharmacy medication verification and same-day delivery dispatch.".to_string(),
            global_system_prompt: "You are the Beacon Health pharmacy receptionist.".to_string(),
            initial_node_id: "rx_lookup_node".to_string(),
            nodes,
            created_at: 1791462500,
            updated_at: 1791462500,
        }
    }

    /// List all consolidated workflows
    pub fn list_all_workflows(&self) -> Vec<UnifiedWorkflowGraph> {
        let flows = self.workflows.read();
        flows.values().cloned().collect()
    }

    /// Register or update a consolidated workflow
    pub fn register_workflow(&self, wf: UnifiedWorkflowGraph) {
        let mut flows = self.workflows.write();
        flows.insert(wf.id.clone(), wf);
        let mut session = self.session.write();
        session.available_flows = flows
            .iter()
            .map(|(id, f)| format!("{} ({})", f.name, id))
            .collect();
    }

    /// Switch the active workflow graph and reset session state
    pub fn switch_workflow(&self, flow_id: &str) -> bool {
        let flows = self.workflows.read();
        let flow = match flows.get(flow_id) {
            Some(f) => f,
            None => return false,
        };

        let init_node = match flow.nodes.get(&flow.initial_node_id) {
            Some(n) => n,
            None => return false,
        };

        let active_modules = init_node.modules.iter().map(|m| format!("{}: {}", m.title, m.description)).collect();

        let mut session = self.session.write();
        session.active_workflow_id = flow.id.clone();
        session.workflow_name = flow.name.clone();
        session.current_node_id = init_node.id.clone();
        session.current_node_title = init_node.title.clone();
        session.instructions = init_node.instructions.clone();
        session.active_tools = init_node.bound_tool_ids.clone();
        session.attached_docs = init_node.attached_knowledge_doc_ids.clone();
        session.active_modules = active_modules;
        session.slots.clear();
        session.history = vec![format!("SWITCH -> {} [{}]", flow.id, init_node.id)];
        session.completed = false;
        true
    }

    /// Set an extracted entity slot
    pub fn set_slot(&self, name: &str, val: Value) {
        let mut session = self.session.write();
        session.slots.insert(name.to_string(), val);
    }

    /// Transition to target node or evaluate next edge
    pub fn transition(&self, target_node_id: &str) -> String {
        let mut session = self.session.write();

        if target_node_id == "call_completed" {
            session.current_node_id = "call_completed".to_string();
            session.current_node_title = "Call Completed".to_string();
            session.instructions = "Caller confirmed no further assistance is needed. Directives: Warmly thank caller and conclude the call.".to_string();
            session.history.push("call_completed".to_string());
            session.active_modules.clear();
            session.completed = true;
            return "call_completed".to_string();
        }

        let flows = self.workflows.read();
        let flow = match flows.get(&session.active_workflow_id) {
            Some(f) => f,
            None => return session.current_node_id.clone(),
        };

        let node = match flow.nodes.get(target_node_id) {
            Some(n) => n,
            None => return session.current_node_id.clone(),
        };

        session.current_node_id = node.id.clone();
        session.current_node_title = node.title.clone();
        session.instructions = node.instructions.clone();
        session.active_tools = node.bound_tool_ids.clone();
        session.attached_docs = node.attached_knowledge_doc_ids.clone();
        session.active_modules = node.modules.iter().map(|m| format!("{}: {}", m.title, m.description)).collect();
        session.history.push(node.id.clone());

        if node.transitions.is_empty() && node.node_type == "end" {
            session.completed = true;
        }

        node.id.clone()
    }

    /// Reset session state
    pub fn reset_session(&self, flow_id: Option<&str>) {
        let flow_key = flow_id.unwrap_or("beacon_reception_triage");
        let flows = self.workflows.read();
        let flow = flows.get(flow_key).or_else(|| flows.get("beacon_reception_triage"));
        if let Some(flow) = flow {
            if let Some(init_node) = flow.nodes.get(&flow.initial_node_id) {
                let active_modules = init_node.modules.iter().map(|m| format!("{}: {}", m.title, m.description)).collect();
                let mut session = self.session.write();
                session.active_workflow_id = flow.id.clone();
                session.workflow_name = flow.name.clone();
                session.current_node_id = init_node.id.clone();
                session.current_node_title = init_node.title.clone();
                session.instructions = init_node.instructions.clone();
                session.active_tools = init_node.bound_tool_ids.clone();
                session.attached_docs = init_node.attached_knowledge_doc_ids.clone();
                session.active_modules = active_modules;
                session.slots.clear();
                session.history = vec![init_node.id.clone()];
                session.completed = false;
            }
        }
    }

    /// Get live session snapshot for UI rendering
    pub fn get_snapshot(&self) -> UnifiedSessionState {
        self.session.read().clone()
    }

    /// Build unified dynamic directives for the conversational LLM / System One engine
    pub fn get_llm_directives(&self) -> String {
        let snap = self.get_snapshot();

        if snap.current_node_id == "call_completed" || snap.completed {
            return "ACTIVE STATUS: CALL CONCLUDED / COMPLETED.\nDIRECTIVES: The caller is saying goodbye or has confirmed no further assistance is needed. Warmly thank the caller for choosing Beacon Health, wish them a great day, and conclude politely. Under NO circumstances should you ask any questions, request patient names, or bring up appointments or other topics.\n".to_string();
        }

        let mut out = String::with_capacity(512);
        out.push_str(&format!("ACTIVE CONSOLIDATED WORKFLOW: {}\n", snap.workflow_name));
        out.push_str(&format!("CURRENT STEP: {} (Node ID: {})\n", snap.current_node_title, snap.current_node_id));
        out.push_str(&format!("GOAL & INSTRUCTIONS: {}\n", snap.instructions));

        if !snap.slots.is_empty() {
            out.push_str("COLLECTED SESSION SLOTS:\n");
            for (k, v) in &snap.slots {
                out.push_str(&format!("  - {}: {}\n", k, v));
            }
        } else {
            out.push_str("COLLECTED SESSION SLOTS: (None yet)\n");
        }

        if snap.completed {
            out.push_str("WORKFLOW STATUS: Completed successfully. Conclude the interaction warmly.\n");
        } else {
            out.push_str("WORKFLOW STATUS: In Progress. Prompt the caller naturally for the required details.\n");
        }

        out
    }
}
