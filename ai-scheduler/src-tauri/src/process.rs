use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use chrono::Utc;
use serde::Serialize;
use thiserror::Error;

use crate::config::{RoutineConfig, RunnerConfig, Settings, StdinMode};
use crate::store::{FinishRun, NewRun, RunRecord, RunStatus, RunStore, StoreError};

#[cfg(unix)]
use std::os::unix::process::{CommandExt, ExitStatusExt};

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error("routine `{0}` is missing an id")]
    MissingRoutineId(String),
    #[error("runner `{0}` is missing a model and no routine model was provided")]
    MissingModel(String),
}

#[derive(Clone, Default)]
pub struct ProcessManager {
    active: Arc<Mutex<HashMap<String, ActiveRun>>>,
}

#[derive(Clone)]
struct ActiveRun {
    run_id: String,
    pgid: Arc<Mutex<Option<i32>>>,
    cancel_reason: Arc<Mutex<Option<CancelReason>>>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CancelReason {
    User,
    Timeout,
    Superseded,
    AppClosed,
}

impl CancelReason {
    fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Timeout => "timeout",
            Self::Superseded => "superseded",
            Self::AppClosed => "app_closed",
        }
    }
}

impl ProcessManager {
    pub fn start_run(
        &self,
        store: Arc<RunStore>,
        settings: Settings,
        runner: RunnerConfig,
        routine: RoutineConfig,
        scheduled_for: Option<chrono::DateTime<Utc>>,
    ) -> Result<RunRecord, ProcessError> {
        let routine_id = routine
            .id
            .clone()
            .ok_or_else(|| ProcessError::MissingRoutineId(routine.title.clone()))?;
        self.cancel_routine(&routine_id, CancelReason::Superseded);

        let argv = expand_args(&runner, &routine)?;
        let command_for_history = std::iter::once(runner.command.clone())
            .chain(argv.iter().cloned())
            .collect::<Vec<_>>();
        let queued = store.create_run(NewRun {
            routine_id: routine_id.clone(),
            routine_title: routine.title.clone(),
            status: RunStatus::Queued,
            scheduled_for,
            command: command_for_history,
            cwd: routine.cwd.display().to_string(),
            cancel_reason: None,
        })?;

        let active = ActiveRun {
            run_id: queued.id.clone(),
            pgid: Arc::new(Mutex::new(None)),
            cancel_reason: Arc::new(Mutex::new(None)),
        };
        self.active
            .lock()
            .expect("active lock poisoned")
            .insert(routine_id.clone(), active.clone());

        let manager = self.clone();
        thread::spawn(move || {
            manager.run_child(store, settings, runner, routine, argv, active);
        });

        Ok(queued)
    }

    pub fn cancel_routine(&self, routine_id: &str, reason: CancelReason) -> Option<String> {
        let active = self
            .active
            .lock()
            .expect("active lock poisoned")
            .get(routine_id)
            .cloned()?;
        *active.cancel_reason.lock().expect("cancel lock poisoned") = Some(reason);
        if let Some(pgid) = *active.pgid.lock().expect("pgid lock poisoned") {
            terminate_process_group(pgid);
            thread::spawn(move || wait_then_kill(pgid, Duration::from_secs(5)));
        }
        Some(active.run_id)
    }

    pub fn cancel_all(&self, reason: CancelReason) {
        let routine_ids = self
            .active
            .lock()
            .expect("active lock poisoned")
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for routine_id in routine_ids {
            self.cancel_routine(&routine_id, reason.clone());
        }
    }

    pub fn cancel_all_and_wait(&self, reason: CancelReason, grace: Duration, max_wait: Duration) {
        for active in self.active_runs() {
            *active.cancel_reason.lock().expect("cancel lock poisoned") = Some(reason.clone());
            if let Some(pgid) = *active.pgid.lock().expect("pgid lock poisoned") {
                terminate_process_group(pgid);
            }
        }

        let grace_deadline = Instant::now() + grace;
        while Instant::now() < grace_deadline && self.has_active_runs() {
            thread::sleep(Duration::from_millis(50));
        }

        for active in self.active_runs() {
            if let Some(pgid) = *active.pgid.lock().expect("pgid lock poisoned") {
                kill_process_group(pgid);
            }
        }

        let wait_deadline = Instant::now() + max_wait;
        while Instant::now() < wait_deadline && self.has_active_runs() {
            thread::sleep(Duration::from_millis(50));
        }
    }

