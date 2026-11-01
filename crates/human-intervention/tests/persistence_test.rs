use openaccounts_browser_abstraction::ChallengeType;
use openaccounts_database::Database;
use openaccounts_human_intervention::{HumanInterventionController, HumanTaskStatus};
use std::sync::Arc;

#[tokio::test]
async fn tasks_persist_and_cannot_be_resolved_twice() {
    let dir = std::env::temp_dir().join(format!("oa-ht-{}", uuid::Uuid::new_v4()));
    let path = dir.join("tasks.sqlite");

    let (first, second) = {
        let db = Arc::new(Database::init(&path).await.unwrap());
        let gate = HumanInterventionController::with_database(db).await.unwrap();
        let first = gate
            .create_task(None, None, ChallengeType::Captcha, "Solve CAPTCHA".into(), "Complete it".into(), None)
            .await
            .unwrap();
        let second = gate
            .create_task(Some("acc".into()), None, ChallengeType::TwoFactorCode, "Enter 2FA".into(), "Code".into(), None)
            .await
            .unwrap();
        gate.resolve_task(&first).await.unwrap();
        assert!(gate.resolve_task(&first).await.is_err());
        (first, second)
    };

    let db = Arc::new(Database::init(&path).await.unwrap());
    let gate = HumanInterventionController::with_database(db).await.unwrap();
    let pending = gate.list_pending_tasks().await;
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].id, second);
    assert_eq!(pending[0].title, "Enter 2FA");
    assert_eq!(gate.get_task(&first).await.unwrap().status, HumanTaskStatus::Resolved);
    gate.dismiss_task(&second).await.unwrap();
    assert!(gate.list_pending_tasks().await.is_empty());
    std::fs::remove_dir_all(&dir).ok();
}
