use chrono::Utc;
use openaccounts_core::{OpenAccountsError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

pub const DEFAULT_STICK_EXPLAINERS_ROOT: &str = r"C:\Users\chrom\Desktop\StickExplainers";
pub const STICK_ROOT_ENV: &str = "OPENACCOUNTS_STICK_ROOT";

pub fn default_stick_root() -> PathBuf {
    std::env::var(STICK_ROOT_ENV)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_STICK_EXPLAINERS_ROOT))
}

pub fn free_disk_gb(path: &Path) -> Option<f64> {
    let disks = sysinfo::Disks::new_with_refreshed_list();
    let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    disks
        .list()
        .iter()
        .filter(|d| canonical.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space() as f64 / 1_073_741_824.0)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickAgentPairing {
    pub openaccounts_agent: String,
    pub openaccounts_title: String,
    pub stick_agents: Vec<String>,
    pub stick_roles: Vec<String>,
    pub channel: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickBridgeMessage {
    pub id: String,
    pub timestamp: String,
    pub direction: String,
    pub from_agent: String,
    pub to_agent: String,
    pub channel: String,
    pub subject: String,
    pub body: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickEpisodeSummary {
    pub slug: String,
    pub title: String,
    pub has_long_mp4: bool,
    pub shorts_count: usize,
    pub youtube_published: bool,
    pub tiktok_posted: bool,
    pub instagram_posted: bool,
    pub kitchen_dish: Option<String>,
    pub kitchen_show: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickDelegationConfig {
    pub comments_delegated_to_openaccounts: bool,
    pub engage_delegated_to_openaccounts: bool,
    pub social_agent_delegated_to_openaccounts: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GarbageCollectionReport {
    pub executed_at: String,
    pub applied: bool,
    pub free_gb: f64,
    pub freed_this_run_gb: f64,
    pub freed_total_gb: f64,
    pub items_cleaned: usize,
    pub logs_rotated: usize,
    pub emulator_status: String,
    pub orphans_status: String,
    pub disk_pressure: bool,
    pub openaccounts_buffers_scrubbed: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StickBridgeState {
    pub bridge_version: String,
    pub status: String,
    pub stick_studio_root: String,
    pub control_room_url: String,
    pub delegation: StickDelegationConfig,
    pub pairings: Vec<StickAgentPairing>,
    pub messages: Vec<StickBridgeMessage>,
    pub latest_episodes: Vec<StickEpisodeSummary>,
    pub last_synced_at: String,
}

pub struct StickExplainersBridge {
    root: PathBuf,
}

impl Default for StickExplainersBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl StickExplainersBridge {
    pub fn new() -> Self {
        Self {
            root: default_stick_root(),
        }
    }

    pub fn with_root(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }

    fn bridge_json_path(&self) -> PathBuf {
        self.root
            .join("insights")
            .join("bridge")
            .join("openaccounts_bridge.json")
    }

    fn vault_interactions_dir(&self) -> PathBuf {
        self.root.join("office_vault").join("Interactions")
    }

    fn directives_json_path(&self) -> PathBuf {
        self.root.join("insights").join("ceo").join("directives.json")
    }

    pub fn load_state(&self) -> StickBridgeState {
        let path = self.bridge_json_path();
        if let Ok(raw) = std::fs::read_to_string(&path) {
            if let Ok(mut parsed) = serde_json::from_str::<StickBridgeState>(&raw) {
                parsed.latest_episodes = self.scan_latest_episodes(5);
                return parsed;
            }
        }

        let now = Utc::now().to_rfc3339();
        StickBridgeState {
            bridge_version: "1.0.0".into(),
            status: if self.root.is_dir() { "CONNECTED".into() } else { "NOT_FOUND".into() },
            stick_studio_root: self.root.to_string_lossy().to_string(),
            control_room_url: "http://127.0.0.1:4840".into(),
            delegation: StickDelegationConfig {
                comments_delegated_to_openaccounts: true,
                engage_delegated_to_openaccounts: true,
                social_agent_delegated_to_openaccounts: true,
            },
            pairings: Self::default_pairings(),
            messages: vec![],
            latest_episodes: self.scan_latest_episodes(5),
            last_synced_at: now,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn send_message(
        &self,
        from_agent: &str,
        to_agent: &str,
        channel: &str,
        subject: &str,
        body: &str,
        direction: &str,
        create_ceo_directive: bool,
    ) -> Result<StickBridgeMessage> {
        if !self.root.is_dir() {
            return Err(OpenAccountsError::Internal(format!(
                "StickExplainers studio not found at {} (set {})",
                self.root.display(),
                STICK_ROOT_ENV
            )));
        }
        let mut state = self.load_state();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        let msg_id = format!("br-msg-{:03}", state.messages.len() + 1);

        let msg = StickBridgeMessage {
            id: msg_id,
            timestamp: now_str.clone(),
            direction: direction.to_string(),
            from_agent: from_agent.to_string(),
            to_agent: to_agent.to_string(),
            channel: channel.to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
            status: "written".into(),
        };

        state.messages.insert(0, msg.clone());
        state.messages.truncate(50);
        state.last_synced_at = now_str.clone();
        state.latest_episodes = self.scan_latest_episodes(5);

        let bridge_path = self.bridge_json_path();
        if let Some(parent) = bridge_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let serialized = serde_json::to_string_pretty(&state)
            .map_err(|e| OpenAccountsError::Internal(e.to_string()))?;
        std::fs::write(&bridge_path, serialized)
            .map_err(|e| OpenAccountsError::Internal(e.to_string()))?;

        let interactions_dir = self.vault_interactions_dir();
        if interactions_dir.exists() {
            let slug = format!(
                "Bridge_{}_{}_{}.md",
                now.format("%Y-%m-%d_%H%M%S"),
                from_agent.replace(' ', "_").replace('·', ""),
                to_agent.replace(' ', "_").replace('&', "and")
            );
            let md = format!(
                "---\ndate: \"{}\"\ntime: \"{}\"\ncategory: \"OpenAccounts Bridge · {}\"\nparticipants:\n  - \"{}\"\n  - \"{}\"\ntags:\n  - interaction/bridge\n  - openaccounts/sync\n---\n\n# 🔗 Bridge Sync: {} → {}\n\n**Channel:** `{}`  \n**Subject:** {}  \n**Logged At:** `{}`\n\n---\n\n## 📜 Coordination Transcript\n\n> **{}:** \"[{}] {}\"\n",
                now.format("%Y-%m-%d"),
                now.format("%H:%M:%S UTC"),
                channel,
                from_agent,
                to_agent,
                from_agent,
                to_agent,
                channel,
                subject,
                now_str,
                from_agent,
                subject,
                body
            );
            let _ = std::fs::write(interactions_dir.join(slug), md);
        }

        if create_ceo_directive {
            let dir_path = self.directives_json_path();
            if let Ok(raw) = std::fs::read_to_string(&dir_path) {
                if let Ok(mut list) = serde_json::from_str::<Vec<serde_json::Value>>(&raw) {
                    let next_id = format!("dir-{:03}", list.len() + 1);
                    list.push(serde_json::json!({
                        "id": next_id,
                        "dept": "Growth & Community",
                        "priority": "high",
                        "text": format!("[OpenAccounts Bridge] {}: {}", subject, body),
                        "issued_at": now_str,
                        "status": "active"
                    }));
                    if let Ok(out_str) = serde_json::to_string_pretty(&list) {
                        let _ = std::fs::write(&dir_path, out_str);
                    }
                }
            }
        }

        info!(
            "Dispatched coordination message {} from {} to {}",
            msg.id, from_agent, to_agent
        );
        Ok(msg)
    }

    pub fn scan_latest_episodes(&self, limit: usize) -> Vec<StickEpisodeSummary> {
        let out_dir = self.root.join("out");
        let mut dirs: Vec<PathBuf> = std::fs::read_dir(&out_dir)
            .ok()
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok().map(|d| d.path()))
            .filter(|p| {
                p.is_dir()
                    && p.file_name()
                        .and_then(|n| n.to_str())
                        .map(|s| s.starts_with("news-"))
                        .unwrap_or(false)
            })
            .collect();

        dirs.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
        dirs.truncate(limit);

        let mut summaries = Vec::new();
        for d in dirs {
            let slug = d
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
                .to_string();
            let script_val: serde_json::Value = std::fs::read_to_string(d.join("script.json"))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            let seo_val: serde_json::Value = std::fs::read_to_string(d.join("seo.json"))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();
            let pub_val: serde_json::Value = std::fs::read_to_string(d.join("publish.json"))
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();

            let title = seo_val
                .get("title")
                .and_then(|v| v.as_str())
                .or_else(|| script_val.get("title").and_then(|v| v.as_str()))
                .unwrap_or(&slug)
                .to_string();

            let shorts_count = script_val
                .get("shorts")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0);

            let mut kitchen_dish = None;
            let mut kitchen_show = None;
            if let Some(chapters) = script_val.get("chapters").and_then(|v| v.as_array()) {
                for ch in chapters {
                    if ch.get("kitchen").and_then(|v| v.as_bool()).unwrap_or(false) {
                        if let Some(rec) = ch.get("recipe") {
                            kitchen_dish = rec.get("dish").and_then(|v| v.as_str()).map(String::from);
                            kitchen_show = rec.get("show").and_then(|v| v.as_str()).map(String::from);
                        }
                    }
                }
            }

            summaries.push(StickEpisodeSummary {
                slug,
                title,
                has_long_mp4: d.join("long.mp4").exists(),
                shorts_count,
                youtube_published: pub_val.get("long").is_some(),
                tiktok_posted: d.join("tiktok.json").exists(),
                instagram_posted: d.join("instagram.json").exists(),
                kitchen_dish,
                kitchen_show,
            });
        }

        summaries
    }

    pub fn default_pairings() -> Vec<StickAgentPairing> {
        vec![
            StickAgentPairing {
                openaccounts_agent: "ContentStrategist".into(),
                openaccounts_title: "Head of Marketing Strategy".into(),
                stick_agents: vec!["V. Sterling".into(), "J. Jade".into(), "S. Saffron".into()],
                stick_roles: vec![
                    "CEO Boss Agent".into(),
                    "News Scout".into(),
                    "Cartoon Kitchen Chef".into(),
                ],
                channel: "Editorial & Campaign Strategy".into(),
            },
            StickAgentPairing {
                openaccounts_agent: "TrendResearcher".into(),
                openaccounts_title: "Live Trend Intelligence Agent".into(),
                stick_agents: vec!["C. Copper".into(), "A. Amber".into(), "I. Indigo".into()],
                stick_roles: vec![
                    "Competitor Scout".into(),
                    "Competitor Analyst".into(),
                    "Performance Analyst".into(),
                ],
                channel: "Trend & Retention Intelligence".into(),
            },
            StickAgentPairing {
                openaccounts_agent: "GraphicDesigner".into(),
                openaccounts_title: "Vector & Photo Production Specialist".into(),
                stick_agents: vec!["T. Tangerine".into(), "D. Sienna".into()],
                stick_roles: vec!["Thumbnail Director".into(), "Dish Artist".into()],
                channel: "Thumbnails & Dish Art Assets".into(),
            },
            StickAgentPairing {
                openaccounts_agent: "VideoDirector".into(),
                openaccounts_title: "Short-Form Video & Reel Director".into(),
                stick_agents: vec!["C. Cyan".into(), "R. Ruby".into(), "F. Fuchsia".into()],
                stick_roles: vec![
                    "Animator & Renderer".into(),
                    "YouTube Publisher".into(),
                    "TikTok & Reels Poster".into(),
                ],
                channel: "Episode & 9:16 Shorts Delivery".into(),
            },
            StickAgentPairing {
                openaccounts_agent: "CommunityManager".into(),
                openaccounts_title: "Community Moderation & Engagement".into(),
                stick_agents: vec![
                    "C. Coral".into(),
                    "E. Emerald".into(),
                    "P. Plum".into(),
                    "M. Maroon".into(),
                ],
                stick_roles: vec![
                    "Social Media Agent".into(),
                    "Engagement Agent".into(),
                    "Community Manager".into(),
                    "Merch Agent".into(),
                ],
                channel: "Delegated Social, Comments & Merch Replies".into(),
            },
            StickAgentPairing {
                openaccounts_agent: "ComplianceAgent".into(),
                openaccounts_title: "Policy, Quota & Garbage Collection Gatekeeper".into(),
                stick_agents: vec![
                    "Q. Quartz".into(),
                    "G. Gold".into(),
                    "E. Ebony".into(),
                    "G. Graphite".into(),
                ],
                stick_roles: vec![
                    "Quality Gate".into(),
                    "CFO".into(),
                    "Email Watchdog".into(),
                    "Garbage Collector".into(),
                ],
                channel: "24h Buffer, Quota, GC & Sponsor Triage".into(),
            },
        ]
    }

    pub fn run_garbage_collection(&self, apply: bool) -> Result<GarbageCollectionReport> {
        if !self.root.is_dir() {
            return Err(OpenAccountsError::Internal(format!(
                "StickExplainers studio not found at {} (set {})",
                self.root.display(),
                STICK_ROOT_ENV
            )));
        }

        let script = self.root.join("retention.py");
        let mut script_error: Option<String> = None;
        if script.is_file() {
            let mut ran = false;
            for interpreter in ["python3", "python", "py"] {
                let mut cmd = std::process::Command::new(interpreter);
                cmd.arg(&script).current_dir(&self.root);
                if apply {
                    cmd.arg("--apply");
                }
                match cmd.output() {
                    Ok(out) if out.status.success() => {
                        ran = true;
                        break;
                    }
                    Ok(out) => {
                        script_error = Some(format!(
                            "retention.py exited with {}: {}",
                            out.status,
                            String::from_utf8_lossy(&out.stderr).chars().take(300).collect::<String>()
                        ));
                        ran = true;
                        break;
                    }
                    Err(_) => continue,
                }
            }
            if !ran {
                script_error = Some("No Python interpreter found to run retention.py".into());
            }
        } else {
            script_error = Some(format!("{} not found", script.display()));
        }

        let mut oa_scrubbed = 0usize;
        if apply {
            if let Ok(entries) = std::fs::read_dir(self.root.join("_logs")) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.extension().and_then(|e| e.to_str()) == Some("tmp") && std::fs::remove_file(&p).is_ok() {
                        oa_scrubbed += 1;
                    }
                }
            }
        }

        let gc_path = self.root.join("insights").join("gc.json");
        let gc_val: serde_json::Value = std::fs::read_to_string(&gc_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        let free_gb = gc_val
            .get("free_gb")
            .and_then(|v| v.as_f64())
            .or_else(|| free_disk_gb(&self.root))
            .unwrap_or(0.0);

        let report = GarbageCollectionReport {
            executed_at: Utc::now().to_rfc3339(),
            applied: apply && script_error.is_none(),
            free_gb: (free_gb * 10.0).round() / 10.0,
            freed_this_run_gb: gc_val.get("freed_gb").and_then(|v| v.as_f64()).unwrap_or(0.0),
            freed_total_gb: gc_val.get("freed_total_gb").and_then(|v| v.as_f64()).unwrap_or(0.0),
            items_cleaned: gc_val.get("items").and_then(|v| v.as_u64()).unwrap_or(0) as usize + oa_scrubbed,
            logs_rotated: gc_val.get("logs_rotated").and_then(|v| v.as_u64()).unwrap_or(0) as usize,
            emulator_status: gc_val
                .get("emulator")
                .and_then(|v| v.as_str())
                .map(str::to_string)
                .unwrap_or_else(|| "not reported".into()),
            orphans_status: match &script_error {
                Some(e) => e.clone(),
                None => gc_val
                    .get("orphans")
                    .and_then(|v| v.as_str())
                    .map(str::to_string)
                    .unwrap_or_else(|| "not reported".into()),
            },
            disk_pressure: gc_val
                .get("pressure")
                .and_then(|v| v.as_bool())
                .unwrap_or(free_gb > 0.0 && free_gb < 10.0),
            openaccounts_buffers_scrubbed: oa_scrubbed,
        };

        let _ = self.send_message(
            "OpenAccounts · GarbageCollector",
            "G. Graphite (Garbage Collector) & H. Hazel (Health Doctor)",
            "24h Buffer, Quota, GC & Sponsor Triage",
            "Autonomous Garbage Collection Sweep Completed",
            &format!(
                "Free disk: {:.1} GB | Reclaimed total: {:.2} GB | Orphans: {} | Emulator: {}",
                report.free_gb, report.freed_total_gb, report.orphans_status, report.emulator_status
            ),
            "openaccounts_to_stick",
            false,
        );

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stick_explainers_bridge_sync_and_send() {
        let root = std::env::temp_dir().join(format!("oa-stick-{}", uuid::Uuid::new_v4()));
        let missing = StickExplainersBridge::with_root(&root);
        assert_eq!(missing.load_state().status, "NOT_FOUND");
        assert!(missing
            .send_message("a", "b", "c", "d", "e", "openaccounts_to_stick", false)
            .is_err());
        assert!(missing.run_garbage_collection(false).is_err());

        std::fs::create_dir_all(&root).unwrap();
        let bridge = StickExplainersBridge::with_root(&root);
        let state = bridge.load_state();
        assert_eq!(state.status, "CONNECTED");
        assert!(!state.pairings.is_empty());

        let msg = bridge
            .send_message(
                "OpenAccounts · CommunityManager",
                "C. Coral & E. Emerald",
                "Delegated Social, Comments & Merch Replies",
                "Bridge Test Sync",
                "Verified bidirectional coordination between OpenAccounts and StickExplainers.",
                "openaccounts_to_stick",
                false,
            )
            .expect("should write bridge message");
        assert_eq!(msg.status, "written");
        assert_eq!(bridge.load_state().messages.len(), 1);

        let report = bridge.run_garbage_collection(false).unwrap();
        assert!(!report.applied);
        assert!(report.orphans_status.contains("retention.py"));
        std::fs::remove_dir_all(&root).ok();
    }
}
