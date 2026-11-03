//! Campaign management view: every registered campaign (the content-strategy
//! registry) joined with what is actually happening to it: the accounts in
//! the registry, the agents the bridge pairs with it, the supervised job runs
//! persisted by the autonomy runtime, the open human gates, and the metrics
//! the studio and the Nova pipeline record on disk. Nothing is invented; a
//! campaign with no data shows no data.

use crate::autonomy::{AutonomyCycleReport, JobRoot};
use crate::state::AppState;
use openaccounts_account_registry::AccountRecord;
use openaccounts_brand_manager::BrandProfile;
use openaccounts_content_strategy::CampaignBrief;
use openaccounts_human_intervention::HumanTask;
use openaccounts_social_media_manager::bridge::{StickBridgeMessage, StickEpisodeSummary};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

const CDN: &str = "campaign_cdn";
const NOVA: &str = "campaign_nova";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignAccount {
    pub id: String,
    pub platform: String,
    pub username: String,
    pub status: String,
    pub last_active: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignAgent {
    pub name: String,
    pub role: String,
    /// "openaccounts" | "studio" | "pipeline"
    pub side: String,
    pub channel: String,
    /// The supervised jobs this agent is responsible for.
    pub jobs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignIssue {
    /// "error" | "blocked" | "warning"
    pub severity: String,
    pub title: String,
    pub detail: String,
    pub at: Option<String>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignMetric {
    pub label: String,
    pub value: String,
    pub numeric: Option<f64>,
    pub target: Option<f64>,
    pub source: String,
    pub captured_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignActivity {
    pub cycle_id: String,
    pub at: String,
    pub job: String,
    /// "done" | "failed" | "timed_out" | "waiting" | "skipped"
    pub status: String,
    pub summary: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignProgress {
    pub label: String,
    pub done: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignOverview {
    pub brief: CampaignBrief,
    pub brand: Option<BrandProfile>,
    pub accounts: Vec<CampaignAccount>,
    pub agents: Vec<CampaignAgent>,
    pub last_activity_at: Option<String>,
    pub last_cycle_status: Option<String>,
    pub jobs_done: u64,
    pub jobs_failed: u64,
    pub jobs_waiting: u64,
    pub current_job: Option<String>,
    pub issues: Vec<CampaignIssue>,
    pub outstanding: Vec<String>,
    pub metrics: Vec<CampaignMetric>,
    pub progress: Vec<CampaignProgress>,
    pub pending_tasks: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CampaignDetail {
    pub overview: CampaignOverview,
    pub activities: Vec<CampaignActivity>,
    pub cycles: Vec<CycleSummary>,
    pub pending_tasks: Vec<HumanTask>,
    pub bridge_messages: Vec<StickBridgeMessage>,
    pub episodes: Vec<StickEpisodeSummary>,
    pub calendar: Vec<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CycleSummary {
    pub cycle_id: String,
    pub started_at: String,
    pub finished_at: String,
    pub status: String,
    pub done: u64,
    pub failed: u64,
    pub waiting: u64,
}

fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn account_is_nova(a: &AccountRecord) -> bool {
    let meta = &a.profile_metadata;
    meta.get("campaign_id").and_then(|v| v.as_str()) == Some(NOVA)
        || meta
            .get("brand")
            .and_then(|v| v.as_str())
            .is_some_and(|b| b.contains("Nova Vexley"))
        || a.username.to_lowercase().contains("novavexley")
}

/// Which campaign a supervised job belongs to.
fn job_campaign(job: &str) -> &'static str {
    if job.starts_with("Nova ") {
        NOVA
    } else {
        CDN
    }
}

fn job_status(job: &crate::autonomy::AutonomyJobReport) -> &'static str {
    if job.timed_out {
        "timed_out"
    } else if !job.success {
        "failed"
    } else if job.skipped && job.exit_code == Some(3) {
        "waiting"
    } else if job.skipped {
        "skipped"
    } else {
        "done"
    }
}

fn fmt_num(v: f64) -> String {
    if v.fract() == 0.0 {
        let n = v as i64;
        let s = n.abs().to_string();
        let mut out = String::new();
        for (i, c) in s.chars().enumerate() {
            if i > 0 && (s.len() - i) % 3 == 0 {
                out.push(',');
            }
            out.push(c);
        }
        if n < 0 {
            format!("-{out}")
        } else {
            out
        }
    } else {
        format!("{v:.1}")
    }
}

/// Agents responsible for each campaign, from the bridge pairings (CDN) and
/// the Nova pipeline's own stages.
fn agents_for(campaign: &str, pairings: &[openaccounts_social_media_manager::bridge::StickAgentPairing], jobs: &[(String, JobRoot)]) -> Vec<CampaignAgent> {
    let mut out = Vec::new();
    if campaign == CDN {
        let cdn_jobs: Vec<String> = jobs
            .iter()
            .filter(|(_, r)| *r == JobRoot::Studio)
            .map(|(n, _)| n.clone())
            .collect();
        for p in pairings {
            let owned: Vec<String> = match p.openaccounts_agent.as_str() {
                "CommunityManager" => cdn_jobs
                    .iter()
                    .filter(|j| j.starts_with("Comment") || j.starts_with("Social") || j.starts_with("Engagement"))
                    .cloned()
                    .collect(),
                "ComplianceAgent" => cdn_jobs.iter().filter(|j| j.starts_with("Retention")).cloned().collect(),
                _ => Vec::new(),
            };
            out.push(CampaignAgent {
                name: format!("OpenAccounts · {}", p.openaccounts_agent),
                role: p.openaccounts_title.clone(),
                side: "openaccounts".into(),
                channel: p.channel.clone(),
                jobs: owned,
            });
            for (name, role) in p.stick_agents.iter().zip(p.stick_roles.iter()) {
                out.push(CampaignAgent {
                    name: name.clone(),
                    role: role.clone(),
                    side: "studio".into(),
                    channel: p.channel.clone(),
                    jobs: Vec::new(),
                });
            }
        }
    } else {
        for (name, root) in jobs {
            if *root != JobRoot::Influencer {
                continue;
            }
            let stage = name.trim_start_matches("Nova ").to_string();
            out.push(CampaignAgent {
                name: format!("Nova pipeline · {stage}"),
                role: format!("influencer/pipeline.py {stage}"),
                side: "pipeline".into(),
                channel: "Nova Vexley (Instagram, photos only)".into(),
                jobs: vec![name.clone()],
            });
        }
    }
    out
}

fn cdn_metrics(studio: &Path) -> Vec<CampaignMetric> {
    let mut m = Vec::new();
    if let Some(y) = read_json(&studio.join("insights").join("ypp.json")) {
        let at = y.get("updated").and_then(|v| v.as_str()).map(str::to_string);
        for (key, label, target) in [
            ("subscribers", "YouTube subscribers", Some(1000.0)),
            ("watch_hours_12m", "YouTube watch hours (12 mo)", Some(4000.0)),
            ("shorts_views_90d", "YouTube Shorts views (90 d)", None),
        ] {
            if let Some(v) = y.get(key).and_then(|v| v.as_f64()) {
                m.push(CampaignMetric {
                    label: label.into(),
                    value: fmt_num(v),
                    numeric: Some(v),
                    target,
                    source: "insights/ypp.json (YouTube Analytics API)".into(),
                    captured_at: at.clone(),
                });
            }
        }
    }
    if let Some(t) = read_json(&studio.join("insights").join("tiktok.json")) {
        let at = t.get("at").and_then(|v| v.as_str()).map(str::to_string);
        let target = t.pointer("/targets/followers").and_then(|v| v.as_f64());
        if let Some(v) = t.get("followers").and_then(|v| v.as_f64()) {
            m.push(CampaignMetric { label: "TikTok followers".into(), value: fmt_num(v), numeric: Some(v), target, source: "insights/tiktok.json".into(), captured_at: at.clone() });
        }
        if let Some(v) = t.get("views_7d").and_then(|v| v.as_f64()) {
            m.push(CampaignMetric { label: "TikTok views (7 d)".into(), value: fmt_num(v), numeric: Some(v), target: None, source: "insights/tiktok.json".into(), captured_at: at.clone() });
        }
    }
    if let Some(i) = read_json(&studio.join("insights").join("instagram.json")) {
        let at = i.get("at").and_then(|v| v.as_str()).map(str::to_string);
        if let Some(v) = i.get("followers").and_then(|v| v.as_f64()) {
            m.push(CampaignMetric { label: "Instagram followers".into(), value: fmt_num(v), numeric: Some(v), target: i.pointer("/targets/gifts").and_then(|v| v.as_f64()), source: "insights/instagram.json".into(), captured_at: at.clone() });
        }
        if let Some(v) = i.get("reel_plays").and_then(|v| v.as_f64()) {
            m.push(CampaignMetric { label: "Instagram reel plays".into(), value: fmt_num(v), numeric: Some(v), target: None, source: "insights/instagram.json".into(), captured_at: at.clone() });
        }
    }
    m
}

fn nova_metrics(influencer: &Path) -> Vec<CampaignMetric> {
    let mut m = Vec::new();
    let data = influencer.join("data");
    if let Some(a) = read_json(&data.join("analytics.json")) {
        let followers = a
            .get("followers")
            .and_then(|v| v.as_f64())
            .or_else(|| a.pointer("/latest/followers").and_then(|v| v.as_f64()));
        let at = a
            .get("at")
            .or_else(|| a.pointer("/latest/at"))
            .and_then(|v| v.as_str())
            .map(str::to_string);
        if let Some(v) = followers {
            m.push(CampaignMetric { label: "Instagram followers".into(), value: fmt_num(v), numeric: Some(v), target: None, source: "influencer/data/analytics.json".into(), captured_at: at });
        }
    }
    if let Some(cal) = read_json(&data.join("calendar.json")) {
        let slots = cal.get("slots").and_then(|s| s.as_array()).cloned().unwrap_or_default();
        let posted = slots.iter().filter(|s| s.get("status").and_then(|v| v.as_str()) == Some("posted")).count();
        m.push(CampaignMetric { label: "Posts published".into(), value: posted.to_string(), numeric: Some(posted as f64), target: Some(slots.len() as f64), source: "influencer/data/calendar.json".into(), captured_at: cal.get("updated_at").and_then(|v| v.as_str()).map(str::to_string) });
    }
    m
}

pub struct Inputs {
    pub briefs: Vec<CampaignBrief>,
    pub brands: Vec<BrandProfile>,
    pub accounts: Vec<AccountRecord>,
    pub cycles: Vec<AutonomyCycleReport>,
    pub current_job: Option<String>,
    pub pending_tasks: Vec<HumanTask>,
    pub pairings: Vec<openaccounts_social_media_manager::bridge::StickAgentPairing>,
    pub bridge_messages: Vec<StickBridgeMessage>,
    pub episodes: Vec<StickEpisodeSummary>,
    pub jobs: Vec<(String, JobRoot)>,
    pub studio_root: std::path::PathBuf,
    pub influencer_root: std::path::PathBuf,
    pub studio_paused: bool,
}

pub async fn gather(state: &AppState) -> Inputs {
    let status = state.autonomy.status().await;
    let mut pending_tasks = state.human_controller.list_pending_tasks().await;
    pending_tasks.extend(crate::influencer::pending_tasks());
    let bridge = state.social_media_manager.get_stick_bridge_state();
    let studio_root = state.autonomy.studio_root().clone();
    Inputs {
        briefs: state.content_strategy.list_campaigns().await,
        brands: state.brand_manager.list_brands().await,
        accounts: state.account_registry.list_accounts().await.unwrap_or_default(),
        cycles: state.autonomy.recent_cycles(60).await,
        current_job: status.current_job,
        pending_tasks,
        pairings: bridge.pairings,
        bridge_messages: bridge.messages,
        episodes: bridge.latest_episodes,
        jobs: crate::autonomy::planned_jobs(),
        studio_paused: studio_root.join("autopilot.pause").is_file(),
        studio_root,
        influencer_root: state.autonomy.influencer_root().clone(),
    }
}

pub fn overview(inputs: &Inputs, brief: &CampaignBrief) -> CampaignOverview {
    let id = brief.campaign_id.as_str();
    let brand = inputs.brands.iter().find(|b| b.brand_id == brief.brand_id).cloned();

    let accounts: Vec<CampaignAccount> = inputs
        .accounts
        .iter()
        .filter(|a| (id == NOVA) == account_is_nova(a))
        .filter(|a| {
            // The CDN campaign's accounts are the social handles plus the store; inbox and payment rows are shared ops.
            id == NOVA || !matches!(a.platform.to_string().as_str(), "email" | "paypal")
        })
        .map(|a| CampaignAccount {
            id: a.id.to_string(),
            platform: a.platform.to_string(),
            username: a.username.clone(),
            status: a.status.clone(),
            last_active: a.last_active.map(|d| d.to_rfc3339()),
        })
        .collect();

    let agents = agents_for(id, &inputs.pairings, &inputs.jobs);

    // The latest cycle that ran any of this campaign's jobs.
    let mut jobs_done = 0u64;
    let mut jobs_failed = 0u64;
    let mut jobs_waiting = 0u64;
    let mut last_activity_at = None;
    let mut last_cycle_status = None;
    let mut issues: Vec<CampaignIssue> = Vec::new();
    for cycle in &inputs.cycles {
        let mine: Vec<_> = cycle.jobs.iter().filter(|j| job_campaign(&j.name) == id).collect();
        if mine.is_empty() {
            continue;
        }
        if last_activity_at.is_none() {
            last_activity_at = Some(cycle.finished_at.clone());
            let failed = mine.iter().filter(|j| !j.success).count();
            last_cycle_status = Some(if failed == 0 { "succeeded" } else if failed == mine.len() { "failed" } else { "partial" }.to_string());
            for j in &mine {
                match job_status(j) {
                    "done" => jobs_done += 1,
                    "failed" | "timed_out" => {
                        jobs_failed += 1;
                        issues.push(CampaignIssue {
                            severity: "error".into(),
                            title: format!("{} {}", j.name, if j.timed_out { "timed out" } else { "failed" }),
                            detail: j.summary.clone(),
                            at: Some(cycle.finished_at.clone()),
                            source: format!("{} {}", j.program.rsplit(['\\', '/']).next().unwrap_or(""), j.args.join(" ")),
                        });
                    }
                    "waiting" => {
                        jobs_waiting += 1;
                        issues.push(CampaignIssue {
                            severity: "blocked".into(),
                            title: format!("{} is waiting", j.name),
                            detail: j.summary.clone(),
                            at: Some(cycle.finished_at.clone()),
                            source: j.args.join(" "),
                        });
                    }
                    _ => {}
                }
            }
            break;
        }
    }

    let pending: Vec<&HumanTask> = inputs
        .pending_tasks
        .iter()
        .filter(|t| {
            let nova = t.automation_id.as_deref().is_some_and(|a| a.starts_with("influencer:")) || t.title.starts_with("Nova");
            (id == NOVA) == nova
        })
        .collect();
    for t in &pending {
        issues.push(CampaignIssue {
            severity: "blocked".into(),
            title: t.title.clone(),
            detail: t.instructions.clone(),
            at: Some(t.created_at.to_rfc3339()),
            source: "Needs You".into(),
        });
    }
    if id == CDN && inputs.studio_paused {
        issues.push(CampaignIssue {
            severity: "warning".into(),
            title: "Studio autopilot is paused".into(),
            detail: std::fs::read_to_string(inputs.studio_root.join("autopilot.pause")).unwrap_or_default().trim().to_string(),
            at: None,
            source: "autopilot.pause".into(),
        });
    }

    // Outstanding work and progress.
    let mut outstanding = Vec::new();
    let mut progress = Vec::new();
    if id == CDN {
        if let Some(c) = read_json(&inputs.studio_root.join("insights").join("comments.json")) {
            let n = c.get("pending").and_then(|p| p.as_object()).map(|o| o.len()).unwrap_or(0);
            if n > 0 {
                outstanding.push(format!("{n} comment repl{} drafted and waiting", if n == 1 { "y" } else { "ies" }));
            }
        }
        for e in &inputs.episodes {
            let mut missing = Vec::new();
            if !e.youtube_published {
                missing.push("YouTube");
            }
            if !e.tiktok_posted {
                missing.push("TikTok");
            }
            if !e.instagram_posted {
                missing.push("Instagram");
            }
            if e.has_long_mp4 && !missing.is_empty() {
                outstanding.push(format!("{}: not yet on {}", e.title, missing.join(", ")));
            }
        }
        let published = inputs.episodes.iter().filter(|e| e.youtube_published).count() as u64;
        progress.push(CampaignProgress { label: "Recent episodes on YouTube".into(), done: published, total: inputs.episodes.len() as u64 });
    } else {
        let data = inputs.influencer_root.join("data");
        if let Some(cal) = read_json(&data.join("calendar.json")) {
            let slots = cal.get("slots").and_then(|s| s.as_array()).cloned().unwrap_or_default();
            let count = |st: &str| slots.iter().filter(|s| s.get("status").and_then(|v| v.as_str()) == Some(st)).count() as u64;
            let posted = count("posted");
            let ready = count("ready");
            let planned = count("planned");
            if planned > 0 {
                outstanding.push(format!("{planned} calendar slot{} still to generate", if planned == 1 { "" } else { "s" }));
            }
            if ready > 0 {
                outstanding.push(format!("{ready} post{} generated and ready to publish", if ready == 1 { "" } else { "s" }));
            }
            progress.push(CampaignProgress { label: "Calendar slots published".into(), done: posted, total: slots.len() as u64 });
            progress.push(CampaignProgress { label: "Calendar slots generated".into(), done: posted + ready, total: slots.len() as u64 });
        }
        if let Some(ap) = read_json(&data.join("approvals.json")) {
            let gates = ["reference_approved", "account_created", "publishing_authorized"];
            let granted = gates.iter().filter(|g| ap.pointer(&format!("/gates/{g}")).is_some()).count() as u64;
            progress.push(CampaignProgress { label: "Launch gates granted".into(), done: granted, total: gates.len() as u64 });
            for g in gates {
                if ap.pointer(&format!("/gates/{g}")).is_none() {
                    outstanding.push(format!("Gate not yet granted: {g}"));
                }
            }
        }
    }
    let planned_now: Vec<String> = inputs.jobs.iter().filter(|(n, _)| job_campaign(n) == id).map(|(n, _)| n.clone()).collect();
    progress.push(CampaignProgress {
        label: "Jobs completed in the last cycle".into(),
        done: jobs_done,
        total: planned_now.len() as u64,
    });

    let metrics = if id == CDN { cdn_metrics(&inputs.studio_root) } else { nova_metrics(&inputs.influencer_root) };

    CampaignOverview {
        brief: brief.clone(),
        brand,
        accounts,
        agents,
        last_activity_at,
        last_cycle_status,
        jobs_done,
        jobs_failed,
        jobs_waiting,
        current_job: inputs.current_job.clone().filter(|j| job_campaign(j) == id),
        issues,
        outstanding,
        metrics,
        progress,
        pending_tasks: pending.len(),
    }
}

pub fn detail(inputs: &Inputs, brief: &CampaignBrief) -> CampaignDetail {
    let id = brief.campaign_id.as_str();
    let overview = overview(inputs, brief);

    let mut activities = Vec::new();
    let mut cycles = Vec::new();
    for cycle in &inputs.cycles {
        let mine: Vec<_> = cycle.jobs.iter().filter(|j| job_campaign(&j.name) == id).collect();
        if mine.is_empty() {
            continue;
        }
        let mut done = 0;
        let mut failed = 0;
        let mut waiting = 0;
        for j in &mine {
            let status = job_status(j);
            match status {
                "done" => done += 1,
                "failed" | "timed_out" => failed += 1,
                "waiting" => waiting += 1,
                _ => {}
            }
            if activities.len() < 120 {
                activities.push(CampaignActivity {
                    cycle_id: cycle.cycle_id.clone(),
                    at: cycle.finished_at.clone(),
                    job: j.name.clone(),
                    status: status.into(),
                    summary: j.summary.clone(),
                    duration_ms: j.duration_ms,
                });
            }
        }
        cycles.push(CycleSummary {
            cycle_id: cycle.cycle_id.clone(),
            started_at: cycle.started_at.clone(),
            finished_at: cycle.finished_at.clone(),
            status: cycle.status.clone(),
            done,
            failed,
            waiting,
        });
    }

    let pending_tasks = inputs
        .pending_tasks
        .iter()
        .filter(|t| {
            let nova = t.automation_id.as_deref().is_some_and(|a| a.starts_with("influencer:")) || t.title.starts_with("Nova");
            (id == NOVA) == nova
        })
        .cloned()
        .collect();

    let bridge_messages = if id == CDN {
        inputs.bridge_messages.iter().take(20).cloned().collect()
    } else {
        inputs
            .bridge_messages
            .iter()
            .filter(|m| m.subject.contains("Nova") || m.body.contains("Nova") || m.to_agent.contains("Nova"))
            .take(20)
            .cloned()
            .collect()
    };

    let calendar = if id == NOVA {
        read_json(&inputs.influencer_root.join("data").join("calendar.json"))
            .and_then(|c| c.get("slots").and_then(|s| s.as_array()).cloned())
            .unwrap_or_default()
            .into_iter()
            .take(40)
            .map(|mut s| {
                // The prompt text is production detail, not campaign status.
                if let Some(o) = s.as_object_mut() {
                    o.remove("pose");
                }
                s
            })
            .collect()
    } else {
        Vec::new()
    };

    CampaignDetail {
        overview,
        activities,
        cycles,
        pending_tasks,
        bridge_messages,
        episodes: if id == CDN { inputs.episodes.clone() } else { Vec::new() },
        calendar,
    }
}
