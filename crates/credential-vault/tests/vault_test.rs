use openaccounts_credential_vault::{SecretPayload, SecretType, VaultManager};
use openaccounts_database::Database;
use std::sync::Arc;

fn payload(v: &str) -> SecretPayload {
    SecretPayload { value: v.into(), metadata: None }
}

#[tokio::test]
async fn first_unlock_sets_password_and_wrong_password_is_rejected() {
    let vault = VaultManager::new();
    assert!(!vault.is_initialized().await);
    assert!(vault.unlock("short").await.is_err());

    vault.unlock("correct horse battery").await.unwrap();
    let id = vault
        .store_secret(Some("acc".into()), SecretType::ApiKey, "x:token".into(), payload("tok-123"))
        .await
        .unwrap();
    vault.lock().await;
    assert!(vault.get_secret(&id).await.is_err());

    let err = vault.unlock("wrong password!").await.unwrap_err();
    assert!(err.to_string().contains("Incorrect master password"));
    assert!(!vault.is_unlocked().await);
    assert_eq!(vault.status().await.failed_attempts, 1);

    vault.unlock("correct horse battery").await.unwrap();
    assert_eq!(vault.get_secret(&id).await.unwrap().value, "tok-123");
    assert_eq!(vault.find_secret(Some("acc"), "x:token").await, Some(id));
}

#[tokio::test]
async fn secrets_survive_restart_and_password_rotation() {
    let dir = std::env::temp_dir().join(format!("oa-vault-{}", uuid::Uuid::new_v4()));
    let path = dir.join("vault.sqlite");

    let id = {
        let db = Arc::new(Database::init(&path).await.unwrap());
        let vault = VaultManager::with_database(db).await.unwrap();
        vault.unlock("first-master-pass").await.unwrap();
        vault
            .store_secret(None, SecretType::Password, "ig:credential".into(), payload("hunter22"))
            .await
            .unwrap()
    };

    {
        let db = Arc::new(Database::init(&path).await.unwrap());
        let vault = VaultManager::with_database(db).await.unwrap();
        assert!(vault.is_initialized().await);
        assert!(vault.unlock("not-the-password").await.is_err());
        vault.unlock("first-master-pass").await.unwrap();
        assert_eq!(vault.get_secret(&id).await.unwrap().value, "hunter22");
        vault.change_master_password("first-master-pass", "second-master-pass").await.unwrap();
    }

    let db = Arc::new(Database::init(&path).await.unwrap());
    let vault = VaultManager::with_database(db).await.unwrap();
    assert!(vault.unlock("first-master-pass").await.is_err());
    vault.unlock("second-master-pass").await.unwrap();
    assert_eq!(vault.get_secret(&id).await.unwrap().value, "hunter22");
    vault.delete_secret(&id).await.unwrap();
    assert!(vault.list_metadata().await.is_empty());
    std::fs::remove_dir_all(&dir).ok();
}
