use chrono::Utc;
use openaccounts_platform_intelligence::{ConfidenceLevel, PlatformIntelligence};
use openaccounts_policy_versioning::PolicyVersioningRegistry;
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
    #[allow(dead_code)]
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

    /// Reviews the stored policy reference data. It does not fetch the live
    /// policy pages (no fetcher is connected); it only flags profiles whose
    /// stored data is marked Low confidence. The policy engine already refuses
    /// multi-account checks for those platforms.
    pub async fn run_policy_check_cycle(&self) -> Vec<PolicyMonitorAlert> {
        info!("Reviewing stored platform policy confidence levels");
        let profiles = self.platform_intelligence.list_profiles().await;
        let mut alerts = Vec::new();

        for profile in profiles {
            if profile.confidence_level == ConfidenceLevel::Low {
                let alert = PolicyMonitorAlert {
                    alert_id: Uuid::new_v4().to_string(),
                    platform_id: profile.id.clone(),
                    severity: "Warning".into(),
                    message: format!(
                        "{} policy reference data has Low confidence and needs re-validation against {}.",
                        profile.name, profile.terms_of_service.official_url
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
}
