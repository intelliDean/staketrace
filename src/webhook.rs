use crate::error::{AppError, Result};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Structured payload sent to webhook URLs upon watch completion or timeout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    pub event: String,
    pub status: String,
    pub total: usize,
    pub accepted: usize,
    pub queued: usize,
    pub not_accepted: usize,
    pub indeterminate: usize,
    pub duration_seconds: u64,
    pub timestamp: String,
    pub receipt: serde_json::Value,
}

impl WebhookPayload {
    /// Creates a new consolidation webhook payload from a receipt.
    pub fn for_consolidation(
        receipt: &crate::models::VerificationReceipt,
        event: &str,
        duration_seconds: u64,
    ) -> Self {
        let summary = &receipt.summary;
        let status = if summary.is_all_accepted() {
            "SUCCESS".to_string()
        } else if summary.has_attention_items() {
            "FAILURE".to_string()
        } else {
            "QUEUED".to_string()
        };

        Self {
            event: event.to_string(),
            status,
            total: summary.total_pairs,
            accepted: summary.accepted,
            queued: summary.queued,
            not_accepted: summary.not_accepted,
            indeterminate: summary.indeterminate,
            duration_seconds,
            timestamp: Utc::now().to_rfc3339(),
            receipt: serde_json::to_value(receipt).unwrap_or(serde_json::Value::Null),
        }
    }

    /// Creates a new exit webhook payload from an exit receipt.
    pub fn for_exit(
        receipt: &crate::exit::models::ExitReceipt,
        event: &str,
        duration_seconds: u64,
    ) -> Self {
        let summary = &receipt.summary;
        let status = if summary.is_all_accepted() {
            "SUCCESS".to_string()
        } else if summary.has_attention_items() {
            "FAILURE".to_string()
        } else {
            "QUEUED".to_string()
        };

        Self {
            event: event.to_string(),
            status,
            total: summary.total_exits,
            accepted: summary.accepted,
            queued: summary.queued,
            not_accepted: summary.not_accepted,
            indeterminate: summary.indeterminate,
            duration_seconds,
            timestamp: Utc::now().to_rfc3339(),
            receipt: serde_json::to_value(receipt).unwrap_or(serde_json::Value::Null),
        }
    }
}

/// Dispatches an HTTP POST request containing the webhook payload.
pub async fn send_webhook(url: &str, payload: &WebhookPayload) -> Result<()> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| {
            AppError::Evaluation(format!("Failed to build HTTP client for webhook: {e}"))
        })?;

    let res = client
        .post(url)
        .json(payload)
        .send()
        .await
        .map_err(|e| AppError::Evaluation(format!("Webhook POST failed: {e}")))?;

    if res.status().is_success() {
        Ok(())
    } else {
        let status = res.status();
        Err(AppError::Evaluation(format!(
            "Webhook endpoint returned error status: {status}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{LidoFeeExemptionReport, VerificationReceipt, VerificationSummary};

    #[test]
    fn test_webhook_payload_for_consolidation_success() {
        let receipt = VerificationReceipt {
            tool_version: "0.3.0".to_string(),
            timestamp: Utc::now(),
            el_rpc_url: "http://127.0.0.1:8545".to_string(),
            cl_beacon_url: "http://127.0.0.1:5052".to_string(),
            summary: VerificationSummary {
                total_pairs: 2,
                accepted: 2,
                queued: 0,
                not_accepted: 0,
                indeterminate: 0,
            },
            fee_exemption: LidoFeeExemptionReport {
                st_vault_dashboard: None,
                role_name: "test".to_string(),
                role_hash: "0x00".to_string(),
                audited_sources: vec![],
                fee_exemption_observed: false,
                notes: "none".to_string(),
            },
            pairs: vec![],
            raw_evidence: None,
        };

        let payload = WebhookPayload::for_consolidation(&receipt, "consolidation_completed", 48);
        assert_eq!(payload.event, "consolidation_completed");
        assert_eq!(payload.status, "SUCCESS");
        assert_eq!(payload.total, 2);
        assert_eq!(payload.accepted, 2);
        assert_eq!(payload.duration_seconds, 48);
    }
}
