use openaccounts_database::{AuditInsert, Database};

fn entry<'a>(status: &'a str) -> AuditInsert<'a> {
    AuditInsert {
        execution_id: "exec-1",
        account_id: Some("acc-1"),
        automation_id: None,
        action_type: "\"like_post\"",
        risk_level: "Low",
        details: "{}",
        status,
    }
}

#[tokio::test]
async fn audit_chain_links_entries_and_detects_tampering() {
    let db = Database::in_memory().await.unwrap();
    for status in ["Completed", "Denied", "PausedAwaitingHuman"] {
        db.append_audit(entry(status)).await.unwrap();
    }

    let verification = db.verify_audit_chain().await.unwrap();
    assert!(verification.is_intact);
    assert_eq!(verification.entries_checked, 3);

    let recent = db.recent_audit(10).await.unwrap();
    assert_eq!(recent.len(), 3);
    assert_eq!(recent[0].seq, 3);
    assert_eq!(recent[0].prev_hash, recent[1].entry_hash);

    sqlx::query("UPDATE audit_logs SET status = 'Completed' WHERE seq = 2")
        .execute(db.pool())
        .await
        .unwrap();
    let tampered = db.verify_audit_chain().await.unwrap();
    assert!(!tampered.is_intact);
    assert_eq!(tampered.first_broken_id.as_deref(), Some(recent[1].id.as_str()));
}

#[tokio::test]
async fn legacy_rows_are_backfilled_into_the_chain() {
    let dir = std::env::temp_dir().join(format!("oa-db-{}", uuid::Uuid::new_v4()));
    let path = dir.join("legacy.sqlite");
    {
        let db = Database::init(&path).await.unwrap();
        sqlx::query(
            "INSERT INTO audit_logs (id, execution_id, action_type, risk_level, details, status, created_at) \
             VALUES ('legacy-1', 'e', 'x', 'Low', '{}', 'Completed', '2026-01-01T00:00:00Z')",
        )
        .execute(db.pool())
        .await
        .unwrap();
    }
    let db = Database::init(&path).await.unwrap();
    db.append_audit(entry("Completed")).await.unwrap();
    let verification = db.verify_audit_chain().await.unwrap();
    assert!(verification.is_intact);
    assert_eq!(verification.entries_checked, 2);
    std::fs::remove_dir_all(&dir).ok();
}

#[tokio::test]
async fn settings_round_trip() {
    let db = Database::in_memory().await.unwrap();
    assert_eq!(db.get_setting("theme").await.unwrap(), None);
    db.set_setting("theme", "dark").await.unwrap();
    db.set_setting("theme", "light").await.unwrap();
    assert_eq!(db.get_setting("theme").await.unwrap().as_deref(), Some("light"));
}
