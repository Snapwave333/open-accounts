use openaccounts_platform_intelligence::PlatformIntelligence;
use openaccounts_policy_monitor::{normalize_policy_document, policy_content_hash, PolicyMonitor, SEVERITY_CRITICAL};
use openaccounts_policy_versioning::PolicyVersioningRegistry;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};

async fn serve_versions(versions: Vec<&'static str>) -> (String, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let hits = Arc::new(AtomicUsize::new(0));
    let counter = hits.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else { break };
            let n = counter.fetch_add(1, Ordering::SeqCst);
            let body = versions[n.min(versions.len() - 1)];
            let mut buf = [0u8; 2048];
            let _ = sock.read(&mut buf).await;
            let resp = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/html\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = sock.write_all(resp.as_bytes()).await;
        }
    });
    (format!("http://{addr}/terms"), hits)
}

#[test]
fn normalization_ignores_markup_and_scripts() {
    let a = "<html><head><title>x</title></head><body><script>var t=1</script><p>Be   kind.</p></body></html>";
    let b = "<body><p class='n'>Be kind.</p><script>var t=2</script><!-- build 7 --></body>";
    assert_eq!(normalize_policy_document(a), "be kind.");
    assert_eq!(policy_content_hash(a), policy_content_hash(b));
}

#[tokio::test]
async fn detects_real_document_changes_between_cycles() {
    let (url, hits) = serve_versions(vec![
        "<p>Rule one.</p>",
        "<p>Rule one.</p>",
        "<p>Rule one. Rule two: no bots.</p>",
    ])
    .await;

    let intel = Arc::new(PlatformIntelligence::new());
    let mut profile = intel.get_profile("reddit").await.unwrap();
    for target in [
        &mut profile.community_guidelines.official_url,
        &mut profile.terms_of_service.official_url,
        &mut profile.bot_and_automation_policy.official_url,
        &mut profile.multiple_account_policy.official_url,
        &mut profile.account_creation_requirements.official_url,
    ] {
        *target = url.clone();
    }
    for other in intel.list_profiles().await {
        if other.id != "reddit" {
            let mut p = other.clone();
            p.community_guidelines.official_url.clear();
            p.terms_of_service.official_url.clear();
            p.bot_and_automation_policy.official_url.clear();
            p.multiple_account_policy.official_url.clear();
            p.account_creation_requirements.official_url.clear();
            intel.update_profile(p).await;
        }
    }
    intel.update_profile(profile).await;

    let versioning = Arc::new(PolicyVersioningRegistry::new());
    let monitor = PolicyMonitor::new(intel.clone(), versioning.clone());

    let first = monitor.run_policy_check_cycle_report().await;
    assert_eq!(first.documents_checked, 1);
    assert_eq!(first.baselines_recorded, 1);
    assert_eq!(first.changes_detected, 0);

    let second = monitor.run_policy_check_cycle_report().await;
    assert_eq!(second.changes_detected, 0);

    let third = monitor.run_policy_check_cycle_report().await;
    assert_eq!(third.changes_detected, 1);
    assert!(third
        .alerts
        .iter()
        .any(|a| a.platform_id == "reddit" && a.severity == SEVERITY_CRITICAL && a.kind == "document_changed" && a.urls == vec![url.clone()]));
    assert_eq!(versioning.get_history("reddit").await.len(), 1);
    assert_eq!(hits.load(Ordering::SeqCst), 3);
}
