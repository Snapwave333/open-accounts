use openaccounts_browser_abstraction::ChallengeType;
use openaccounts_human_intervention::HumanInterventionController;
use openaccounts_policy_monitor::{PolicyCycleReport, ALERT_DOCUMENT_CHANGED};
use tracing::warn;

pub async fn escalate_policy_changes(gate: &HumanInterventionController, report: &PolicyCycleReport) -> usize {
    let open_titles: Vec<String> = gate.list_pending_tasks().await.into_iter().map(|t| t.title).collect();
    let mut created = 0;
    for alert in report.alerts.iter().filter(|a| a.kind == ALERT_DOCUMENT_CHANGED) {
        let title = format!("Review {} policy change", alert.platform_id);
        if open_titles.contains(&title) {
            continue;
        }
        let instructions = format!(
            "{} Read the updated documents, then resolve this task once automations for {} are still compliant.{}",
            alert.message,
            alert.platform_id,
            if alert.urls.is_empty() { String::new() } else { format!(" Changed: {}", alert.urls.join(", ")) }
        );
        match gate
            .create_task(None, None, ChallengeType::ManualReview, title, instructions, alert.urls.first().cloned())
            .await
        {
            Ok(_) => created += 1,
            Err(e) => warn!("Could not create policy review task: {}", e),
        }
    }
    created
}

#[cfg(test)]
mod tests {
    use super::*;
    use openaccounts_policy_monitor::PolicyMonitorAlert;

    fn alert(kind: &str, platform: &str) -> PolicyMonitorAlert {
        PolicyMonitorAlert {
            kind: kind.into(),
            urls: vec![format!("https://{platform}.example/terms")],
            alert_id: uuid::Uuid::new_v4().to_string(),
            platform_id: platform.into(),
            severity: "Critical_Pause".into(),
            message: format!("{platform} changed its official Terms of Service."),
            affected_accounts: vec![],
            created_at: chrono::Utc::now(),
        }
    }

    #[tokio::test]
    async fn only_document_changes_become_deduplicated_review_tasks() {
        let gate = HumanInterventionController::new();
        let report = PolicyCycleReport {
            alerts: vec![alert("document_changed", "reddit"), alert("low_confidence", "x"), alert("unreachable", "tiktok")],
            ..Default::default()
        };
        assert_eq!(escalate_policy_changes(&gate, &report).await, 1);
        assert_eq!(escalate_policy_changes(&gate, &report).await, 0, "an open review task is not duplicated");
        let tasks = gate.list_pending_tasks().await;
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].title, "Review reddit policy change");
        assert_eq!(tasks[0].page_url.as_deref(), Some("https://reddit.example/terms"));
    }
}