    fn active_runs(&self) -> Vec<ActiveRun> {
        self.active
            .lock()
            .expect("active lock poisoned")
            .values()
            .cloned()
            .collect()
    }

    fn has_active_runs(&self) -> bool {
        !self.active.lock().expect("active lock poisoned").is_empty()
    }

    fn run_child(
        &self,
        store: Arc<RunStore>,
        settings: Settings,
        runner: RunnerConfig,
        routine: RoutineConfig,
        argv: Vec<String>,
        active: ActiveRun,
    ) {
        let timeout = Duration::from_secs(
            routine
                .timeout_seconds
                .or(runner.default_timeout_seconds)
                .unwrap_or(settings.default_timeout_seconds),
        );
        let stream_cap = settings.stream_cap_bytes as usize;
        let started_at = Utc::now();
        let _ = store.mark_running(&active.run_id, started_at);

        let mut command = Command::new(&runner.command);
        command.args(&argv).current_dir(&routine.cwd);
        match runner.stdin {
            StdinMode::Null => {
                command.stdin(Stdio::null());
            }
            StdinMode::Inherit => {
                command.stdin(Stdio::inherit());
            }
        }
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        #[cfg(unix)]
        unsafe {
            command.pre_exec(|| {
                if libc::setpgid(0, 0) == 0 {
                    Ok(())
                } else {
                    Err(std::io::Error::last_os_error())
                }
            });
        }

        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(err) => {
                let _ = store.finish_run(
                    &active.run_id,
                    FinishRun {
                        status: RunStatus::Failed,
                        finished_at: Utc::now(),
                        exit_code: None,
                        signal: None,
                        cancel_reason: None,
                        stdout: String::new(),
                        stderr: err.to_string(),
                        stdout_truncated: false,
                        stderr_truncated: false,
                    },
                );
                if let Some(routine_id) = routine.id.as_deref() {
                    self.remove_active(routine_id, &active.run_id);
                }
                return;
            }
        };

        let pgid = child.id() as i32;
        *active.pgid.lock().expect("pgid lock poisoned") = Some(pgid);
        if active
            .cancel_reason
            .lock()
            .expect("cancel lock poisoned")
            .is_some()
        {
            terminate_process_group(pgid);
            thread::spawn(move || wait_then_kill(pgid, Duration::from_secs(5)));
        }

        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let stdout_reader = thread::spawn(move || read_capped(stdout, stream_cap));
        let stderr_reader = thread::spawn(move || read_capped(stderr, stream_cap));

