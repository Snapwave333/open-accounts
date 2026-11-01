use chrono::{TimeZone, Utc};
use openaccounts_scheduler::{JobHandler, Schedule, TaskScheduler};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[test]
fn cron_and_interval_expressions_compute_real_next_runs() {
    let base = Utc.with_ymd_and_hms(2026, 11, 1, 10, 7, 30).unwrap();

    let every = Schedule::parse("every 15m").unwrap();
    assert_eq!(every.next_after(base).unwrap(), base + chrono::Duration::minutes(15));

    let quarter = Schedule::parse("*/15 * * * *").unwrap();
    assert_eq!(quarter.next_after(base).unwrap(), Utc.with_ymd_and_hms(2026, 11, 1, 10, 15, 0).unwrap());

    let weekday_nine = Schedule::parse("0 9 * * 1-5").unwrap();
    assert_eq!(weekday_nine.next_after(base).unwrap(), Utc.with_ymd_and_hms(2026, 11, 2, 9, 0, 0).unwrap());

    let daily = Schedule::parse("@daily").unwrap();
    assert_eq!(daily.next_after(base).unwrap(), Utc.with_ymd_and_hms(2026, 11, 2, 0, 0, 0).unwrap());

    let feb29 = Schedule::parse("30 12 29 2 *").unwrap();
    assert_eq!(feb29.next_after(base).unwrap(), Utc.with_ymd_and_hms(2028, 2, 29, 12, 30, 0).unwrap());

    assert!(Schedule::parse("61 * * * *").is_err());
    assert!(Schedule::parse("every 0m").is_err());
    assert!(Schedule::parse("* * *").is_err());
}

#[tokio::test]
async fn due_jobs_run_handlers_and_reschedule() {
    let scheduler = TaskScheduler::new();
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = calls.clone();
    let handler: JobHandler = Arc::new(move |_job| {
        let counter = counter.clone();
        Box::pin(async move {
            counter.fetch_add(1, Ordering::SeqCst);
            Ok("ran".to_string())
        })
    });
    let id = scheduler
        .register_handler("policy scan".into(), "every 10s".into(), handler)
        .await
        .unwrap();

    let now = Utc::now();
    assert!(scheduler.run_due(now).await.is_empty());

    let later = now + chrono::Duration::seconds(11);
    let outcomes = scheduler.run_due(later).await;
    assert_eq!(outcomes.len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);

    let job = scheduler.list_jobs().await.into_iter().find(|j| j.id == id).unwrap();
    assert_eq!(job.run_count, 1);
    assert_eq!(job.last_outcome.as_deref(), Some("ran"));
    assert!(job.next_run > later);

    scheduler.set_active(&id, false).await.unwrap();
    assert!(scheduler.run_due(later + chrono::Duration::hours(1)).await.is_empty());
    assert_eq!(scheduler.active_job_count().await, 0);
}
