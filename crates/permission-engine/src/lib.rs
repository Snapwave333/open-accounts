use openaccounts_core::{AccountId, ActionType, RiskLevel};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionDecision {
    Allowed,
    ApprovalRequired,
    Denied,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountPolicy {
    pub account_id: AccountId,
    pub action_permissions: HashMap<String, PermissionDecision>,
    pub max_risk_auto_approved: RiskLevel,
}

pub struct PermissionEngine {
    policies: Arc<RwLock<HashMap<AccountId, AccountPolicy>>>,
}

impl PermissionEngine {
    pub fn new() -> Self {
        Self {
            policies: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn set_policy(&self, policy: AccountPolicy) {
        self.policies.write().await.insert(policy.account_id.clone(), policy);
    }

    pub async fn check_permission(
        &self,
        account_id: &AccountId,
        action: &ActionType,
        risk: RiskLevel,
    ) -> PermissionDecision {
        let policies = self.policies.read().await;
        if let Some(policy) = policies.get(account_id) {
            let action_key = serde_json::to_string(action).unwrap_or_default();
            if let Some(&decision) = policy.action_permissions.get(&action_key) {
                return decision;
            }
            if risk > policy.max_risk_auto_approved {
                return PermissionDecision::ApprovalRequired;
            }
        }

        // Default secure fallbacks:
        match action {
            ActionType::ExportCredentials => PermissionDecision::Denied,
            ActionType::ChangePassword | ActionType::DeleteComment => PermissionDecision::ApprovalRequired,
            _ => {
                if risk >= RiskLevel::High {
                    PermissionDecision::ApprovalRequired
                } else {
                    PermissionDecision::Allowed
                }
            }
        }
    }
}
