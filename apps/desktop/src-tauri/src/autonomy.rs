//! Real 24/7 autonomy runtime.
//!
//! OpenAccounts does not invent agent activity. Every cycle executes the real
//! Clearly Drawn Studios jobs in `StickExplainers` through a supervised
//! subprocess runner, records the true exit codes and output, and persists each
//! cycle to SQLite. A cycle that fails is reported as failed.

use chrono::{DateTime, Utc};
use openaccounts_automation_engine::AutomationEngine;
use openaccounts_database::Database;
use serde::{Deserialize, Serialize};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, OnceLock,
    },
    time::{Duration, Instant},
};
use tokio::{
    process::Command,
    sync::RwLock,
};
use tracing::{error, info, warn};
use uuid::Uuid;

pub const DEFAULT_INTERVAL_SECS: u64 = 1800;
const MAX_SUMMARY_CHARS: usize = 700;
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// How often the studio lets a job run. The studio's autopilot gates its
/// steps on the age of a file; the app checks the same file so the two never
/// run a job more often than the studio intends.
struct Cadence {
    /// File whose modification time marks the last run, relative to the studio root.
    marker: &'static str,
    min_age_secs: u64,
    /// True when the marker is a stamp the runner writes (the job itself does not).
    touch: bool,
}

/// Which project directory a job runs in.
#[derive(Clone, Copy, PartialEq, Eq)]
enum JobRoot {
    /// StickExplainers: the Clearly Drawn Studios pipeline.
    Studio,
    /// `influencer/`: the Nova Vexley pipeline.
    Influencer,
}

struct JobSpec {
    name: &'static str,
    root: JobRoot,
    script: &'static str,
    args: &'static [&'static str],
    timeout_secs: u64,
    cadence: Option<Cadence>,
}