        let start = Instant::now();
        let exit_status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) if start.elapsed() >= timeout => {
                    *active.cancel_reason.lock().expect("cancel lock poisoned") =
                        Some(CancelReason::Timeout);
                    terminate_process_group(pgid);
                    wait_then_kill(pgid, Duration::from_secs(5));
                }
                Ok(None) => thread::sleep(Duration::from_millis(200)),
                Err(_) => break None,
            }
        };

        let final_status = if exit_status.is_none() {
            child.wait().ok()
        } else {
            exit_status
        };

        let (stdout, stdout_truncated) = stdout_reader
            .join()
            .unwrap_or_else(|_| ("".to_string(), false));
        let (stderr, stderr_truncated) = stderr_reader
            .join()
            .unwrap_or_else(|_| ("".to_string(), false));
        let cancel_reason = active
            .cancel_reason
            .lock()
            .expect("cancel lock poisoned")
            .clone();

        let status = match cancel_reason {
            Some(CancelReason::Timeout) => RunStatus::TimedOut,
            Some(CancelReason::Superseded) => RunStatus::Superseded,
            Some(CancelReason::User | CancelReason::AppClosed) => RunStatus::Cancelled,
            None => match final_status {
                Some(status) if status.success() => RunStatus::Succeeded,
                _ => RunStatus::Failed,
            },
        };

        let _ = store.finish_run(
            &active.run_id,
            FinishRun {
                status,
                finished_at: Utc::now(),
                exit_code: final_status.and_then(|status| status.code()),
                #[cfg(unix)]
                signal: final_status.and_then(|status| status.signal()),
                #[cfg(not(unix))]
                signal: None,
                cancel_reason: cancel_reason.map(|reason| reason.as_str().to_string()),
                stdout,
                stderr,
                stdout_truncated,
                stderr_truncated,
            },
        );
        let _ = store.prune(settings.max_runs_per_routine, settings.max_run_age_days);

        if let Some(routine_id) = routine.id.as_deref() {
            self.remove_active(routine_id, &active.run_id);
        }
    }

    fn remove_active(&self, routine_id: &str, run_id: &str) {
        let mut active = self.active.lock().expect("active lock poisoned");
        if active
            .get(routine_id)
            .is_some_and(|current| current.run_id == run_id)
        {
            active.remove(routine_id);
        }
    }
}

pub fn expand_args(
    runner: &RunnerConfig,
    routine: &RoutineConfig,
) -> Result<Vec<String>, ProcessError> {
    let model = routine
        .model
        .as_deref()
        .or(runner.default_model.as_deref())
        .ok_or_else(|| ProcessError::MissingModel(runner.id.clone()))?;
    let effort = routine
        .effort
        .as_deref()
        .or(runner.default_effort.as_deref())
        .unwrap_or("");
    let routine_id = routine.id.as_deref().unwrap_or("");
    let dangerous_flag = runner.dangerous_flag.as_deref().unwrap_or("");

    let mut args = Vec::new();
    for template in &runner.args {
        if template == "{{dangerous_flag}}" {
            if routine.dangerous && !dangerous_flag.is_empty() {
                args.push(dangerous_flag.to_string());
            }
            continue;
        }
        let rendered = template
            .replace("{{model}}", model)
            .replace("{{effort}}", effort)
            .replace("{{prompt}}", &routine.prompt)
            .replace("{{routine_id}}", routine_id)
            .replace("{{cwd}}", routine.cwd.to_string_lossy().as_ref())
            .replace(
                "{{dangerous_flag}}",
                if routine.dangerous {
                    dangerous_flag
                } else {
                    ""
                },
            );
        if !rendered.is_empty() {
            args.push(rendered);
        }
    }
    Ok(args)
}

fn read_capped(reader: Option<impl Read>, cap: usize) -> (String, bool) {
    let Some(mut reader) = reader else {
        return (String::new(), false);
    };
    let mut output = Vec::new();
    let mut truncated = false;
    let mut buffer = [0_u8; 8192];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                let remaining = cap.saturating_sub(output.len());
                if remaining == 0 {
                    truncated = true;
                    continue;
                }
                let take = remaining.min(n);
                output.extend_from_slice(&buffer[..take]);
                if take < n {
                    truncated = true;
                }
            }
            Err(_) => break,
        }
    }
    (String::from_utf8_lossy(&output).to_string(), truncated)
}

fn terminate_process_group(pgid: i32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-pgid, libc::SIGTERM);
    }
}

fn wait_then_kill(pgid: i32, grace: Duration) {
    thread::sleep(grace);
    kill_process_group(pgid);
}

