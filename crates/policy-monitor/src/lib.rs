use chrono::{DateTime, Utc};
use futures::{stream, StreamExt};
use openaccounts_database::{hex, Database};
use openaccounts_platform_intelligence::{ConfidenceLevel, PlatformIntelligence, PlatformKnowledgeProfile};
use openaccounts_policy_versioning::{PolicyDiff, PolicyVersioningRegistry};
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::Row;
use std::{
    collections::HashMap,
    sync::{Arc, OnceLock},
    time::Duration,
};
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};
use uuid::Uuid;

pub const SEVERITY_INFO: &str = "Info";
pub const SEVERITY_WARNING: &str = "Warning";
pub const SEVERITY_CRITICAL: &str = "Critical_Pause";

const FETCH_CONCURRENCY: usize = 8;
const FETCH_TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolicyMonitorAlert {
    pub alert_id: String,
    pub platform_id: String,
    pub severity: String,
    pub message: String,
    pub affected_accounts: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PolicySnapshot {
    pub platform_id: String,
    pub url: String,
    pub content_hash: String,
    pub http_status: u16,
    pub checked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PolicyCycleReport {
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub platforms_checked: usize,
    pub documents_checked: usize,
    pub baselines_recorded: usize,
    pub changes_detected: usize,
    pub unreachable: usize,
    pub alerts: Vec<PolicyMonitorAlert>,
}

enum FetchOutcome {
    Fetched { status: u16, hash: String },
    Unreachable { reason: String },
}

fn tag_regexes() -> &'static (Regex, Regex, Regex, Regex) {
    static RE: OnceLock<(Regex, Regex, Regex, Regex)> = OnceLock::new();
    RE.get_or_init(|| {
        (
            Regex::new(r"(?is)<(script|style|noscript|svg|head)[^>]*>.*?</(script|style|noscript|svg|head)>").unwrap(),
            Regex::new(r"(?s)<!--.*?-->").unwrap(),
            Regex::new(r"(?s)<[^>]+>").unwrap(),
            Regex::new(r"\s+").unwrap(),
        )
    })
}

pub fn normalize_policy_document(body: &str) -> String {
    let (blocks, comments, tags, ws) = tag_regexes();
    let text = blocks.replace_all(body, " ");
    let text = comments.replace_all(&text, " ");
    let text = tags.replace_all(&text, " ");
    let text = text
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'");
    ws.replace_all(&text, " ").trim().to_lowercase()
}

pub fn policy_content_hash(body: &str) -> String {
    hex(&Sha256::digest(normalize_policy_document(body).as_bytes()))
}

pub fn official_urls(profile: &PlatformKnowledgeProfile) -> Vec<(String, String)> {
    let mut urls: Vec<(String, String)> = [
        ("Community Guidelines", &profile.community_guidelines.official_url),
        ("Terms of Service", &profile.terms_of_service.official_url),
        ("Automation Policy", &profile.bot_and_automation_policy.official_url),
        ("Multiple Account Policy", &profile.multiple_account_policy.official_url),
        ("Account Creation Requirements", &profile.account_creation_requirements.official_url),
    ]
    .into_iter()
    .filter(|(_, url)| url.starts_with("http://") || url.starts_with("https://"))
    .map(|(section, url)| (section.to_string(), url.clone()))
    .collect();
    let mut seen = std::collections::HashSet::new();
    urls.retain(|(_, url)| seen.insert(url.clone()));
    urls
}

fn alert(platform_id: &str, severity: &str, message: String) -> PolicyMonitorAlert {
    PolicyMonitorAlert {
        alert_id: Uuid::new_v4().to_string(),
        platform_id: platform_id.to_string(),
        severity: severity.to_string(),
        message,
        affected_accounts: vec![],
        created_at: Utc::now(),
    }
}

pub struct PolicyMonitor {
    platform_intelligence: Arc<PlatformIntelligence>,
    versioning: Arc<PolicyVersioningRegistry>,
    alert_sender: broadcast::Sender<PolicyMonitorAlert>,
    client: reqwest::Client,
    db: Option<Arc<Database>>,
    snapshots: RwLock<HashMap<(String, String), PolicySnapshot>>,
    last_report: RwLock<Option<PolicyCycleReport>>,
}

impl PolicyMonitor {
    pub fn new(platform_intelligence: Arc<PlatformIntelligence>, versioning: Arc<PolicyVersioningRegistry>) -> Self {
        let (alert_sender, _) = broadcast::channel(100);
        let client = reqwest::Client::builder()
            .timeout(FETCH_TIMEOUT)
            .user_agent(concat!("OpenAccounts-PolicyMonitor/", env!("CARGO_PKG_VERSION")))
            .build()
            .unwrap_or_default();
        Self {
            platform_intelligence,
            versioning,
            alert_sender,
            client,
            db: None,
            snapshots: RwLock::new(HashMap::new()),
            last_report: RwLock::new(None),
        }
    }

    pub async fn with_database(
        platform_intelligence: Arc<PlatformIntelligence>,
        versioning: Arc<PolicyVersioningRegistry>,
        db: Arc<Database>,
    ) -> openaccounts_core::Result<Self> {
        let mut monitor = Self::new(platform_intelligence, versioning);
        let rows = sqlx::query("SELECT platform_id, url, content_hash, http_status, checked_at FROM policy_snapshots")
            .fetch_all(db.pool())
            .await
            .map_err(|e| openaccounts_core::OpenAccountsError::Database(e.to_string()))?;
        let mut snapshots = HashMap::new();
        for row in rows {
            let snap = PolicySnapshot {
                platform_id: row.get("platform_id"),
                url: row.get("url"),
                content_hash: row.get("content_hash"),
                http_status: row.get::<i64, _>("http_status") as u16,
                checked_at: DateTime::parse_from_rfc3339(&row.get::<String, _>("checked_at"))
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now()),
            };
            snapshots.insert((snap.platform_id.clone(), snap.url.clone()), snap);
        }
        monitor.snapshots = RwLock::new(snapshots);
        monitor.db = Some(db);
        Ok(monitor)
    }

    pub fn subscribe_alerts(&self) -> broadcast::Receiver<PolicyMonitorAlert> {
        self.alert_sender.subscribe()
    }

    pub async fn last_report(&self) -> Option<PolicyCycleReport> {
        self.last_report.read().await.clone()
    }

    pub async fn snapshots(&self) -> Vec<PolicySnapshot> {
        let mut out: Vec<PolicySnapshot> = self.snapshots.read().await.values().cloned().collect();
        out.sort_by(|a, b| a.platform_id.cmp(&b.platform_id).then_with(|| a.url.cmp(&b.url)));
        out
    }

    async fn fetch(&self, url: &str) -> FetchOutcome {
        match self.client.get(url).send().await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                if !resp.status().is_success() {
                    return FetchOutcome::Unreachable { reason: format!("HTTP {status}") };
                }
                match resp.text().await {
                    Ok(body) => FetchOutcome::Fetched { status, hash: policy_content_hash(&body) },
                    Err(e) => FetchOutcome::Unreachable { reason: format!("read error: {e}") },
                }
            }
            Err(e) => FetchOutcome::Unreachable {
                reason: if e.is_timeout() { "timed out".into() } else { format!("{e}") },
            },
        }
    }

    async fn persist(&self, snap: &PolicySnapshot) {
        if let Some(db) = &self.db {
            let res = sqlx::query(
                "INSERT INTO policy_snapshots (platform_id, url, content_hash, http_status, checked_at) VALUES (?, ?, ?, ?, ?) \
                 ON CONFLICT(platform_id, url) DO UPDATE SET content_hash = excluded.content_hash, \
                 http_status = excluded.http_status, checked_at = excluded.checked_at",
            )
            .bind(&snap.platform_id)
            .bind(&snap.url)
            .bind(&snap.content_hash)
            .bind(snap.http_status as i64)
            .bind(snap.checked_at.to_rfc3339())
            .execute(db.pool())
            .await;
            if let Err(e) = res {
                warn!("Failed to persist policy snapshot for {}: {}", snap.url, e);
            }
        }
    }

    pub async fn run_policy_check_cycle(&self) -> Vec<PolicyMonitorAlert> {
        self.run_policy_check_cycle_report().await.alerts
    }

    pub async fn run_policy_check_cycle_report(&self) -> PolicyCycleReport {
        let started_at = Utc::now();
        let profiles = self.platform_intelligence.list_profiles().await;
        info!("Policy monitor: verifying official documents for {} platforms", profiles.len());

        let mut report = PolicyCycleReport {
            started_at: Some(started_at),
            platforms_checked: profiles.len(),
            ..Default::default()
        };

        let targets: Vec<(PlatformKnowledgeProfile, String, String)> = profiles
            .iter()
            .flat_map(|p| official_urls(p).into_iter().map(move |(section, url)| (p.clone(), section, url)))
            .collect();

        let fetched: Vec<_> = stream::iter(targets)
            .map(|(profile, section, url)| async move {
                let outcome = self.fetch(&url).await;
                (profile, section, url, outcome)
            })
            .buffer_unordered(FETCH_CONCURRENCY)
            .collect()
            .await;

        let mut changed_by_platform: HashMap<String, (PlatformKnowledgeProfile, Vec<PolicyDiff>)> = HashMap::new();
        for (profile, section, url, outcome) in fetched {
            report.documents_checked += 1;
            match outcome {
                FetchOutcome::Unreachable { reason } => {
                    report.unreachable += 1;
                    report.alerts.push(alert(
                        &profile.id,
                        SEVERITY_INFO,
                        format!("Could not verify {} {} ({}): {}", profile.name, section, url, reason),
                    ));
                }
                FetchOutcome::Fetched { status, hash } => {
                    let key = (profile.id.clone(), url.clone());
                    let previous = self.snapshots.read().await.get(&key).cloned();
                    let snap = PolicySnapshot {
                        platform_id: profile.id.clone(),
                        url: url.clone(),
                        content_hash: hash.clone(),
                        http_status: status,
                        checked_at: Utc::now(),
                    };
                    match previous {
                        None => report.baselines_recorded += 1,
                        Some(prev) if prev.content_hash != hash => {
                            report.changes_detected += 1;
                            let entry = changed_by_platform
                                .entry(profile.id.clone())
                                .or_insert_with(|| (profile.clone(), Vec::new()));
                            entry.1.push(PolicyDiff {
                                change_id: Uuid::new_v4().to_string(),
                                platform_id: profile.id.clone(),
                                section_modified: section.clone(),
                                old_summary: format!("content sha256 {}", &prev.content_hash[..12.min(prev.content_hash.len())]),
                                new_summary: format!("content sha256 {}", &hash[..12.min(hash.len())]),
                                requires_workflow_pause: true,
                                detected_at: Utc::now(),
                            });
                        }
                        Some(_) => {}
                    }
                    self.persist(&snap).await;
                    self.snapshots.write().await.insert(key, snap);
                }
            }
        }

        for (platform_id, (mut profile, diffs)) in changed_by_platform {
            let sections: Vec<String> = diffs.iter().map(|d| d.section_modified.clone()).collect();
            profile.last_verified = Utc::now();
            self.versioning.record_version(&platform_id, profile.clone(), diffs).await;
            self.platform_intelligence.update_profile(profile.clone()).await;
            report.alerts.push(alert(
                &platform_id,
                SEVERITY_CRITICAL,
                format!(
                    "{} changed its official {}. Automations for this platform should be reviewed before resuming.",
                    profile.name,
                    sections.join(", ")
                ),
            ));
        }

        for profile in &profiles {
            if profile.confidence_level == ConfidenceLevel::Low {
                report.alerts.push(alert(
                    &profile.id,
                    SEVERITY_CRITICAL,
                    format!("Compliance uncertainty: {} policy confidence is Low and needs human re-validation.", profile.name),
                ));
            }
        }

        for a in &report.alerts {
            if a.severity != SEVERITY_INFO {
                warn!("{}", a.message);
            }
            let _ = self.alert_sender.send(a.clone());
        }

        report.finished_at = Some(Utc::now());
        info!(
            "Policy monitor finished: {} documents, {} baselines, {} changes, {} unreachable",
            report.documents_checked, report.baselines_recorded, report.changes_detected, report.unreachable
        );
        *self.last_report.write().await = Some(report.clone());
        report
    }

    pub async fn record_manual_policy_update(
        &self,
        platform_id: &str,
        section_name: &str,
        new_summary: &str,
        requires_pause: bool,
    ) -> openaccounts_core::Result<()> {
        let mut profile = self
            .platform_intelligence
            .get_profile(platform_id)
            .await
            .ok_or_else(|| openaccounts_core::OpenAccountsError::Internal(format!("Platform {platform_id} not registered")))?;

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
        self.versioning.record_version(platform_id, profile.clone(), vec![diff]).await;
        self.platform_intelligence.update_profile(profile).await;

        if requires_pause {
            let _ = self.alert_sender.send(alert(
                platform_id,
                SEVERITY_CRITICAL,
                format!("Operator recorded a policy update for {} in {}. Workflows paused for review.", platform_id, section_name),
            ));
        }
        Ok(())
    }
}
