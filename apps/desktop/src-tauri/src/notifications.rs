use openaccounts_human_intervention::{HumanTask, HumanTaskStatus};
use tauri::{AppHandle, Runtime};
use tauri_plugin_notification::NotificationExt;
use tokio::sync::broadcast::{error::RecvError, Receiver};
use tracing::warn;

pub fn notification_text(task: &HumanTask) -> Option<(String, String)> {
    if task.status != HumanTaskStatus::Pending {
        return None;
    }
    let body: String = task.instructions.chars().take(180).collect();
    Some((format!("OpenAccounts needs you: {}", task.title), body))
}

pub fn spawn<R: Runtime>(app: AppHandle<R>, mut events: Receiver<HumanTask>) {
    tauri::async_runtime::spawn(async move {
        loop {
            match events.recv().await {
                Ok(task) => {
                    if let Some((title, body)) = notification_text(&task) {
                        if let Err(e) = app.notification().builder().title(title).body(body).show() {
                            warn!("Desktop notification failed: {}", e);
                        }
                    }
                }
                Err(RecvError::Lagged(skipped)) => warn!("Skipped {} task notifications", skipped),
                Err(RecvError::Closed) => break,
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use openaccounts_browser_abstraction::ChallengeType;
    use openaccounts_human_intervention::HumanInterventionController;

    #[tokio::test]
    async fn only_new_pending_tasks_produce_notifications() {
        let gate = HumanInterventionController::new();
        let mut events = gate.subscribe();
        let id = gate
            .create_task(None, None, ChallengeType::Captcha, "Solve CAPTCHA on X".into(), "Complete the puzzle".into(), None)
            .await
            .unwrap();
        let created = events.recv().await.unwrap();
        let (title, body) = notification_text(&created).unwrap();
        assert_eq!(title, "OpenAccounts needs you: Solve CAPTCHA on X");
        assert_eq!(body, "Complete the puzzle");

        gate.resolve_task(&id).await.unwrap();
        let resolved = events.recv().await.unwrap();
        assert!(notification_text(&resolved).is_none());
    }
}
