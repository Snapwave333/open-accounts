use async_trait::async_trait;
use chrono::{DateTime, Utc};
use openaccounts_core::{AccountId, Result};
use openaccounts_email_identity::EmailIdentityManager;
use openaccounts_email_provider_gateway::{plus_alias, EmailProviderAdapter, EmailProviderType, IncomingEmailMessage};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

struct TestMailbox {
    counter: AtomicUsize,
}

#[async_trait]
impl EmailProviderAdapter for TestMailbox {
    fn provider_type(&self) -> EmailProviderType {
        EmailProviderType::GmailApi
    }

    async fn mailbox_address(&self) -> Result<String> {
        Ok("owner@gmail.com".into())
    }

    async fn send_email(&self, _to: &str, _subject: &str, _body_text: &str) -> Result<String> {
        Ok("sent".into())
    }

    async fn fetch_recent_messages(&self, _limit: u32, _since: Option<DateTime<Utc>>) -> Result<Vec<IncomingEmailMessage>> {
        Ok(vec![])
    }

    async fn create_alias_or_mailbox(&self, prefix: &str) -> Result<String> {
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        plus_alias("owner@gmail.com", prefix, &n.to_string())
    }
}

#[tokio::test]
async fn allocation_uses_real_provider_type_and_orders_newest_first() {
    let manager = EmailIdentityManager::new();
    assert!(manager.allocate_identity("gmail", "x", None).await.is_err());

    manager
        .register_provider("gmail", Arc::new(TestMailbox { counter: AtomicUsize::new(0) }))
        .await;
    assert_eq!(manager.list_providers().await, vec!["gmail".to_string()]);
    assert!(manager.provider("gmail").await.is_some());

    let first = manager.allocate_identity("gmail", "Insta", Some("Instagram".into())).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let second = manager.allocate_identity("gmail", "insta", Some("instagram".into())).await.unwrap();

    assert_eq!(first.provider_type, EmailProviderType::GmailApi);
    assert_eq!(first.email_address, "owner+insta-0@gmail.com");
    assert_eq!(first.platform.as_deref(), Some("instagram"));
    assert!(first.is_primary);
    assert!(!second.is_primary);

    let listed = manager.list_identities().await;
    assert_eq!(listed[0].identity_id, second.identity_id);

    let account = AccountId::new();
    manager.assign_to_account(&first.identity_id, account.clone()).await.unwrap();
    let updated = manager.list_identities().await;
    assert_eq!(
        updated.iter().find(|i| i.identity_id == first.identity_id).unwrap().assigned_account_id,
        Some(account)
    );
}
