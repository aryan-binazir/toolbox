use std::str::FromStr;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, TimeDelta, Utc};
use chrono_tz::Tz;
use cron::Schedule as CronSchedule;
use serde::Serialize;

use crate::config::{normalize_cron, AppConfig, RoutineConfig};
use crate::store::{NewRun, RunStatus};
use crate::AppState;

const FRESH_RUN_WINDOW: TimeDelta = TimeDelta::seconds(60);

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RoutineScheduleInfo {
    pub routine_id: String,
    pub next_run_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SchedulePreview {
    pub next_run_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

pub fn start_scheduler(state: AppState, _app_handle: tauri::AppHandle) {
    thread::spawn(move || {
        let _ = reconcile_missed_since_last_check(&state, Utc::now());
        loop {
            thread::sleep(Duration::from_secs(30));
            let now = Utc::now();
            let _ = run_due_since_last_check(&state, now);
        }
    });
}

pub fn reconcile_missed_since_last_check(
    state: &AppState,
    now: DateTime<Utc>,
) -> Result<(), String> {
    let store = state.store();
    let Some(last_checked) = store
        .scheduler_last_checked()
        .map_err(|err| err.to_string())?
    else {
        store
            .set_scheduler_last_checked(now)
            .map_err(|err| err.to_string())?;
        return Ok(());
    };

    let config = state.config();
    for routine in config.routines.iter().filter(|routine| !routine.paused) {
        let routine_id = routine.id.clone().unwrap_or_default();
        for_each_due_occurrence(&config, routine, last_checked, now, |due| {
            let _ = store.create_run(NewRun {
                routine_id: routine_id.clone(),
                routine_title: routine.title.clone(),
                status: RunStatus::Missed,
                scheduled_for: Some(due),
                command: vec![],
                cwd: routine.cwd.display().to_string(),
                cancel_reason: Some("app_closed".to_string()),
            });
        });
    }
    let _ = store.prune(
        config.settings.max_runs_per_routine,
        config.settings.max_run_age_days,
    );
    store
        .set_scheduler_last_checked(now)
        .map_err(|err| err.to_string())
}

pub fn run_due_since_last_check(state: &AppState, now: DateTime<Utc>) -> Result<(), String> {
    let store = state.store();
    let last_checked = store
        .scheduler_last_checked()
        .map_err(|err| err.to_string())?
        .unwrap_or(now);
    let config = state.config();

    for routine in config.routines.iter().filter(|routine| !routine.paused) {
        let Some(runner) = config
            .runners
            .iter()
            .find(|runner| runner.id == routine.runner)
        else {
            continue;
        };
        let mut fresh_due = Vec::new();
        let routine_id = routine.id.clone().unwrap_or_default();
        for_each_due_occurrence(&config, routine, last_checked, now, |due| {
            if now.signed_duration_since(due) <= FRESH_RUN_WINDOW {
                fresh_due.push(due);
            } else {
                let _ = store.create_run(NewRun {
                    routine_id: routine_id.clone(),
                    routine_title: routine.title.clone(),
                    status: RunStatus::Missed,
                    scheduled_for: Some(due),
                    command: vec![],
                    cwd: routine.cwd.display().to_string(),
                    cancel_reason: Some("scheduler_late".to_string()),
                });
            }
        });
        for due in fresh_due {
            let _ = state.process_manager().start_run(
                store.clone(),
                config.settings.clone(),
                runner.clone(),
                routine.clone(),
                Some(due),
            );
        }
    }
    let _ = store.prune(
        config.settings.max_runs_per_routine,
        config.settings.max_run_age_days,
    );

    store
        .set_scheduler_last_checked(now)
        .map_err(|err| err.to_string())
}

#[cfg(test)]
struct DueByFreshness {
    fresh: Vec<DateTime<Utc>>,
    missed: Vec<DateTime<Utc>>,
}

#[cfg(test)]
fn due_occurrences_by_freshness(
    config: &AppConfig,
    routine: &RoutineConfig,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> DueByFreshness {
    let (fresh, missed) = due_occurrences(config, routine, since, until)
        .into_iter()
        .partition(|due| until.signed_duration_since(*due) <= FRESH_RUN_WINDOW);
    DueByFreshness { fresh, missed }
}

pub fn due_occurrences(
    config: &AppConfig,
    routine: &RoutineConfig,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
) -> Vec<DateTime<Utc>> {
    let mut due = Vec::new();
    for_each_due_occurrence(config, routine, since, until, |occurrence| {
        due.push(occurrence)
    });
    due
}

pub fn routine_schedule_infos(config: &AppConfig, now: DateTime<Utc>) -> Vec<RoutineScheduleInfo> {
    config
        .routines
        .iter()
        .filter_map(|routine| {
            let routine_id = routine.id.clone()?;
            let preview = preview_schedule(config, routine, now);
            Some(RoutineScheduleInfo {
                routine_id,
                next_run_at: preview.next_run_at,
                error: preview.error,
            })
        })
        .collect()
}

pub fn preview_schedule(
    config: &AppConfig,
    routine: &RoutineConfig,
    now: DateTime<Utc>,
) -> SchedulePreview {
    match next_due_occurrence(config, routine, now) {
        Ok(next_run_at) => SchedulePreview {
            next_run_at,
            error: None,
        },
        Err(error) => SchedulePreview {
            next_run_at: None,
            error: Some(error),
        },
    }
}

fn next_due_occurrence(
    config: &AppConfig,
    routine: &RoutineConfig,
    after: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>, String> {
    let timezone = routine
        .timezone
        .as_deref()
        .unwrap_or(config.settings.timezone.as_str());
    let tz = timezone
        .parse::<Tz>()
        .map_err(|_| format!("invalid timezone `{timezone}`"))?;
    let schedule = CronSchedule::from_str(&normalize_cron(&routine.schedule))
        .map_err(|err| format!("schedule is invalid: {err}"))?;
    Ok(schedule
        .after(&after.with_timezone(&tz))
        .next()
        .map(|due| due.with_timezone(&Utc)))
}

fn for_each_due_occurrence(
    config: &AppConfig,
    routine: &RoutineConfig,
    since: DateTime<Utc>,
    until: DateTime<Utc>,
    mut on_due: impl FnMut(DateTime<Utc>),
) {
    if since >= until {
        return;
    }
    let timezone = routine
        .timezone
        .as_deref()
        .unwrap_or(config.settings.timezone.as_str());
    let Ok(tz) = timezone.parse::<Tz>() else {
        return;
    };
    let Ok(schedule) = CronSchedule::from_str(&normalize_cron(&routine.schedule)) else {
        return;
    };

    let start = since.with_timezone(&tz);
    for due in schedule
        .after(&start)
        .map(|due| due.with_timezone(&Utc))
        .take_while(|due| *due <= until)
    {
        on_due(due);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use chrono::TimeZone;

    use super::*;
    use crate::config::{AppConfig, RoutineConfig, Settings};

    #[test]
    fn finds_due_occurrences_with_timezone() {
        let config = AppConfig {
            settings: Settings {
                timezone: "America/New_York".to_string(),
                ..Settings::default()
            },
            runners: vec![],
            routines: vec![],
        };
        let routine = RoutineConfig {
            id: Some("rtn_a".to_string()),
            title: "Routine".to_string(),
            description: String::new(),
            prompt: "Do it.".to_string(),
            runner: "codex".to_string(),
            model: Some("gpt-5.5".to_string()),
            effort: Some("xhigh".to_string()),
            cwd: PathBuf::from("/tmp"),
            schedule: "0 7 * * Sat".to_string(),
            timezone: None,
            paused: false,
            dangerous: false,
            timeout_seconds: None,
        };
        let since = Utc.with_ymd_and_hms(2026, 7, 3, 0, 0, 0).unwrap();
        let until = Utc.with_ymd_and_hms(2026, 7, 5, 0, 0, 0).unwrap();

        let due = due_occurrences(&config, &routine, since, until);

        assert_eq!(due.len(), 1);
        assert_eq!(due[0], Utc.with_ymd_and_hms(2026, 7, 4, 11, 0, 0).unwrap());
    }

    #[test]
    fn classifies_stale_due_occurrences_as_missed() {
        let config = AppConfig {
            settings: Settings {
                timezone: "UTC".to_string(),
                ..Settings::default()
            },
            runners: vec![],
            routines: vec![],
        };
        let routine = RoutineConfig {
            id: Some("rtn_hourly".to_string()),
            title: "Routine".to_string(),
            description: String::new(),
            prompt: "Do it.".to_string(),
            runner: "codex".to_string(),
            model: Some("gpt-5.5".to_string()),
            effort: Some("xhigh".to_string()),
            cwd: PathBuf::from("/tmp"),
            schedule: "0 * * * *".to_string(),
            timezone: None,
            paused: false,
            dangerous: false,
            timeout_seconds: None,
        };
        let since = Utc.with_ymd_and_hms(2026, 7, 3, 0, 0, 0).unwrap();
        let delayed_until = Utc.with_ymd_and_hms(2026, 7, 3, 4, 10, 0).unwrap();

        let delayed = due_occurrences_by_freshness(&config, &routine, since, delayed_until);

        assert!(delayed.fresh.is_empty());
        assert_eq!(delayed.missed.len(), 4);

        let on_time_until = Utc.with_ymd_and_hms(2026, 7, 3, 4, 0, 30).unwrap();
        let on_time = due_occurrences_by_freshness(&config, &routine, since, on_time_until);

        assert_eq!(
            on_time.fresh,
            vec![Utc.with_ymd_and_hms(2026, 7, 3, 4, 0, 0).unwrap()]
        );
        assert_eq!(on_time.missed.len(), 3);
    }

    #[test]
    fn due_occurrence_enumeration_is_not_capped_at_thirty_two() {
        let config = AppConfig {
            settings: Settings {
                timezone: "UTC".to_string(),
                ..Settings::default()
            },
            runners: vec![],
            routines: vec![],
        };
        let routine = RoutineConfig {
            id: Some("rtn_hourly".to_string()),
            title: "Routine".to_string(),
            description: String::new(),
            prompt: "Do it.".to_string(),
            runner: "codex".to_string(),
            model: Some("gpt-5.5".to_string()),
            effort: Some("xhigh".to_string()),
            cwd: PathBuf::from("/tmp"),
            schedule: "0 * * * *".to_string(),
            timezone: None,
            paused: false,
            dangerous: false,
            timeout_seconds: None,
        };
        let since = Utc.with_ymd_and_hms(2026, 7, 1, 0, 0, 0).unwrap();
        let until = Utc.with_ymd_and_hms(2026, 7, 3, 0, 0, 0).unwrap();

        let due = due_occurrences(&config, &routine, since, until);

        assert_eq!(due.len(), 48);
    }

    #[test]
    fn previews_next_due_occurrence() {
        let config = AppConfig {
            settings: Settings {
                timezone: "America/New_York".to_string(),
                ..Settings::default()
            },
            runners: vec![],
            routines: vec![],
        };
        let routine = RoutineConfig {
            id: Some("rtn_preview".to_string()),
            title: "Routine".to_string(),
            description: String::new(),
            prompt: "Do it.".to_string(),
            runner: "codex".to_string(),
            model: Some("gpt-5.5".to_string()),
            effort: Some("xhigh".to_string()),
            cwd: PathBuf::from("/tmp"),
            schedule: "0 7 * * Sat".to_string(),
            timezone: None,
            paused: false,
            dangerous: false,
            timeout_seconds: None,
        };
        let now = Utc.with_ymd_and_hms(2026, 7, 3, 0, 0, 0).unwrap();

        let preview = preview_schedule(&config, &routine, now);

        assert_eq!(
            preview.next_run_at,
            Some(Utc.with_ymd_and_hms(2026, 7, 4, 11, 0, 0).unwrap())
        );
        assert_eq!(preview.error, None);
    }

    #[test]
    fn schedule_preview_reports_invalid_cron() {
        let config = AppConfig {
            settings: Settings::default(),
            runners: vec![],
            routines: vec![],
        };
        let routine = RoutineConfig {
            id: Some("rtn_bad".to_string()),
            title: "Routine".to_string(),
            description: String::new(),
            prompt: "Do it.".to_string(),
            runner: "codex".to_string(),
            model: Some("gpt-5.5".to_string()),
            effort: Some("xhigh".to_string()),
            cwd: PathBuf::from("/tmp"),
            schedule: "not cron".to_string(),
            timezone: None,
            paused: false,
            dangerous: false,
            timeout_seconds: None,
        };

        let preview = preview_schedule(&config, &routine, Utc::now());

        assert!(preview.next_run_at.is_none());
        assert!(preview.error.unwrap().contains("schedule is invalid"));
    }
}