/// The jobs the studio's own autopilot dispatches, with its cadences and timeouts
/// (see `comments_step`, `social_step` and `gc_step` in StickExplainers/autopilot.py).
const JOBS: &[JobSpec] = &[
    JobSpec {
        name: "Comment sweep",
        root: JobRoot::Studio,
        script: "comments.py",
        args: &["check"],
        timeout_secs: 1200,
        cadence: Some(Cadence { marker: r"insights\comments.json", min_age_secs: 3600, touch: false }),
    },
    JobSpec {
        name: "Social plan",
        root: JobRoot::Studio,
        script: "social_agent.py",
        args: &["plan"],
        timeout_secs: 120,
        cadence: None,
    },
    JobSpec {
        name: "Social publish",
        root: JobRoot::Studio,
        script: "social_agent.py",
        args: &["next"],
        timeout_secs: 1800,
        cadence: Some(Cadence { marker: r"_logs\social_step.stamp", min_age_secs: 50 * 60, touch: true }),
    },
    JobSpec {
        name: "Engagement run",
        root: JobRoot::Studio,
        script: "engage.py",
        args: &["run"],
        timeout_secs: 1800,
        cadence: Some(Cadence { marker: r"_logs\engage_step.stamp", min_age_secs: 4 * 3600, touch: true }),
    },
    JobSpec {
        name: "Engagement stats",
        root: JobRoot::Studio,
        script: "engage.py",
        args: &["stats"],
        timeout_secs: 120,
        cadence: None,
    },
    JobSpec {
        name: "Retention GC",
        root: JobRoot::Studio,
        script: "retention.py",
        args: &["--apply"],
        timeout_secs: 1800,
        cadence: Some(Cadence { marker: r"insights\gc.json", min_age_secs: 20 * 3600, touch: false }),
    },
    // Nova Vexley (influencer/pipeline.py). Each command exits at once when its
    // human gate is still open, so the cadences only bound the paid work.
    JobSpec {
        name: "Nova plan",
        root: JobRoot::Influencer,
        script: "pipeline.py",
        args: &["plan"],
        timeout_secs: 120,
        cadence: None,
    },
    JobSpec {
        name: "Nova handles",
        root: JobRoot::Influencer,
        script: "pipeline.py",
        args: &["handles"],
        timeout_secs: 600,
        cadence: Some(Cadence { marker: r"data\logs\handles.stamp", min_age_secs: 6 * 3600, touch: true }),
    },
    JobSpec {
        name: "Nova reference",
        root: JobRoot::Influencer,
        script: "pipeline.py",
        args: &["reference"],
        timeout_secs: 1800,
        cadence: Some(Cadence { marker: r"data\logs\reference.stamp", min_age_secs: 3600, touch: true }),
    },
    JobSpec {
        name: "Nova generate",
        root: JobRoot::Influencer,
        script: "pipeline.py",
        args: &["generate"],
        timeout_secs: 3600,
        cadence: Some(Cadence { marker: r"data\logs\generate.stamp", min_age_secs: 6 * 3600, touch: true }),
    },
    JobSpec {
        name: "Nova reels",
        root: JobRoot::Influencer,
        script: "pipeline.py",
        args: &["reels"],
        timeout_secs: 1800,
        cadence: Some(Cadence { marker: r"data\logs\reels.stamp", min_age_secs: 12 * 3600, touch: true }),
    },
    JobSpec {
        name: "Nova publish",
        root: JobRoot::Influencer,
        script: "pipeline.py",
        args: &["publish"],
        timeout_secs: 1800,
        cadence: Some(Cadence { marker: r"data\logs\publish.stamp", min_age_secs: 50 * 60, touch: true }),
    },
    JobSpec {
        name: "Nova analytics",
        root: JobRoot::Influencer,
        script: "pipeline.py",
        args: &["analytics"],
        timeout_secs: 600,
        cadence: Some(Cadence { marker: r"data\logs\analytics.stamp", min_age_secs: 6 * 3600, touch: true }),
    },
    JobSpec {
        name: "Nova GC",
        root: JobRoot::Influencer,
        script: "pipeline.py",
        args: &["gc"],
        timeout_secs: 300,
        cadence: Some(Cadence { marker: r"data\logs\gc.stamp", min_age_secs: 24 * 3600, touch: true }),
    },
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyJobReport {
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    pub exit_code: Option<i32>,
    pub success: bool,
    pub timed_out: bool,
    /// Not run because the studio ran it recently (its cadence has not elapsed).
    #[serde(default)]
    pub skipped: bool,
    pub duration_ms: u64,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyCycleReport {
    pub cycle_id: String,
    pub started_at: String,
    pub finished_at: String,
    /// "succeeded" | "partial" | "failed" | "skipped_kill_switch" | "skipped_busy"
    pub status: String,
    pub duration_ms: u64,
    pub jobs: Vec<AutonomyJobReport>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutonomyStatus {
    pub loop_running: bool,
    pub paused: bool,
    pub interval_secs: u64,
    pub cycles_completed: u64,
    pub cycles_failed: u64,
    pub last_cycle: Option<AutonomyCycleReport>,
    pub next_cycle_at: Option<String>,
    pub last_error: Option<String>,
    pub studio_root: String,
    pub python: String,
    pub studio_root_exists: bool,
    pub influencer_root: String,
    pub influencer_root_exists: bool,
    /// True while a cycle is executing.
    pub cycle_in_flight: bool,
    /// The job the loop is executing right now, if any.
    pub current_job: Option<String>,
    /// Every job a cycle runs, in order.
    pub planned_jobs: Vec<String>,
}

/// Resets the in-flight flag even if a cycle returns early.
struct InFlightGuard<'a>(&'a AtomicBool);
impl Drop for InFlightGuard<'_> {
    fn drop(&mut self) {
        self.0.store(false, Ordering::SeqCst);
    }
}

pub struct AutonomyRuntime {
    db: Arc<Database>,
    automation_engine: Arc<AutomationEngine>,
    studio_root: PathBuf,
    influencer_root: PathBuf,
    python: String,
    interval: Duration,
    running: AtomicBool,
    paused: AtomicBool,
    cycle_in_flight: AtomicBool,
    cycles_completed: AtomicU64,
    cycles_failed: AtomicU64,
    last_cycle: RwLock<Option<AutonomyCycleReport>>,
    last_error: RwLock<Option<String>>,
    next_cycle_at: RwLock<Option<DateTime<Utc>>>,
    current_job: RwLock<Option<String>>,
    app_handle: OnceLock<tauri::AppHandle>,
}

impl AutonomyRuntime {
    pub fn new(db: Arc<Database>, automation_engine: Arc<AutomationEngine>) -> Self {
        let studio_root = openaccounts_core::studio_root();

        let interval_secs = std::env::var("OPENACCOUNTS_CYCLE_SECS")
            .ok()
            .and_then(|v| v.parse::<u64>().ok())
            .filter(|v| *v >= 5)
            .unwrap_or(DEFAULT_INTERVAL_SECS);

        Self {
            db,
            automation_engine,
            studio_root,
            influencer_root: openaccounts_core::influencer_root(),
            python: openaccounts_core::python_interpreter(),
            interval: Duration::from_secs(interval_secs),
            running: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            cycle_in_flight: AtomicBool::new(false),
            cycles_completed: AtomicU64::new(0),
            cycles_failed: AtomicU64::new(0),
            last_cycle: RwLock::new(None),
            last_error: RwLock::new(None),
            next_cycle_at: RwLock::new(None),
            current_job: RwLock::new(None),
            app_handle: OnceLock::new(),
        }
    }

    pub fn studio_root(&self) -> &PathBuf {
        &self.studio_root
    }

    pub fn is_paused(&self) -> bool {
        self.paused.load(Ordering::SeqCst)
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    pub async fn status(&self) -> AutonomyStatus {
        AutonomyStatus {
            loop_running: self.is_running(),
            paused: self.is_paused(),
            interval_secs: self.interval.as_secs(),
            cycles_completed: self.cycles_completed.load(Ordering::SeqCst),
            cycles_failed: self.cycles_failed.load(Ordering::SeqCst),
            last_cycle: self.last_cycle.read().await.clone(),
            next_cycle_at: self.next_cycle_at.read().await.map(|d| d.to_rfc3339()),
            last_error: self.last_error.read().await.clone(),
            studio_root: self.studio_root.to_string_lossy().to_string(),
            python: self.python.clone(),
            studio_root_exists: self.studio_root.is_dir(),
            influencer_root: self.influencer_root.to_string_lossy().to_string(),
            influencer_root_exists: self.influencer_root.join("pipeline.py").is_file(),
            cycle_in_flight: self.cycle_in_flight.load(Ordering::SeqCst),
            current_job: self.current_job.read().await.clone(),
            planned_jobs: JOBS.iter().map(|j| j.name.to_string()).collect(),
        }
    }

    /// Starts the supervised loop. Safe to call multiple times; only the first
    /// call spawns the task.
    pub fn start(self: &Arc<Self>, app: Option<tauri::AppHandle>) {
        if let Some(handle) = app {
            let _ = self.app_handle.set(handle);
        }
        if self.running.swap(true, Ordering::SeqCst) {
            return;
        }
        let this = self.clone();
        tauri::async_runtime::spawn(async move {
            this.loop_forever().await;
        });
        info!(
            "Autonomy runtime started (interval {}s, studio root {}, python {})",
            self.interval.as_secs(),
            self.studio_root.display(),
            self.python
        );
    }

    pub fn pause(&self) {
        self.paused.store(true, Ordering::SeqCst);
        warn!("Autonomy runtime paused by operator");
    }

    /// Clears a pause and makes sure the loop task exists.
    pub fn resume(self: &Arc<Self>) {
        self.paused.store(false, Ordering::SeqCst);
        if !self.is_running() {
            self.start(None);
        }
        info!("Autonomy runtime resumed by operator");
    }

    async fn loop_forever(self: Arc<Self>) {
        loop {
            if self.paused.load(Ordering::SeqCst) {
                tokio::time::sleep(Duration::from_secs(2)).await;
                continue;
            }

            let report = self.run_cycle().await;
            self.emit(&report).await;

            let deadline = Instant::now() + self.interval;
            *self.next_cycle_at.write().await = Some(Utc::now() + chrono::Duration::seconds(
                self.interval.as_secs() as i64,
            ));

            while Instant::now() < deadline {
                if self.paused.load(Ordering::SeqCst) {
                    break;
                }
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
    }

    /// Runs one full cycle of real jobs. Never panics; failures are recorded.
    pub async fn run_cycle(&self) -> AutonomyCycleReport {
        let started = Utc::now();
        let started_instant = Instant::now();
        let cycle_id = Uuid::new_v4().to_string();

        if self.automation_engine.is_kill_switch_active() {
            let report = AutonomyCycleReport {
                cycle_id,
                started_at: started.to_rfc3339(),
                finished_at: Utc::now().to_rfc3339(),
                status: "skipped_kill_switch".into(),
                duration_ms: started_instant.elapsed().as_millis() as u64,
                jobs: Vec::new(),
                error: Some("Emergency kill switch is active".into()),
            };
            self.persist(&report).await;
            return report;
        }

        if self.cycle_in_flight.swap(true, Ordering::SeqCst) {
            return AutonomyCycleReport {
                cycle_id,
                started_at: started.to_rfc3339(),
                finished_at: Utc::now().to_rfc3339(),
                status: "skipped_busy".into(),
                duration_ms: 0,
                jobs: Vec::new(),
                error: Some("A cycle is already running".into()),
            };
        }
        let _guard = InFlightGuard(&self.cycle_in_flight);

        info!("Autonomy cycle {} starting ({} jobs)", cycle_id, JOBS.len());

        let mut jobs = Vec::with_capacity(JOBS.len());
        for spec in JOBS {
            if self.automation_engine.is_kill_switch_active() {
                jobs.push(AutonomyJobReport {
                    name: spec.name.into(),
                    program: self.python.clone(),
                    args: std::iter::once(spec.script.to_string())
                        .chain(spec.args.iter().map(|a| a.to_string()))
                        .collect(),
                    exit_code: None,
                    success: false,
                    timed_out: false,
                    skipped: false,
                    duration_ms: 0,
                    summary: "Not executed: emergency kill switch activated mid-cycle".into(),
                });
                break;
            }
            *self.current_job.write().await = Some(spec.name.to_string());
            jobs.push(self.run_job(spec).await);
        }
        *self.current_job.write().await = None;

        let failed = jobs.iter().filter(|j| !j.success).count();
        let status = if jobs.is_empty() || failed == jobs.len() {
            "failed"
        } else if failed > 0 {
            "partial"
        } else {
            "succeeded"
        };

        let report = AutonomyCycleReport {
            cycle_id,
            started_at: started.to_rfc3339(),
            finished_at: Utc::now().to_rfc3339(),
            status: status.into(),
            duration_ms: started_instant.elapsed().as_millis() as u64,
            jobs,
            error: None,
        };

        self.cycles_completed.fetch_add(1, Ordering::SeqCst);
        if failed > 0 {
            self.cycles_failed.fetch_add(1, Ordering::SeqCst);
            let detail = report
                .jobs
                .iter()
                .filter(|j| !j.success)
                .map(|j| format!("{}: {}", j.name, j.summary))
                .collect::<Vec<_>>()
                .join(" | ");
            *self.last_error.write().await = Some(detail);
        } else {
            *self.last_error.write().await = None;
        }

        *self.last_cycle.write().await = Some(report.clone());
        self.persist(&report).await;

        if failed > 0 {
            warn!(
                "Autonomy cycle {} finished {} ({}/{} jobs failed)",
                report.cycle_id, report.status, failed, report.jobs.len()
            );
        } else {
            info!("Autonomy cycle {} succeeded", report.cycle_id);
        }

        report
    }

    async fn run_job(&self, spec: &JobSpec) -> AutonomyJobReport {
        let started = Instant::now();
        let mut args: Vec<String> = Vec::with_capacity(spec.args.len() + 1);
        args.push(spec.script.to_string());
        args.extend(spec.args.iter().map(|a| (*a).to_string()));

        let root = match spec.root {
            JobRoot::Studio => &self.studio_root,
            JobRoot::Influencer => &self.influencer_root,
        };
        if !root.is_dir() {
            return AutonomyJobReport {
                name: spec.name.into(),
                program: self.python.clone(),
                args,
                exit_code: None,
                success: false,
                timed_out: false,
                skipped: false,
                duration_ms: 0,
                summary: format!("Project folder not found: {}", root.display()),
            };
        }

        if let Some(cadence) = &spec.cadence {
            let marker = root.join(cadence.marker);
            let age = std::fs::metadata(&marker)
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.elapsed().ok());
            if let Some(age) = age.filter(|a| a.as_secs() < cadence.min_age_secs) {
                return AutonomyJobReport {
                    name: spec.name.into(),
                    program: self.python.clone(),
                    args,
                    exit_code: None,
                    success: true,
                    timed_out: false,
                    skipped: true,
                    duration_ms: 0,
                    summary: format!(
                        "Skipped: last run {} ago, studio cadence is {}",
                        human_duration(age.as_secs()),
                        human_duration(cadence.min_age_secs)
                    ),
                };
            }
        }

        let mut command = Command::new(&self.python);
        command
            .arg(spec.script)
            .args(spec.args)
            .current_dir(root)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);

        let child = match command.spawn() {
            Ok(child) => child,
            Err(e) => {
                return AutonomyJobReport {
                    name: spec.name.into(),
                    program: self.python.clone(),
                    args,
                    exit_code: None,
                    success: false,
                    timed_out: false,
                    skipped: false,
                    duration_ms: started.elapsed().as_millis() as u64,
                    summary: format!("Failed to launch: {e}"),
                }
            }
        };

        if let Some(cadence) = spec.cadence.as_ref().filter(|c| c.touch) {
            // Same stamp format the studio's autopilot writes, so it skips this job too.
            let marker = root.join(cadence.marker);
            if let Some(parent) = marker.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
            if let Err(e) = std::fs::write(&marker, stamp) {
                warn!("Cannot write cadence stamp {}: {e}", marker.display());
            }
        }

        // On timeout the child is dropped and killed (kill_on_drop); it is the
        // real interpreter, not a launcher, so nothing is left running.
        match tokio::time::timeout(Duration::from_secs(spec.timeout_secs), child.wait_with_output()).await {
            Err(_) => AutonomyJobReport {
                name: spec.name.into(),
                program: self.python.clone(),
                args,
                exit_code: None,
                success: false,
                timed_out: true,
                skipped: false,
                duration_ms: started.elapsed().as_millis() as u64,
                summary: format!("Timed out after {}s", spec.timeout_secs),
            },
            Ok(Err(e)) => AutonomyJobReport {
                name: spec.name.into(),
                program: self.python.clone(),
                args,
                exit_code: None,
                success: false,
                timed_out: false,
                skipped: false,
                duration_ms: started.elapsed().as_millis() as u64,
                summary: format!("Failed while waiting for the job: {e}"),
            },
            Ok(Ok(output)) => {
                let code = output.status.code();
                let stdout = String::from_utf8_lossy(&output.stdout);
                let stderr = String::from_utf8_lossy(&output.stderr);
                // Exit code 3 is the pipelines' "waiting on a human gate or a busy
                // resource": nothing went wrong, nothing was done.
                let waiting = code == Some(3);
                AutonomyJobReport {
                    name: spec.name.into(),
                    program: self.python.clone(),
                    args,
                    exit_code: code,
                    success: output.status.success() || waiting,
                    timed_out: false,
                    skipped: waiting,
                    duration_ms: started.elapsed().as_millis() as u64,
                    summary: summarise(&stdout, &stderr),
                }
            }
        }
    }

    async fn persist(&self, report: &AutonomyCycleReport) {
        let jobs_json = serde_json::to_string(&report.jobs).unwrap_or_else(|_| "[]".into());
        let failed = report.jobs.iter().filter(|j| !j.success).count() as i64;

        let result = sqlx::query(
            "INSERT INTO autonomy_cycles
                (id, started_at, finished_at, status, job_count, failed_count, jobs_json, error, created_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&report.cycle_id)
        .bind(&report.started_at)
        .bind(&report.finished_at)
        .bind(&report.status)
        .bind(report.jobs.len() as i64)
        .bind(failed)
        .bind(jobs_json)
        .bind(&report.error)
        .bind(Utc::now().to_rfc3339())
        .execute(self.db.pool())
        .await;

        if let Err(e) = result {
            error!("Failed to persist autonomy cycle {}: {e}", report.cycle_id);
        }
    }

    pub async fn recent_cycles(&self, limit: i64) -> Vec<AutonomyCycleReport> {
        let rows: Vec<(String, String, String, String, String, Option<String>)> = sqlx::query_as(
            "SELECT id, started_at, finished_at, status, jobs_json, error
             FROM autonomy_cycles
             ORDER BY started_at DESC
             LIMIT ?",
        )
        .bind(limit.clamp(1, 200))
        .fetch_all(self.db.pool())
        .await
        .unwrap_or_else(|e| {
            error!("Cannot read autonomy cycles: {e}");
            Vec::new()
        });

        rows.into_iter()
            .map(|(id, started_at, finished_at, status, jobs_json, error)| {
                let jobs: Vec<AutonomyJobReport> = serde_json::from_str(&jobs_json)
                    .unwrap_or_else(|e| {
                        warn!("Cycle {id} has unreadable job records: {e}");
                        Vec::new()
                    });
                // Wall-clock time of the whole cycle, not the sum of its jobs.
                let duration_ms = DateTime::parse_from_rfc3339(&finished_at)
                    .and_then(|f| DateTime::parse_from_rfc3339(&started_at).map(|s| (f - s).num_milliseconds()))
                    .map(|ms| ms.max(0) as u64)
                    .unwrap_or_else(|_| jobs.iter().map(|j| j.duration_ms).sum());
                AutonomyCycleReport {
                    cycle_id: id,
                    started_at,
                    finished_at,
                    status,
                    duration_ms,
                    jobs,
                    error,
                }
            })
            .collect()
    }

    async fn emit(&self, report: &AutonomyCycleReport) {
        if let Some(app) = self.app_handle.get() {
            use tauri::Emitter;
            if let Err(e) = app.emit("autonomy://cycle", report.clone()) {
                warn!("Failed to emit autonomy cycle event: {e}");
            }
        }
    }
}

fn human_duration(secs: u64) -> String {
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m", s / 60),
        s if s % 3600 == 0 => format!("{}h", s / 3600),
        s => format!("{}h {}m", s / 3600, (s % 3600) / 60),
    }
}

fn summarise(stdout: &str, stderr: &str) -> String {
    let mut parts: Vec<String> = Vec::new();
    for line in stdout.lines().rev().take(3) {
        let line = line.trim();
        if !line.is_empty() {
            parts.push(line.to_string());
        }
    }
    parts.reverse();
    let mut summary = parts.join(" | ");

    if summary.is_empty() {
        for line in stderr.lines().rev().take(3) {
            let line = line.trim();
            if !line.is_empty() {
                summary = line.to_string();
                break;
            }
        }
    }

    if summary.is_empty() {
        summary = "(no output)".into();
    }
    if summary.chars().count() > MAX_SUMMARY_CHARS {
        summary = summary.chars().take(MAX_SUMMARY_CHARS).collect::<String>() + "…";
    }
    summary
}
