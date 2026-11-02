use chrono::Utc;
use openaccounts_core::{OpenAccountsError, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use tracing::info;

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

impl StickExplainersBridge {
    pub fn new() -> Self {
        Self {
            root: openaccounts_core::studio_root(),
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

    /// The bridge as it actually is: the message log and pairings persisted in
    /// `insights/bridge/openaccounts_bridge.json`, the delegation flags from the
    /// studio's `config.json`, and the episodes on disk. Nothing is assumed.
    pub fn load_state(&self) -> StickBridgeState {
        let path = self.bridge_json_path();
        let persisted = std::fs::read_to_string(&path)
            .ok()
            .and_then(|raw| serde_json::from_str::<StickBridgeState>(&raw).ok());

        let status = if !self.root.is_dir() {
            "STUDIO_NOT_FOUND"
        } else if persisted.is_some() {
            "CONNECTED"
        } else {
            "NO_BRIDGE_FILE"
        };

        let (bridge_version, pairings, messages, last_synced_at) = match persisted {
            Some(p) => (p.bridge_version, p.pairings, p.messages, p.last_synced_at),
            None => (String::new(), Self::default_pairings(), Vec::new(), String::new()),
        };

        StickBridgeState {
            bridge_version,
            status: status.into(),
            stick_studio_root: self.root.to_string_lossy().to_string(),
            // The studio control room's port (CDN_CR_PORT, default 4840 in control_room.py).
            control_room_url: format!(
                "http://127.0.0.1:{}",
                std::env::var("CDN_CR_PORT").unwrap_or_else(|_| "4840".into())
            ),
            delegation: self.read_delegation(),
            pairings,
            messages,
            latest_episodes: self.scan_latest_episodes(5),
            last_synced_at,
        }
    }

    /// Which studio jobs `config.json` hands to OpenAccounts
    /// (`comments` / `engage` / `social` with `delegated_to: "OpenAccounts"`).
    fn read_delegation(&self) -> StickDelegationConfig {
        let cfg: serde_json::Value = std::fs::read_to_string(self.root.join("config.json"))
            .ok()
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default();
        let delegated = |key: &str| {
            cfg.pointer(&format!("/{key}/delegated_to"))
                .and_then(|v| v.as_str())
                .map(|v| v.eq_ignore_ascii_case("openaccounts"))
                .unwrap_or(false)
        };
        StickDelegationConfig {
            comments_delegated_to_openaccounts: delegated("comments"),
            engage_delegated_to_openaccounts: delegated("engage"),
            social_agent_delegated_to_openaccounts: delegated("social"),
        }
    }

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
        let mut state = self.load_state();
        let now = Utc::now();
        let now_str = now.to_rfc3339();
        // Unique even after the log is truncated to 50 entries.
        let msg_id = format!("br-msg-{}", uuid::Uuid::new_v4().simple());

        let msg = StickBridgeMessage {
            id: msg_id,
            timestamp: now_str.clone(),
            direction: direction.to_string(),
            from_agent: from_agent.to_string(),
            to_agent: to_agent.to_string(),
            channel: channel.to_string(),
            subject: subject.to_string(),
            body: body.to_string(),
            status: "delivered".into(),
        };

        state.messages.insert(0, msg.clone());
        state.messages.truncate(50);
        state.last_synced_at = now_str.clone();
        if state.bridge_version.is_empty() {
            state.bridge_version = "1.0.0".into();
        }

        let bridge_path = self.bridge_json_path();
        if let Some(parent) = bridge_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let serialized = serde_json::to_string_pretty(&state)
            .map_err(|e| OpenAccountsError::Internal(e.to_string()))?;
        write_atomic(&bridge_path, serialized.as_bytes())
            .map_err(|e| OpenAccountsError::Internal(e.to_string()))?;

        // Also write interaction transcript to StickExplainers Obsidian Vault
        let interactions_dir = self.vault_interactions_dir();
        if interactions_dir.exists() {
            let slug = format!(
                "Bridge_{}_{}_{}.md",
                now.format("%Y-%m-%d_%H%M%S"),
                from_agent.replace(' ', "_").replace('·', ""),
                to_agent.replace(' ', "_").replace('&', "and")
            );
            let md = format!(
                "---\ndate: \"{}\"\ntime: \"{}\"\ncategory: \"OpenAccounts Bridge · {}\"\nparticipants:\n  - \"{}\"\n  - \"{}\"\ntags:\n  - interaction/bridge\n  - openaccounts/sync\n---\n\n# Bridge message: {} → {}\n\n**Channel:** `{}`  \n**Subject:** {}  \n**Logged At:** `{}`\n\n---\n\n> **{}:** \"[{}] {}\"\n",
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
                body,
            );
            let _ = std::fs::write(interactions_dir.join(slug), md);
        }

        // Optionally append to StickExplainers CEO directives.json
        if create_ceo_directive {
            self.append_ceo_directive(subject, body, &now_str).map_err(|e| {
                OpenAccountsError::Internal(format!(
                    "Bridge message {} was written, but the CEO directive was not: {e}",
                    msg.id
                ))
            })?;
        }

        info!(
            "Dispatched coordination message {} from {} to {}",
            msg.id, from_agent, to_agent
        );
        Ok(msg)
    }

    fn append_ceo_directive(&self, subject: &str, body: &str, issued_at: &str) -> Result<()> {
        let dir_path = self.directives_json_path();
        let raw = std::fs::read_to_string(&dir_path)
            .map_err(|e| OpenAccountsError::Internal(format!("{}: {e}", dir_path.display())))?;
        let mut list: Vec<serde_json::Value> = serde_json::from_str(&raw)?;
        // Next number after the highest existing `dir-NNN`, so ids never repeat.
        let next = list
            .iter()
            .filter_map(|d| d.get("id").and_then(|v| v.as_str()))
            .filter_map(|id| id.strip_prefix("dir-").and_then(|n| n.parse::<u32>().ok()))
            .max()
            .unwrap_or(0)
            + 1;
        list.push(serde_json::json!({
            "id": format!("dir-{next:03}"),
            "dept": "Growth & Community",
            "priority": "high",
            "text": format!("[OpenAccounts Bridge] {}: {}", subject, body),
            "issued_at": issued_at,
            "status": "active"
        }));
        write_atomic(&dir_path, serde_json::to_string_pretty(&list)?.as_bytes())
            .map_err(|e| OpenAccountsError::Internal(format!("{}: {e}", dir_path.display())))
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

    /// Runs the studio's `retention.py` and reports what it recorded in
    /// `insights/gc.json`. A failed run is an error, not a report.
    pub fn run_garbage_collection(&self, apply: bool) -> Result<GarbageCollectionReport> {
        let mut cmd = std::process::Command::new(openaccounts_core::python_interpreter());
        cmd.arg("retention.py");
        if apply {
            cmd.arg("--apply");
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
        }
        let output = cmd
            .current_dir(&self.root)
            .stdin(std::process::Stdio::null())
            .output()
            .map_err(|e| OpenAccountsError::Internal(format!("Cannot launch retention.py: {e}")))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let last = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("no output");
            return Err(OpenAccountsError::Internal(format!(
                "retention.py exited with {:?}: {last}",
                output.status.code()
            )));
        }

        // Clean any .tmp files left in the studio's _logs folder.
        let mut oa_scrubbed = 0usize;
        if let Ok(entries) = std::fs::read_dir(self.root.join("_logs")) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.extension().and_then(|e| e.to_str()) == Some("tmp")
                    && std::fs::remove_file(&p).is_ok()
                {
                    oa_scrubbed += 1;
                }
            }
        }

        let mut report = self.read_gc_report().ok_or_else(|| {
            OpenAccountsError::Internal("retention.py ran but wrote no insights/gc.json".into())
        })?;
        report.items_cleaned += oa_scrubbed;
        report.openaccounts_buffers_scrubbed = oa_scrubbed;

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

    /// The last garbage-collection result the studio recorded, without running
    /// anything. `None` when no run has been recorded yet.
    pub fn read_gc_report(&self) -> Option<GarbageCollectionReport> {
        let raw = std::fs::read_to_string(self.root.join("insights").join("gc.json")).ok()?;
        let gc: serde_json::Value = serde_json::from_str(&raw).ok()?;
        let f = |k: &str| gc.get(k).and_then(|v| v.as_f64()).unwrap_or(0.0);
        let n = |k: &str| gc.get(k).and_then(|v| v.as_u64()).unwrap_or(0) as usize;
        let s = |k: &str| gc.get(k).and_then(|v| v.as_str()).unwrap_or("unknown").to_string();
        Some(GarbageCollectionReport {
            executed_at: gc.get("at").and_then(|v| v.as_str())?.to_string(),
            applied: gc.get("applied").and_then(|v| v.as_bool()).unwrap_or(false),
            free_gb: f("free_gb"),
            freed_this_run_gb: f("freed_gb"),
            freed_total_gb: f("freed_total_gb"),
            items_cleaned: n("items"),
            logs_rotated: n("logs_rotated"),
            emulator_status: s("emulator"),
            orphans_status: s("orphans"),
            disk_pressure: gc.get("pressure").and_then(|v| v.as_bool()).unwrap_or(false),
            openaccounts_buffers_scrubbed: 0,
        })
    }
}

impl Default for StickExplainersBridge {
    fn default() -> Self {
        Self::new()
    }
}

/// Writes to a sibling temp file and renames it into place. The studio's own
/// Python reads and rewrites these JSON files; a rename is atomic on the same
/// volume, so neither side ever sees a partially written file.
fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension(format!("tmp-{}", std::process::id()));
    std::fs::write(&tmp, bytes)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root() -> PathBuf {
        let root = std::env::temp_dir().join(format!("oa-bridge-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("office_vault").join("Interactions")).unwrap();
        std::fs::write(
            root.join("config.json"),
            r#"{"comments":{"delegated_to":"OpenAccounts"},"engage":{"delegated_to":"studio"}}"#,
        )
        .unwrap();
        root
    }

    /// Runs against a temporary studio root, never the real one.
    #[test]
    fn test_bridge_reports_real_state_and_unique_ids() {
        let root = temp_root();
        let bridge = StickExplainersBridge::with_root(&root);

        let state = bridge.load_state();
        assert_eq!(state.status, "NO_BRIDGE_FILE");
        assert!(state.messages.is_empty());
        assert!(state.delegation.comments_delegated_to_openaccounts);
        assert!(!state.delegation.engage_delegated_to_openaccounts);
        assert!(!state.delegation.social_agent_delegated_to_openaccounts);

        let first = bridge
            .send_message("A", "B", "chan", "Subject", "Body", "openaccounts_to_stick", false)
            .expect("should write bridge message");
        let second = bridge
            .send_message("A", "B", "chan", "Subject 2", "Body 2", "openaccounts_to_stick", false)
            .expect("should write bridge message");
        assert_ne!(first.id, second.id);

        let state = bridge.load_state();
        assert_eq!(state.status, "CONNECTED");
        assert_eq!(state.messages.len(), 2);

        // The vault transcript records only what was sent.
        let transcripts: Vec<_> = std::fs::read_dir(root.join("office_vault").join("Interactions"))
            .unwrap()
            .flatten()
            .collect();
        assert!(!transcripts.is_empty());
        for t in transcripts {
            let text = std::fs::read_to_string(t.path()).unwrap();
            assert!(!text.contains("Acknowledged"));
        }

        // A requested directive that cannot be written is an error.
        let res = bridge.send_message("A", "B", "chan", "S", "B", "openaccounts_to_stick", true);
        assert!(res.is_err());

        let _ = std::fs::remove_dir_all(&root);
    }
}
