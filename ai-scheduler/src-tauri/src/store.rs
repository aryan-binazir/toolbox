use std::fs;
use std::path::Path;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use nanoid::nanoid;
use rusqlite::types::Type;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use thiserror::Error;

const STORE_SCHEMA_VERSION: &str = "1";

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("timestamp parse error: {0}")]
    Time(#[from] chrono::ParseError),
    #[error("run `{0}` was not found")]
    RunNotFound(String),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
    TimedOut,
    Missed,
    Superseded,
}

impl RunStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::TimedOut => "timed_out",
            Self::Missed => "missed",
            Self::Superseded => "superseded",
        }
    }

    fn from_str(value: &str) -> rusqlite::Result<Self> {
        match value {
            "queued" => Ok(Self::Queued),
            "running" => Ok(Self::Running),
            "succeeded" => Ok(Self::Succeeded),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            "timed_out" => Ok(Self::TimedOut),
            "missed" => Ok(Self::Missed),
            "superseded" => Ok(Self::Superseded),
            _ => Err(rusqlite::Error::FromSqlConversionFailure(
                0,
                Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("unknown run status `{value}`"),
                )),
            )),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunRecord {
    pub id: String,
    pub routine_id: String,
    pub routine_title: String,
    pub status: RunStatus,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub cancel_reason: Option<String>,
    pub command: Vec<String>,
    pub cwd: String,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

#[derive(Debug, Clone)]
pub struct NewRun {
    pub routine_id: String,
    pub routine_title: String,
    pub status: RunStatus,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub command: Vec<String>,
    pub cwd: String,
    pub cancel_reason: Option<String>,
}

#[derive(Debug, Clone)]
pub struct FinishRun {
    pub status: RunStatus,
    pub finished_at: DateTime<Utc>,
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    pub cancel_reason: Option<String>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

pub struct RunStore {
    conn: Mutex<Connection>,
}

impl RunStore {
    pub fn new_run_id() -> String {
        format!("run_{}", nanoid!(16))
    }

    pub fn open(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path)?;
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    pub fn in_memory() -> Result<Self, StoreError> {
        let store = Self {
            conn: Mutex::new(Connection::open_in_memory()?),
        };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock poisoned");
        conn.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            CREATE TABLE IF NOT EXISTS runs (
                id TEXT PRIMARY KEY,
                routine_id TEXT NOT NULL,
                routine_title TEXT NOT NULL,
                status TEXT NOT NULL,
                scheduled_for TEXT,
                started_at TEXT,
                finished_at TEXT,
                exit_code INTEGER,
                signal INTEGER,
                cancel_reason TEXT,
                command_json TEXT NOT NULL DEFAULT '[]',
                cwd TEXT NOT NULL DEFAULT '',
                stdout TEXT NOT NULL DEFAULT '',
                stderr TEXT NOT NULL DEFAULT '',
                stdout_truncated INTEGER NOT NULL DEFAULT 0,
                stderr_truncated INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_runs_routine_created ON runs (routine_id, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_runs_status ON runs (status);
            CREATE TABLE IF NOT EXISTS scheduler_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS app_meta (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;
        conn.execute(
            r#"
            INSERT INTO app_meta (key, value)
            VALUES ('schema_version', ?1)
            ON CONFLICT(key) DO NOTHING
            "#,
            params![STORE_SCHEMA_VERSION],
        )?;
        Ok(())
    }

    pub fn create_run(&self, run: NewRun) -> Result<RunRecord, StoreError> {
        self.create_run_with_id(Self::new_run_id(), run)
    }

    pub fn create_run_with_id(&self, id: String, run: NewRun) -> Result<RunRecord, StoreError> {
        let now = Utc::now();
        let command_json = serde_json::to_string(&run.command).unwrap_or_else(|_| "[]".to_string());
        let conn = self.conn.lock().expect("store lock poisoned");
        conn.execute(
            r#"
            INSERT INTO runs (
                id, routine_id, routine_title, status, scheduled_for, command_json, cwd,
                cancel_reason, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
            "#,
            params![
                id,
                run.routine_id,
                run.routine_title,
                run.status.as_str(),
                opt_time(run.scheduled_for),
                command_json,
                run.cwd,
                run.cancel_reason,
                fmt_time(now),
            ],
        )?;
        drop(conn);
        self.get_run(&id)?
            .ok_or_else(|| StoreError::RunNotFound(id))
    }

    pub fn mark_running(&self, run_id: &str, started_at: DateTime<Utc>) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock poisoned");
        let changed = conn.execute(
            "UPDATE runs SET status = ?1, started_at = ?2 WHERE id = ?3",
            params![RunStatus::Running.as_str(), fmt_time(started_at), run_id],
        )?;
        ensure_run_changed(changed, run_id)?;
        Ok(())
    }

    pub fn finish_run(&self, run_id: &str, finish: FinishRun) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock poisoned");
        let changed = conn.execute(
            r#"
            UPDATE runs
            SET status = ?1,
                finished_at = ?2,
                exit_code = ?3,
                signal = ?4,
                cancel_reason = ?5,
                stdout = ?6,
                stderr = ?7,
                stdout_truncated = ?8,
                stderr_truncated = ?9
            WHERE id = ?10
            "#,
            params![
                finish.status.as_str(),
                fmt_time(finish.finished_at),
                finish.exit_code,
                finish.signal,
                finish.cancel_reason,
                finish.stdout,
                finish.stderr,
                finish.stdout_truncated as i64,
                finish.stderr_truncated as i64,
                run_id,
            ],
        )?;
        ensure_run_changed(changed, run_id)?;
        Ok(())
    }

    pub fn cancel_active_runs_on_startup(&self) -> Result<usize, StoreError> {
        let conn = self.conn.lock().expect("store lock poisoned");
        let finished_at = fmt_time(Utc::now());
        conn.execute(
            r#"
            UPDATE runs
            SET status = ?1,
                finished_at = ?2,
                cancel_reason = ?3
            WHERE status IN ('queued', 'running')
            "#,
            params![RunStatus::Cancelled.as_str(), finished_at, "app_restarted",],
        )
        .map_err(Into::into)
    }

    pub fn update_stdout(
        &self,
        run_id: &str,
        stdout: &str,
        truncated: bool,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock poisoned");
        let changed = conn.execute(
            "UPDATE runs SET stdout = ?1, stdout_truncated = ?2 WHERE id = ?3",
            params![stdout, truncated as i64, run_id],
        )?;
        ensure_run_changed(changed, run_id)?;
        Ok(())
    }

    pub fn update_stderr(
        &self,
        run_id: &str,
        stderr: &str,
        truncated: bool,
    ) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock poisoned");
        let changed = conn.execute(
            "UPDATE runs SET stderr = ?1, stderr_truncated = ?2 WHERE id = ?3",
            params![stderr, truncated as i64, run_id],
        )?;
        ensure_run_changed(changed, run_id)?;
        Ok(())
    }

    pub fn get_run(&self, run_id: &str) -> Result<Option<RunRecord>, StoreError> {
        let conn = self.conn.lock().expect("store lock poisoned");
        conn.query_row(
            "SELECT * FROM runs WHERE id = ?1",
            params![run_id],
            read_run,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn list_runs_for_routine(&self, routine_id: &str) -> Result<Vec<RunRecord>, StoreError> {
        let conn = self.conn.lock().expect("store lock poisoned");
        let mut stmt =
            conn.prepare("SELECT * FROM runs WHERE routine_id = ?1 ORDER BY created_at DESC")?;
        let rows = stmt.query_map(params![routine_id], read_run)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn scheduled_run_exists(
        &self,
        routine_id: &str,
        scheduled_for: DateTime<Utc>,
    ) -> Result<bool, StoreError> {
        let conn = self.conn.lock().expect("store lock poisoned");
        let exists = conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE routine_id = ?1 AND scheduled_for = ?2)",
            params![routine_id, fmt_time(scheduled_for)],
            |row| row.get::<_, bool>(0),
        )?;
        Ok(exists)
    }

    pub fn delete_runs_for_routine(&self, routine_id: &str) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock poisoned");
        conn.execute(
            "DELETE FROM runs WHERE routine_id = ?1",
            params![routine_id],
        )?;
        Ok(())
    }

    pub fn prune(&self, max_runs_per_routine: u32, max_age_days: u32) -> Result<usize, StoreError> {
        let conn = self.conn.lock().expect("store lock poisoned");
        let age_removed = conn.execute(
            r#"
            DELETE FROM runs
            WHERE status NOT IN ('queued', 'running')
              AND datetime(created_at) < datetime('now', ?1)
            "#,
            params![format!("-{max_age_days} days")],
        )?;
        let count_removed = conn.execute(
            r#"
            DELETE FROM runs
            WHERE id IN (
                SELECT id FROM (
                    SELECT id,
                           ROW_NUMBER() OVER (PARTITION BY routine_id ORDER BY created_at DESC) AS rn
                    FROM runs
                    WHERE status NOT IN ('queued', 'running')
                )
                WHERE rn > ?1
            )
            "#,
            params![max_runs_per_routine],
        )?;
        Ok(age_removed + count_removed)
    }

    pub fn scheduler_last_checked(&self) -> Result<Option<DateTime<Utc>>, StoreError> {
        let conn = self.conn.lock().expect("store lock poisoned");
        let value: Option<String> = conn
            .query_row(
                "SELECT value FROM scheduler_state WHERE key = 'last_checked_at'",
                [],
                |row| row.get(0),
            )
            .optional()?;
        value
            .map(|value| parse_time(&value))
            .transpose()
            .map_err(Into::into)
    }

    pub fn set_scheduler_last_checked(&self, value: DateTime<Utc>) -> Result<(), StoreError> {
        let conn = self.conn.lock().expect("store lock poisoned");
        conn.execute(
            r#"
            INSERT INTO scheduler_state (key, value)
            VALUES ('last_checked_at', ?1)
            ON CONFLICT(key) DO UPDATE SET value = excluded.value
            "#,
            params![fmt_time(value)],
        )?;
        Ok(())
    }

    pub fn schema_version(&self) -> Result<Option<String>, StoreError> {
        let conn = self.conn.lock().expect("store lock poisoned");
        conn.query_row(
            "SELECT value FROM app_meta WHERE key = 'schema_version'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(Into::into)
    }
}

fn ensure_run_changed(changed: usize, run_id: &str) -> Result<(), StoreError> {
    if changed == 1 {
        Ok(())
    } else {
        Err(StoreError::RunNotFound(run_id.to_string()))
    }
}

fn read_run(row: &rusqlite::Row<'_>) -> rusqlite::Result<RunRecord> {
    let command_json: String = row.get("command_json")?;
    let command = serde_json::from_str(&command_json).unwrap_or_default();
    Ok(RunRecord {
        id: row.get("id")?,
        routine_id: row.get("routine_id")?,
        routine_title: row.get("routine_title")?,
        status: RunStatus::from_str(row.get::<_, String>("status")?.as_str())?,
        scheduled_for: parse_opt(row.get("scheduled_for")?),
        started_at: parse_opt(row.get("started_at")?),
        finished_at: parse_opt(row.get("finished_at")?),
        exit_code: row.get("exit_code")?,
        signal: row.get("signal")?,
        cancel_reason: row.get("cancel_reason")?,
        command,
        cwd: row.get("cwd")?,
        stdout: row.get("stdout")?,
        stderr: row.get("stderr")?,
        stdout_truncated: row.get::<_, i64>("stdout_truncated")? != 0,
        stderr_truncated: row.get::<_, i64>("stderr_truncated")? != 0,
    })
}

fn opt_time(value: Option<DateTime<Utc>>) -> Option<String> {
    value.map(fmt_time)
}

fn fmt_time(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

fn parse_opt(value: Option<String>) -> Option<DateTime<Utc>> {
    value.and_then(|value| parse_time(&value).ok())
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, chrono::ParseError> {
    DateTime::parse_from_rfc3339(value).map(|value| value.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stores_runs_and_prunes_per_routine() {
        let store = RunStore::in_memory().unwrap();
        for index in 0..3 {
            store
                .create_run(NewRun {
                    routine_id: "rtn_a".to_string(),
                    routine_title: "Routine A".to_string(),
                    status: RunStatus::Missed,
                    scheduled_for: None,
                    command: vec![],
                    cwd: "/tmp".to_string(),
                    cancel_reason: Some(format!("missed {index}")),
                })
                .unwrap();
        }

        assert_eq!(store.list_runs_for_routine("rtn_a").unwrap().len(), 3);
        assert_eq!(store.prune(2, 90).unwrap(), 1);
        assert_eq!(store.list_runs_for_routine("rtn_a").unwrap().len(), 2);
    }

    #[test]
    fn pruning_keeps_active_runs() {
        let store = RunStore::in_memory().unwrap();
        store
            .create_run(NewRun {
                routine_id: "rtn_a".to_string(),
                routine_title: "Routine".to_string(),
                status: RunStatus::Running,
                scheduled_for: None,
                command: vec![],
                cwd: "/tmp".to_string(),
                cancel_reason: None,
            })
            .unwrap();
        for index in 0..3 {
            store
                .create_run(NewRun {
                    routine_id: "rtn_a".to_string(),
                    routine_title: "Routine".to_string(),
                    status: RunStatus::Succeeded,
                    scheduled_for: None,
                    command: vec![],
                    cwd: "/tmp".to_string(),
                    cancel_reason: Some(format!("done {index}")),
                })
                .unwrap();
        }

        store.prune(1, 90).unwrap();
        let runs = store.list_runs_for_routine("rtn_a").unwrap();

        assert_eq!(runs.len(), 2);
        assert!(runs.iter().any(|run| run.status == RunStatus::Running));
    }

    #[test]
    fn stores_schema_version() {
        let store = RunStore::in_memory().unwrap();

        assert_eq!(
            store.schema_version().unwrap().as_deref(),
            Some(STORE_SCHEMA_VERSION)
        );
    }

    #[test]
    fn updates_output_before_finish() {
        let store = RunStore::in_memory().unwrap();
        let run = store
            .create_run(NewRun {
                routine_id: "rtn_a".to_string(),
                routine_title: "Routine".to_string(),
                status: RunStatus::Running,
                scheduled_for: None,
                command: vec![],
                cwd: "/tmp".to_string(),
                cancel_reason: None,
            })
            .unwrap();

        store.update_stdout(&run.id, "partial", false).unwrap();
        store.update_stderr(&run.id, "warn", true).unwrap();

        let updated = store.get_run(&run.id).unwrap().unwrap();
        assert_eq!(updated.stdout, "partial");
        assert_eq!(updated.stderr, "warn");
        assert!(!updated.stdout_truncated);
        assert!(updated.stderr_truncated);
    }

    #[test]
    fn cancels_stale_active_runs_on_startup() {
        let store = RunStore::in_memory().unwrap();
        let running = store
            .create_run(NewRun {
                routine_id: "rtn_a".to_string(),
                routine_title: "Routine".to_string(),
                status: RunStatus::Running,
                scheduled_for: None,
                command: vec![],
                cwd: "/tmp".to_string(),
                cancel_reason: None,
            })
            .unwrap();

        assert_eq!(store.cancel_active_runs_on_startup().unwrap(), 1);

        let run = store.get_run(&running.id).unwrap().unwrap();
        assert_eq!(run.status, RunStatus::Cancelled);
        assert_eq!(run.cancel_reason.as_deref(), Some("app_restarted"));
        assert!(run.finished_at.is_some());
    }

    #[test]
    fn run_updates_reject_unknown_run_ids() {
        let store = RunStore::in_memory().unwrap();

        assert!(store.mark_running("missing", Utc::now()).is_err());
        assert!(store.update_stdout("missing", "output", false).is_err());
        assert!(store.update_stderr("missing", "error", false).is_err());
        assert!(store
            .finish_run(
                "missing",
                FinishRun {
                    status: RunStatus::Failed,
                    finished_at: Utc::now(),
                    exit_code: None,
                    signal: None,
                    cancel_reason: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    stdout_truncated: false,
                    stderr_truncated: false,
                },
            )
            .is_err());
    }

    #[test]
    fn scheduled_run_lookup_is_scoped_to_routine_and_occurrence() {
        let store = RunStore::in_memory().unwrap();
        let scheduled_for = Utc::now();
        store
            .create_run(NewRun {
                routine_id: "rtn_a".to_string(),
                routine_title: "A".to_string(),
                status: RunStatus::Missed,
                scheduled_for: Some(scheduled_for),
                command: vec![],
                cwd: "/tmp".to_string(),
                cancel_reason: Some("app_closed".to_string()),
            })
            .unwrap();

        assert!(store.scheduled_run_exists("rtn_a", scheduled_for).unwrap());
        assert!(!store.scheduled_run_exists("rtn_b", scheduled_for).unwrap());
    }

    #[test]
    fn corrupted_run_status_is_reported_instead_of_relabelled_failed() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("runs.db");
        let store = RunStore::open(&path).unwrap();
        let run = store
            .create_run(NewRun {
                routine_id: "rtn_a".to_string(),
                routine_title: "A".to_string(),
                status: RunStatus::Succeeded,
                scheduled_for: None,
                command: vec![],
                cwd: "/tmp".to_string(),
                cancel_reason: None,
            })
            .unwrap();
        let external = Connection::open(path).unwrap();
        external
            .execute(
                "UPDATE runs SET status = 'corrupt' WHERE id = ?1",
                params![run.id],
            )
            .unwrap();

        assert!(store.list_runs_for_routine("rtn_a").is_err());
    }
}