fn kill_process_group(pgid: i32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::thread;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::config::{RunnerKind, StdinMode};

    #[test]
    fn expands_runner_args_and_omits_inactive_dangerous_flag() {
        let runner = RunnerConfig {
            id: "cursor".to_string(),
            label: "Cursor".to_string(),
            command: "cursor-agent".to_string(),
            kind: RunnerKind::Cursor,
            args: vec![
                "--print".to_string(),
                "{{dangerous_flag}}".to_string(),
                "--model".to_string(),
                "{{model}}".to_string(),
                "{{prompt}}".to_string(),
            ],
            dangerous_flag: Some("--force".to_string()),
            default_model: Some("composer-2.5".to_string()),
            default_effort: None,
            model_options: vec![],
            effort_options: vec![],
            stdin: StdinMode::Null,
            default_timeout_seconds: None,
        };
        let mut routine = RoutineConfig {
            id: Some("rtn_a".to_string()),
            title: "Routine".to_string(),
            description: String::new(),
            prompt: "Do it.".to_string(),
            runner: "cursor".to_string(),
            model: None,
            effort: None,
            cwd: PathBuf::from("/tmp"),
            schedule: "0 7 * * Sat".to_string(),
            timezone: None,
            paused: false,
            dangerous: false,
            timeout_seconds: None,
        };

        assert_eq!(
            expand_args(&runner, &routine).unwrap(),
            vec!["--print", "--model", "composer-2.5", "Do it."]
        );

        routine.dangerous = true;
        assert_eq!(
            expand_args(&runner, &routine).unwrap(),
            vec!["--print", "--force", "--model", "composer-2.5", "Do it."]
        );
    }

    #[test]
    fn starts_process_and_stores_separate_output_streams() {
        let store = Arc::new(RunStore::in_memory().unwrap());
        let manager = ProcessManager::default();
        let runner = RunnerConfig {
            id: "shell".to_string(),
            label: "Shell".to_string(),
            command: "/bin/sh".to_string(),
            kind: RunnerKind::Custom,
            args: vec!["-c".to_string(), "printf out; printf err >&2".to_string()],
            dangerous_flag: None,
            default_model: Some("none".to_string()),
            default_effort: None,
            model_options: vec![],
            effort_options: vec![],
            stdin: StdinMode::Null,
            default_timeout_seconds: Some(5),
        };
        let routine = RoutineConfig {
            id: Some("rtn_process".to_string()),
            title: "Process".to_string(),
            description: String::new(),
            prompt: "ignored".to_string(),
            runner: "shell".to_string(),
            model: Some("none".to_string()),
            effort: None,
            cwd: PathBuf::from("/tmp"),
            schedule: "0 7 * * Sat".to_string(),
            timezone: None,
            paused: false,
            dangerous: false,
            timeout_seconds: Some(5),
        };

        let queued = manager
            .start_run(store.clone(), Settings::default(), runner, routine, None)
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        let finished = loop {
            let run = store.get_run(&queued.id).unwrap().unwrap();
            if !matches!(run.status, RunStatus::Queued | RunStatus::Running) {
                break run;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(50));
        };

        assert_eq!(finished.status, RunStatus::Succeeded);
        assert_eq!(finished.stdout, "out");
        assert_eq!(finished.stderr, "err");
    }

    #[test]
    fn finishing_superseded_run_does_not_untrack_newer_run() {
        let manager = ProcessManager::default();
        let routine_id = "rtn_same".to_string();
        let old = ActiveRun {
            run_id: "run_old".to_string(),
            pgid: Arc::new(Mutex::new(None)),
            cancel_reason: Arc::new(Mutex::new(None)),
        };
        let new = ActiveRun {
            run_id: "run_new".to_string(),
            pgid: Arc::new(Mutex::new(None)),
            cancel_reason: Arc::new(Mutex::new(None)),
        };
        manager
            .active
            .lock()
            .expect("active lock poisoned")
            .insert(routine_id.clone(), old);
        manager
            .active
            .lock()
            .expect("active lock poisoned")
            .insert(routine_id.clone(), new);

        manager.remove_active(&routine_id, "run_old");

        let active = manager.active.lock().expect("active lock poisoned");
        assert_eq!(
            active.get(&routine_id).map(|active| active.run_id.as_str()),
            Some("run_new")
        );
        drop(active);

        manager.remove_active(&routine_id, "run_new");

        assert!(!manager.has_active_runs());
    }
}
