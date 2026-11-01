use chrono::{DateTime, Datelike, Duration, DurationRound, Timelike, Utc};
use openaccounts_core::{OpenAccountsError, Result, TaskId};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc};
use tokio::sync::RwLock;
use tracing::{info, warn};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Schedule {
    Every { seconds: u64 },
    Cron(CronSpec),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CronSpec {
    minutes: Vec<u32>,
    hours: Vec<u32>,
    days_of_month: Vec<u32>,
    months: Vec<u32>,
    days_of_week: Vec<u32>,
    dom_restricted: bool,
    dow_restricted: bool,
}

fn parse_field(field: &str, min: u32, max: u32) -> Result<(Vec<u32>, bool)> {
    let bad = || OpenAccountsError::Internal(format!("Invalid cron field '{field}'"));
    let mut values = Vec::new();
    let restricted = field != "*";
    for part in field.split(',') {
        let (range, step) = match part.split_once('/') {
            Some((r, s)) => (r, s.parse::<u32>().map_err(|_| bad())?),
            None => (part, 1),
        };
        if step == 0 {
            return Err(bad());
        }
        let (lo, hi) = if range == "*" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            (a.parse().map_err(|_| bad())?, b.parse().map_err(|_| bad())?)
        } else {
            let v: u32 = range.parse().map_err(|_| bad())?;
            (v, if part.contains('/') { max } else { v })
        };
        if lo < min || hi > max || lo > hi {
            return Err(bad());
        }
        values.extend((lo..=hi).step_by(step as usize));
    }
    values.sort_unstable();
    values.dedup();
    Ok((values, restricted))
}

impl CronSpec {
    pub fn parse(expr: &str) -> Result<Self> {
        let fields: Vec<&str> = expr.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(OpenAccountsError::Internal(format!(
                "Cron expression '{expr}' must have 5 fields (minute hour day-of-month month day-of-week)"
            )));
        }
        let (minutes, _) = parse_field(fields[0], 0, 59)?;
        let (hours, _) = parse_field(fields[1], 0, 23)?;
        let (days_of_month, dom_restricted) = parse_field(fields[2], 1, 31)?;
        let (months, _) = parse_field(fields[3], 1, 12)?;
        let (mut days_of_week, dow_restricted) = parse_field(fields[4].replace('7', "0").as_str(), 0, 6)?;
        days_of_week.sort_unstable();
        days_of_week.dedup();
        Ok(Self { minutes, hours, days_of_month, months, days_of_week, dom_restricted, dow_restricted })
    }

    fn day_matches(&self, t: &DateTime<Utc>) -> bool {
        let dom = self.days_of_month.contains(&t.day());
        let dow = self.days_of_week.contains(&t.weekday().num_days_from_sunday());
        match (self.dom_restricted, self.dow_restricted) {
            (true, true) => dom || dow,
            (true, false) => dom,
            (false, true) => dow,
            (false, false) => true,
        }
    }

    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        let mut t = after.duration_trunc(Duration::minutes(1)).ok()? + Duration::minutes(1);
        let limit = after + Duration::days(366 * 5);
        while t <= limit {
            if !self.months.contains(&t.month()) {
                t = (t + Duration::days(1)).with_hour(0)?.with_minute(0)?;
                continue;
            }
            if !self.day_matches(&t) {
                t = (t + Duration::days(1)).with_hour(0)?.with_minute(0)?;
                continue;
            }
            if !self.hours.contains(&t.hour()) {
                t = (t + Duration::hours(1)).with_minute(0)?;
                continue;
            }
            if !self.minutes.contains(&t.minute()) {
                t += Duration::minutes(1);
                continue;
            }
            return Some(t);
        }
        None
    }
}

impl Schedule {
    pub fn parse(expr: &str) -> Result<Self> {
        let e = expr.trim().to_lowercase();
        let alias = match e.as_str() {
            "@hourly" => Some("0 * * * *"),
            "@daily" | "@midnight" => Some("0 0 * * *"),
            "@weekly" => Some("0 0 * * 0"),
            "@monthly" => Some("0 0 1 * *"),
            "@yearly" | "@annually" => Some("0 0 1 1 *"),
            _ => None,
        };
        if let Some(cron) = alias {
            return Ok(Schedule::Cron(CronSpec::parse(cron)?));
        }
        if let Some(rest) = e.strip_prefix("every ").or_else(|| e.strip_prefix("@every ")) {
            let rest = rest.trim();
            let split = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
            let (num, unit) = rest.split_at(split);
            let n: u64 = num
                .parse()
                .map_err(|_| OpenAccountsError::Internal(format!("Invalid interval '{expr}'")))?;
            let mult = match unit.trim() {
                "s" | "sec" | "secs" | "second" | "seconds" => 1,
                "m" | "min" | "mins" | "minute" | "minutes" => 60,
                "h" | "hr" | "hrs" | "hour" | "hours" => 3600,
                "d" | "day" | "days" => 86_400,
                _ => return Err(OpenAccountsError::Internal(format!("Unknown interval unit in '{expr}'"))),
            };
            if n == 0 {
                return Err(OpenAccountsError::Internal("Interval must be greater than zero".into()));
            }
            return Ok(Schedule::Every { seconds: n * mult });
        }
        Ok(Schedule::Cron(CronSpec::parse(&e)?))
    }

