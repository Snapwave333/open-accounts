use chrono::Utc;
use openaccounts_platform_intelligence::PlatformIntelligence;
use openaccounts_policy_versioning::{PolicyDiff, PolicyVersioningRegistry};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tracing::{info, warn};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyMonitorAlert {
    pub alert_id: String,
    pub platform_id: String,
    pub severity: String, // "Info", "Warning", "Critical_Pause"
    pub message: String,
    pub affected_accounts: Vec<String>,
    pub created_at: chrono::DateTime<Utc>,
}

pub struct PolicyMonitor {
    platform_intelligence: Arc<PlatformIntelligence>,
    versioning: Arc<PolicyVersioningRegistry>,
    alert_sender: broadcast::Sender<PolicyMonitorAlert>,
}

impl PolicyMonitor {
    pub fn new(
        platform_intelligence: Arc<PlatformIntelligence>,
        versioning: Arc<PolicyVersioningRegistry>,
    ) -> Self {
        let (alert_sender, _) = broadcast::channel(100);
        Self {
            platform_intelligence,
            versioning,
            alert_sender,
        }
    }

    pub fn subscribe_alerts(&self) -> broadcast::Receiver<PolicyMonitorAlert> {
        self.alert_sender.subscribe()
    }

    pub async fn run_policy_check_cycle(&self) -> Vec<PolicyMonitorAlert> {
        info!("Running Policy Intelligence Monitor verification cycle across all platforms...");
        let profiles = self.platform_intelligence.list_profiles().await;
        let mut alerts = Vec::new();

        for profile in profiles {
            // Simulated validation against official endpoint headers
            if profile.confidence_level == openaccounts_platform_intelligence::ConfidenceLevel::Low {
                let alert = PolicyMonitorAlert {
                    alert_id: Uuid::new_v4().to_string(),
                    platform_id: profile.id.clone(),
                    severity: "Critical_Pause".into(),
                    message: format!(
                        "Compliance Uncertainty: {} policy requires re-validation. Automations paused.",
                        profile.name
                    ),
                    affected_accounts: vec![],
                    created_at: Utc::now(),
                };
                warn!("{}", alert.message);
                let _ = self.alert_sender.send(alert.clone());
                alerts.push(alert);
            }
        }

        alerts
    }

    pub async fn simulate_policy_update(
        &self,
        platform_id: &str,
        section_name: &str,
        new_summary: &str,
        requires_pause: bool,
    ) {
        if let Some(mut profile) = self.platform_intelligence.get_profile(platform_id).await {
            let diff = PolicyDiff {
                change_id: Uuid::new_v4().to_string(),
                platform_id: platform_id.to_string(),
                section_modified: section_name.to_string(),
                old_summary: profile.community_guidelines.summary.clone(),
                new_summary: new_summary.to_string(),
                requires_workflow_pause: requires_pause,
                detected_at: Utc::now(),
            };

            profile.community_guidelines.summary = new_summary.to_string();
            profile.last_verified = Utc::now();
            self.versioning
                .record_version(platform_id, profile.clone(), vec![diff.clone()])
                .await;
            self.platform_intelligence.update_profile(profile).await;

            if requires_pause {
                let alert = PolicyMonitorAlert {
                    alert_id: Uuid::new_v4().to_string(),
                    platform_id: platform_id.to_string(),
                    severity: "Critical_Pause".into(),
                    message: format!(
                        "Policy update detected for {} in {}. Workflows paused for review.",
                        platform_id, section_name
                    ),
                    affected_accounts: vec![],
                    created_at: Utc::now(),
                };
                let _ = self.alert_sender.send(alert);
            }
        }
    }
}
