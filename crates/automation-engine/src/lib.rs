use openaccounts_audit_engine::AuditEngine;
use openaccounts_core::{AccountId, ActionType, AutomationId, OpenAccountsError, Result, RiskLevel};
use openaccounts_human_intervention::HumanInterventionController;
use openaccounts_lm_gateway::LMGateway;
use openaccounts_permission_engine::{PermissionDecision, PermissionEngine};
use serde::{Deserialize, Serialize};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRule {
    pub id: AutomationId,
    pub account_id: AccountId,
    pub name: String,
    pub module: String,
    pub config: serde_json::Value,
    pub enabled: bool,
}

pub struct AutomationEngine {
    permission_engine: Arc<PermissionEngine>,
    audit_engine: Arc<AuditEngine>,
    pub lm_gateway: Arc<LMGateway>,
    human_controller: Arc<HumanInterventionController>,
    kill_switch: Arc<AtomicBool>,
}

impl AutomationEngine {
    pub fn new(
        permission_engine: Arc<PermissionEngine>,
        audit_engine: Arc<AuditEngine>,
        lm_gateway: Arc<LMGateway>,
        human_controller: Arc<HumanInterventionController>,
    ) -> Self {
        Self {
            permission_engine,
            audit_engine,
            lm_gateway,
            human_controller,
            kill_switch: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn trigger_kill_switch(&self) {
        self.kill_switch.store(true, Ordering::SeqCst);
        warn!("EMERGENCY KILL SWITCH ACTIVATED: Halting all automations immediately.");
    }

    pub fn reset_kill_switch(&self) {
        self.kill_switch.store(false, Ordering::SeqCst);
        info!("Kill switch reset: automations permitted to resume.");
    }

    pub fn is_kill_switch_active(&self) -> bool {
        self.kill_switch.load(Ordering::SeqCst)
    }

    pub async fn execute_action(
        &self,
        execution_id: &str,
        account_id: &AccountId,
        automation_id: &AutomationId,
        action: &ActionType,
        risk: RiskLevel,
        payload: serde_json::Value,
    ) -> Result<()> {
        if self.is_kill_switch_active() {
            return Err(OpenAccountsError::KillSwitchActivated);
        }

        let decision = self
            .permission_engine
            .check_permission(account_id, action, risk)
            .await;

        match decision {
            PermissionDecision::Denied => {
                self.audit_engine
                    .log_action(
                        execution_id,
                        Some(&account_id.0.to_string()),
                        Some(&automation_id.0.to_string()),
                        action,
                        risk,
                        payload,
                        "Denied",
                    )
                    .await?;
                Err(OpenAccountsError::PermissionDenied(format!(
                    "Action {:?} denied by account policy",
                    action
                )))
            }
            PermissionDecision::ApprovalRequired => {
                self.human_controller
                    .create_task(
                        Some(account_id.0.to_string()),
                        Some(automation_id.0.to_string()),
                        openaccounts_browser_abstraction::ChallengeType::ManualReview,
                        format!("Approval Required: {:?}", action),
                        format!("Action {:?} requires explicit human authorization.", action),
                        None,
                    )
                    .await?;

                self.audit_engine
                    .log_action(
                        execution_id,
                        Some(&account_id.0.to_string()),
                        Some(&automation_id.0.to_string()),
                        action,
                        risk,
                        payload,
                        "PausedAwaitingHuman",
                    )
                    .await?;

                Err(OpenAccountsError::HumanInterventionRequired(
                    "Approval required before execution".into(),
                ))
            }
            PermissionDecision::Allowed => {
                info!("Authorized action {:?} for account {}", action, account_id);
                self.audit_engine
                    .log_action(
                        execution_id,
                        Some(&account_id.0.to_string()),
                        Some(&automation_id.0.to_string()),
                        action,
                        risk,
                        payload,
                        "Authorized",
                    )
                    .await?;
                Ok(())
            }
        }
    }
}