    pub fn next_after(&self, after: DateTime<Utc>) -> Option<DateTime<Utc>> {
        match self {
            Schedule::Every { seconds } => Some(after + Duration::seconds(*seconds as i64)),
            Schedule::Cron(spec) => spec.next_after(after),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleJob {
    pub id: TaskId,
    pub name: String,
    pub cron_expression: String,
    pub next_run: DateTime<Utc>,
    pub last_run: Option<DateTime<Utc>>,
    pub last_outcome: Option<String>,
    pub run_count: u64,
    pub is_active: bool,
}

pub type JobFuture = Pin<Box<dyn Future<Output = Result<String>> + Send>>;
pub type JobHandler = Arc<dyn Fn(ScheduleJob) -> JobFuture + Send + Sync>;

#[derive(Default)]
pub struct TaskScheduler {
    jobs: Arc<RwLock<HashMap<TaskId, (ScheduleJob, Schedule)>>>,
    handlers: Arc<RwLock<HashMap<TaskId, JobHandler>>>,
}

impl TaskScheduler {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn register_job(&self, name: String, cron_expression: String) -> Result<TaskId> {
        let schedule = Schedule::parse(&cron_expression)?;
        let next_run = schedule
            .next_after(Utc::now())
            .ok_or_else(|| OpenAccountsError::Internal(format!("'{cron_expression}' never fires")))?;
        let id = TaskId::new();
        let job = ScheduleJob {
            id: id.clone(),
            name,
            cron_expression,
            next_run,
            last_run: None,
            last_outcome: None,
            run_count: 0,
            is_active: true,
        };
        info!("Scheduled job '{}' next at {}", job.name, job.next_run);
        self.jobs.write().await.insert(id.clone(), (job, schedule));
        Ok(id)
    }

    pub async fn register_handler(&self, name: String, cron_expression: String, handler: JobHandler) -> Result<TaskId> {
        let id = self.register_job(name, cron_expression).await?;
        self.handlers.write().await.insert(id.clone(), handler);
        Ok(id)
    }

    pub async fn set_active(&self, id: &TaskId, active: bool) -> Result<()> {
        let mut jobs = self.jobs.write().await;
        let (job, schedule) = jobs
            .get_mut(id)
            .ok_or_else(|| OpenAccountsError::Internal("Job not found".into()))?;
        job.is_active = active;
        if active {
            if let Some(next) = schedule.next_after(Utc::now()) {
                job.next_run = next;
            }
        }
        Ok(())
    }

    pub async fn remove_job(&self, id: &TaskId) -> bool {
        self.handlers.write().await.remove(id);
        self.jobs.write().await.remove(id).is_some()
    }

    pub async fn list_jobs(&self) -> Vec<ScheduleJob> {
        let mut jobs: Vec<ScheduleJob> = self.jobs.read().await.values().map(|(j, _)| j.clone()).collect();
        jobs.sort_by_key(|a| a.next_run);
        jobs
    }

    pub async fn active_job_count(&self) -> usize {
        self.jobs.read().await.values().filter(|(j, _)| j.is_active).count()
    }

    pub async fn run_due(&self, now: DateTime<Utc>) -> Vec<(TaskId, Result<String>)> {
        let due: Vec<ScheduleJob> = {
            let mut jobs = self.jobs.write().await;
            jobs.values_mut()
                .filter(|(j, _)| j.is_active && j.next_run <= now)
                .map(|(job, schedule)| {
                    let snapshot = job.clone();
                    job.next_run = schedule.next_after(now).unwrap_or(now + Duration::days(365 * 100));
                    snapshot
                })
                .collect()
        };

        let mut outcomes = Vec::with_capacity(due.len());
        for job in due {
            let handler = self.handlers.read().await.get(&job.id).cloned();
            let outcome = match handler {
                Some(h) => h(job.clone()).await,
                None => Ok("no handler registered".to_string()),
            };
            if let Err(e) = &outcome {
                warn!("Scheduled job '{}' failed: {}", job.name, e);
            }
            if let Some((stored, _)) = self.jobs.write().await.get_mut(&job.id) {
                stored.last_run = Some(now);
                stored.run_count += 1;
                stored.last_outcome = Some(match &outcome {
                    Ok(msg) => msg.clone(),
                    Err(e) => format!("error: {e}"),
                });
            }
            outcomes.push((job.id, outcome));
        }
        outcomes
    }

    pub async fn run_now(&self, id: &TaskId) -> Result<String> {
        let job = self
            .jobs
            .read()
            .await
            .get(id)
            .map(|(j, _)| j.clone())
            .ok_or_else(|| OpenAccountsError::Internal("Job not found".into()))?;
        let handler = self
            .handlers
            .read()
            .await
            .get(id)
            .cloned()
            .ok_or_else(|| OpenAccountsError::Internal(format!("Job '{}' has no handler", job.name)))?;
        let outcome = handler(job).await;
        if let Some((stored, _)) = self.jobs.write().await.get_mut(id) {
            stored.last_run = Some(Utc::now());
            stored.run_count += 1;
            stored.last_outcome = Some(match &outcome {
                Ok(msg) => msg.clone(),
                Err(e) => format!("error: {e}"),
            });
        }
        outcome
    }

    pub fn spawn_runner(self: Arc<Self>, tick: std::time::Duration) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tick);
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            loop {
                interval.tick().await;
                self.run_due(Utc::now()).await;
            }
        })
    }
}
