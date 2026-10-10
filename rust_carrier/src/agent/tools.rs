//! Sub-Microsecond In-Memory Clinical Tool Dispatch Engine

use rand::Rng;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolExecutionResult {
    pub tool_name: String,
    pub duration_microseconds: f64,
    pub duration_milliseconds: f64,
    pub status: String,
    pub result: Value,
}

#[derive(Default)]
pub struct ToolRegistry;

impl ToolRegistry {
    pub fn new() -> Self {
        Self
    }

    #[inline]
    pub fn execute(&self, name: &str, params: HashMap<String, Value>) -> ToolExecutionResult {
        let start = Instant::now();
        let (status, result) = match name {
            "list_clinic_services" => (
                "success",
                json!({
                    "clinic_name": "Beacon Health Comprehensive Center",
                    "departments": [
                        "Cardiology & Vascular Medicine",
                        "Dermatology & Skin Surgery",
                        "Family & Internal Medicine",
                        "Pediatrics & Child Wellness",
                        "Dental Care & Oral Surgery",
                        "Urgent Care Walk-In Center"
                    ],
                    "pharmacy": "In-House 24/7 Dispensary & Prescription Delivery",
                    "lab": "Fast-Track CLIA-Certified Diagnostic Lab"
                }),
            ),

            "lookup_doctor_profile" => {
                let doctor = params
                    .get("doctor_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Dr. Marcus Vance");
                if doctor.contains("Lin") || doctor.contains("Dermatology") {
                    (
                        "success",
                        json!({
                            "doctor_name": "Dr. Sarah Lin, MD, FAAD",
                            "specialty": "Dermatology & Cutaneous Surgery",
                            "certifications": ["American Board of Dermatology (ABD)", "FAAD"],
                            "education": "Stanford University School of Medicine (MD), UCSF Residency",
                            "experience_years": 12,
                            "next_available": "Tomorrow at 1:15 PM"
                        }),
                    )
                } else if doctor.contains("Pendelton") || doctor.contains("Dental") {
                    (
                        "success",
                        json!({
                            "doctor_name": "Dr. Arthur Pendelton, DDS",
                            "specialty": "General & Restorative Dentistry",
                            "certifications": ["American Board of General Dentistry", "ADA Member"],
                            "education": "Columbia University College of Dental Medicine",
                            "experience_years": 14,
                            "next_available": "Tomorrow at 4:00 PM"
                        }),
                    )
                } else {
                    (
                        "success",
                        json!({
                            "doctor_name": "Dr. Marcus Vance, MD, FACC",
                            "specialty": "Cardiovascular Disease & Internal Medicine",
                            "certifications": ["ABIM Board Certified in Cardiovascular Disease", "FACC"],
                            "education": "Johns Hopkins School of Medicine (MD), Harvard Brigham Residency",
                            "experience_years": 16,
                            "next_available": "Tomorrow at 9:30 AM"
                        }),
                    )
                }
            }

            "check_calendar_availability" => {
                let dept = params
                    .get("department")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Cardiology");
                let (doctor, slots) = match dept {
                    "Dermatology" => (
                        "Dr. Sarah Lin, MD, FAAD",
                        vec!["10:00 AM", "1:15 PM", "3:45 PM"],
                    ),
                    "Dental" => (
                        "Dr. Arthur Pendelton, DDS",
                        vec!["8:45 AM", "11:30 AM", "4:00 PM"],
                    ),
                    _ => (
                        "Dr. Marcus Vance, MD, FACC",
                        vec!["9:30 AM", "1:15 PM", "4:00 PM"],
                    ),
                };
                (
                    "success",
                    json!({
                        "department": dept,
                        "doctor": doctor,
                        "available_slots": slots,
                        "date": "Tomorrow"
                    }),
                )
            }

            "confirm_appointment_booking" => {
                let patient = params
                    .get("patient_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Caller");
                let dept = params
                    .get("department")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Cardiology");
                let slot = params
                    .get("time_slot")
                    .and_then(|v| v.as_str())
                    .unwrap_or("9:30 AM");
                let booking_id = format!("APT-{}", rand::thread_rng().gen_range(10000..99999));
                (
                    "success",
                    json!({
                        "booking_id": booking_id,
                        "patient_name": patient,
                        "department": dept,
                        "time_slot": slot,
                        "status": "confirmed",
                        "sms_confirmation": "dispatched"
                    }),
                )
            }

            "lookup_prescription" | "verify_prescription_refill" => {
                let med = params
                    .get("medication")
                    .or_else(|| params.get("medication_name"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Lipitor (Atorvastatin 20mg)");
                let fulfillment = params
                    .get("fulfillment")
                    .or_else(|| params.get("fulfillment_method"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("Express Home Delivery");
                let refill_id = format!("REFILL-{}", rand::thread_rng().gen_range(10000..99999));
                let tracking = format!("TRK-MED-{}", rand::thread_rng().gen_range(100000..999999));
                (
                    "success",
                    json!({
                        "rx_number": "RX-84920",
                        "medication": med,
                        "refill_order_id": refill_id,
                        "fulfillment_method": fulfillment,
                        "copay_charged": "$12.00",
                        "estimated_arrival": "Tomorrow by 11:00 AM",
                        "tracking_number": tracking,
                        "status": "refill_confirmed"
                    }),
                )
            }

            "lookup_lab_results" | "get_recent_lab_results" => (
                "success",
                json!({
                    "patient_id": "PT-39210",
                    "panel": "Comprehensive Fasting Lipid & Metabolic Panel",
                    "date_collected": "Yesterday",
                    "total_cholesterol": "178 mg/dL (Normal < 200)",
                    "triglycerides": "124 mg/dL (Normal < 150)",
                    "hdl_cholesterol": "58 mg/dL (Optimal > 40)",
                    "ldl_cholesterol": "95 mg/dL (Optimal < 100)",
                    "status": "Normal / Cleared by Physician"
                }),
            ),

            "lookup_patient_billing" | "check_billing_balance" => {
                let pay_now = params
                    .get("pay_now")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if pay_now {
                    let auth_code = format!("AUTH-{}", rand::thread_rng().gen_range(100000..999999));
                    (
                        "success",
                        json!({
                            "invoice_id": "INV-88241",
                            "amount_paid": "$35.00",
                            "balance_remaining": "$0.00",
                            "payment_method": "Visa ending in 4281",
                            "authorization_code": auth_code,
                            "receipt": "Emailed & SMS Dispatched"
                        }),
                    )
                } else {
                    (
                        "success",
                        json!({
                            "invoice_id": "INV-88241",
                            "service_description": "Office Visit & Specialist Consultation",
                            "insurance_covered": "$305.00",
                            "copay_due": "$35.00",
                            "due_date": "Within 14 days",
                            "status": "outstanding"
                        }),
                    )
                }
            }

            "process_copay_payment" => {
                let auth_code = format!("AUTH-{}", rand::thread_rng().gen_range(100000..999999));
                (
                    "success",
                    json!({
                        "invoice_id": "INV-88241",
                        "amount_paid": "$35.00",
                        "balance_remaining": "$0.00",
                        "payment_method": "Visa ending in 4281",
                        "authorization_code": auth_code,
                        "receipt": "Emailed & SMS Dispatched"
                    }),
                )
            }

            "cancel_appointment" => (
                "success",
                json!({
                    "booking_id": "APT-49210",
                    "doctor": "Dr. Marcus Vance",
                    "cancellation_fee": "$0.00",
                    "status": "cancelled",
                    "slot_released": true
                }),
            ),

            "clinical_symptom_triage" | "triage_symptom_urgency" => (
                "success",
                json!({
                    "acuity_level": 3,
                    "classification": "Urgent Same-Day (Non-Emergency)",
                    "emergency_red_flags": "None detected",
                    "recommendation": "Fast-track Walk-In Check-In at Suite 100",
                    "transfer": "On-call triage RN notified"
                }),
            ),

            "reschedule_appointment" => {
                let booking_id = params.get("booking_id").and_then(|v| v.as_str()).unwrap_or("APT-49210");
                let new_date = params.get("new_date").and_then(|v| v.as_str()).unwrap_or("Next Monday");
                let new_time = params.get("new_time").and_then(|v| v.as_str()).unwrap_or("02:30 PM");
                (
                    "success",
                    json!({
                        "status": "rescheduled_confirmed",
                        "booking_id": booking_id,
                        "previous_slot": "This Friday 10:00 AM",
                        "new_date": new_date,
                        "new_time": new_time,
                        "calendar_status": "updated",
                        "sms_sent": true
                    }),
                )
            }

            "lookup_active_appointments" => (
                "success",
                json!({
                    "status": "found",
                    "active_booking": {
                        "booking_id": "APT-49210",
                        "department": "Cardiology",
                        "doctor": "Dr. Marcus Vance",
                        "date": "This Friday",
                        "time_slot": "10:00 AM",
                        "facility": "Beacon Health Pavilion Room 302"
                    }
                }),
            ),

            "send_lab_report_portal" => {
                let patient_id = params.get("patient_id").and_then(|v| v.as_str()).unwrap_or("PT-39210");
                (
                    "success",
                    json!({
                        "status": "delivered",
                        "patient_id": patient_id,
                        "destination": "Beacon Patient Portal & Verified Email",
                        "file_format": "Encrypted PDF"
                    }),
                )
            }

            "check_medication_stock" => {
                let med = params.get("medication_name").and_then(|v| v.as_str()).unwrap_or("Prescribed Medication");
                (
                    "success",
                    json!({
                        "status": "in_stock",
                        "medication": med,
                        "units_available": 142,
                        "pharmacy_location": "Beacon Clinic Central Dispensary",
                        "curbside_ready_in": "30 minutes",
                        "delivery_eligible": true
                    }),
                )
            }

            "confirm_rx_refill" => {
                let rx_num = params.get("rx_number").and_then(|v| v.as_str()).unwrap_or("RX-84920");
                let fulfillment = params.get("fulfillment_method").and_then(|v| v.as_str()).unwrap_or("Express Home Delivery");
                let refill_id = format!("REFILL-{}", rand::thread_rng().gen_range(10000..99999));
                let tracking = format!("TRK-MED-{}", rand::thread_rng().gen_range(100000..999999));
                (
                    "success",
                    json!({
                        "status": "refill_confirmed",
                        "refill_order_id": refill_id,
                        "rx_number": rx_num,
                        "fulfillment_method": fulfillment,
                        "estimated_arrival": "Tomorrow by 11:00 AM",
                        "copay_charged": "$12.00",
                        "tracking_number": tracking
                    }),
                )
            }

            "book_urgent_care_queue" => {
                let patient = params.get("patient_name").and_then(|v| v.as_str()).unwrap_or("Caller");
                let pos = rand::thread_rng().gen_range(1..15);
                let code = format!("UC-{}", rand::thread_rng().gen_range(1000..9999));
                (
                    "success",
                    json!({
                        "status": "queued",
                        "patient_name": patient,
                        "queue_position": pos,
                        "estimated_wait": format!("{} minutes", pos * 4),
                        "check_in_code": code,
                        "clinic_address": "Beacon Urgent Care Center, Suite 100"
                    }),
                )
            }

            "lookup_order" => {
                let order_id = params.get("order_id").and_then(|v| v.as_str()).unwrap_or("ORD-98214");
                (
                    "success",
                    json!({
                        "order_id": order_id,
                        "status": "Delivered",
                        "delivered_timestamp": "Yesterday 2:15 PM",
                        "item": "Ultra-Low Latency Sound Card Pro",
                        "amount": "$189.00",
                        "eligible_for_return": true
                    }),
                )
            }

            "create_return_label" => {
                let order_id = params.get("order_id").and_then(|v| v.as_str()).unwrap_or("ORD-98214");
                let reason = params.get("reason").and_then(|v| v.as_str()).unwrap_or("No longer needed");
                let rma = format!("RMA-{}", rand::thread_rng().gen_range(100000..999999));
                (
                    "success",
                    json!({
                        "status": "label_created",
                        "rma_number": rma,
                        "order_id": order_id,
                        "reason": reason,
                        "carrier": "Priority Overnight Air",
                        "email_status": "sent"
                    }),
                )
            }

            _ => (
                "not_found",
                json!({
                    "error": format!("Tool '{}' is not registered", name)
                }),
            ),
        };

        let elapsed = start.elapsed();
        let us = elapsed.as_micros() as f64;
        let ms = us / 1000.0;

        ToolExecutionResult {
            tool_name: name.to_string(),
            duration_microseconds: us,
            duration_milliseconds: ms,
            status: status.to_string(),
            result,
        }
    }
}
