use std::collections::HashMap;
use std::env;
use std::io::Read;
use std::path::{Path, PathBuf};
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
    #[error("routine `{0}` is already running")]
    AlreadyRunning(String),
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
    kill_deadline: Arc<Mutex<Option<Instant>>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OverlapPolicy {
    Replace,
    Reject,
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
        self.start_run_with_policy(
            store,
            settings,
            runner,
            routine,
            scheduled_for,
            OverlapPolicy::Replace,
        )
    }

    pub fn start_manual_run(
        &self,
        store: Arc<RunStore>,
        settings: Settings,
        runner: RunnerConfig,
        routine: RoutineConfig,
    ) -> Result<RunRecord, ProcessError> {
        self.start_run_with_policy(
            store,
            settings,
            runner,
            routine,
            None,
            OverlapPolicy::Reject,
        )
    }

    fn start_run_with_policy(
        &self,
        store: Arc<RunStore>,
        settings: Settings,
        runner: RunnerConfig,
        routine: RoutineConfig,
        scheduled_for: Option<chrono::DateTime<Utc>>,
        overlap_policy: OverlapPolicy,
    ) -> Result<RunRecord, ProcessError> {
        let routine_id = routine
            .id
            .clone()
            .ok_or_else(|| ProcessError::MissingRoutineId(routine.title.clone()))?;

        let argv = expand_args(&runner, &routine)?;
        let command_for_history = std::iter::once(runner.command.clone())
            .chain(argv.iter().cloned())
            .collect::<Vec<_>>();

        let run_id = RunStore::new_run_id();
        let active = ActiveRun {
            run_id: run_id.clone(),
            pgid: Arc::new(Mutex::new(None)),
            cancel_reason: Arc::new(Mutex::new(None)),
            kill_deadline: Arc::new(Mutex::new(None)),
        };
        let replaced = {
            let mut active_runs = self.active.lock().expect("active lock poisoned");
            let replaced = if let Some(active) = active_runs.get(&routine_id).cloned() {
                match overlap_policy {
                    OverlapPolicy::Reject => return Err(ProcessError::AlreadyRunning(routine_id)),
                    OverlapPolicy::Replace => Some(active),
                }
            } else {
                None
            };
            active_runs.insert(routine_id.clone(), active.clone());
            replaced
        };
        if let Some(replaced) = replaced {
            request_cancel(&replaced, CancelReason::Superseded);
        }

        let queued = match store.create_run_with_id(
            run_id,
            NewRun {
                routine_id: routine_id.clone(),
                routine_title: routine.title.clone(),
                status: RunStatus::Queued,
                scheduled_for,
                command: command_for_history,
                cwd: routine.cwd.display().to_string(),
                cancel_reason: None,
            },
        ) {
            Ok(queued) => queued,
            Err(err) => {
                self.remove_active(&routine_id, &active.run_id);
                return Err(err.into());
            }
        };

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
        request_cancel(&active, reason);
        Some(active.run_id)
    }

    pub fn has_active_routine(&self, routine_id: &str) -> bool {
        self.active
            .lock()
            .expect("active lock poisoned")
            .contains_key(routine_id)
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
            request_cancel(&active, reason.clone());
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
        if let Err(error) = store.mark_running(&active.run_id, started_at) {
            eprintln!(
                "AI Scheduler could not mark run {} as running: {error}",
                active.run_id
            );
            if let Some(routine_id) = routine.id.as_deref() {
                self.remove_active(routine_id, &active.run_id);
            }
            return;
        }

        let mut command = Command::new(&runner.command);
        command.args(&argv).current_dir(&routine.cwd);
        apply_ssh_agent_env(&mut command);
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
                if let Err(store_error) = store.finish_run(
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
                ) {
                    eprintln!(
                        "AI Scheduler could not record spawn failure for run {}: {store_error}",
                        active.run_id
                    );
                }
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
            begin_termination(&active, pgid, Duration::from_secs(5));
        }

        let persistence_error = Arc::new(Mutex::new(None::<String>));
        let stdout = child.stdout.take();
        let stderr = child.stderr.take();
        let stdout_store = store.clone();
        let stdout_run_id = active.run_id.clone();
        let stdout_persistence_error = persistence_error.clone();
        let stdout_reader = thread::spawn(move || {
            read_capped(stdout, stream_cap, |text, truncated| {
                if let Err(error) = stdout_store.update_stdout(&stdout_run_id, text, truncated) {
                    record_first_error(&stdout_persistence_error, error.to_string());
                }
            })
        });
        let stderr_store = store.clone();
        let stderr_run_id = active.run_id.clone();
        let stderr_persistence_error = persistence_error.clone();
        let stderr_reader = thread::spawn(move || {
            read_capped(stderr, stream_cap, |text, truncated| {
                if let Err(error) = stderr_store.update_stderr(&stderr_run_id, text, truncated) {
                    record_first_error(&stderr_persistence_error, error.to_string());
                }
            })
        });

        let start = Instant::now();
        let exit_status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None)
                    if start.elapsed() >= timeout
                        && active
                            .cancel_reason
                            .lock()
                            .expect("cancel lock poisoned")
                            .is_none() =>
                {
                    *active.cancel_reason.lock().expect("cancel lock poisoned") =
                        Some(CancelReason::Timeout);
                    begin_termination(&active, pgid, Duration::from_secs(5));
                }
                Ok(None) => {
                    maybe_kill_after_grace(&active, pgid);
                    thread::sleep(Duration::from_millis(200));
                }
                Err(error) => {
                    record_first_error(
                        &persistence_error,
                        format!("failed to poll child process: {error}"),
                    );
                    kill_process_group(pgid);
                    let _ = child.kill();
                    break None;
                }
            }
        };

        let final_status = if exit_status.is_none() {
            child.wait().ok()
        } else {
            exit_status
        };

        let (stdout, stdout_truncated, stdout_error) = stdout_reader.join().unwrap_or_else(|_| {
            (
                "".to_string(),
                false,
                Some("stdout reader panicked".to_string()),
            )
        });
        let (mut stderr, stderr_truncated, stderr_error) =
            stderr_reader.join().unwrap_or_else(|_| {
                (
                    "".to_string(),
                    false,
                    Some("stderr reader panicked".to_string()),
                )
            });
        for error in [stdout_error, stderr_error].into_iter().flatten() {
            record_first_error(&persistence_error, error);
        }
        let process_error = persistence_error
            .lock()
            .expect("persistence error lock poisoned")
            .clone();
        if let Some(error) = &process_error {
            if !stderr.is_empty() {
                stderr.push('\n');
            }
            stderr.push_str("AI Scheduler internal error: ");
            stderr.push_str(error);
        }
        let cancel_reason = active
            .cancel_reason
            .lock()
            .expect("cancel lock poisoned")
            .clone();

        let status = match cancel_reason {
            Some(CancelReason::Timeout) => RunStatus::TimedOut,
            Some(CancelReason::Superseded) => RunStatus::Superseded,
            Some(CancelReason::User | CancelReason::AppClosed) => RunStatus::Cancelled,
            None if process_error.is_some() => RunStatus::Failed,
            None => match final_status {
                Some(status) if status.success() => RunStatus::Succeeded,
                _ => RunStatus::Failed,
            },
        };

        if let Err(error) = store.finish_run(
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
        ) {
            eprintln!(
                "AI Scheduler could not finish run {}: {error}",
                active.run_id
            );
        }
        if let Err(error) = store.prune(settings.max_runs_per_routine, settings.max_run_age_days) {
            eprintln!("AI Scheduler could not prune run history: {error}");
        }

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
        .unwrap_or("");
    if runner.uses_model() && model.is_empty() {
        return Err(ProcessError::MissingModel(runner.id.clone()));
    }
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

fn read_capped(
    reader: Option<impl Read>,
    cap: usize,
    mut on_update: impl FnMut(&str, bool),
) -> (String, bool, Option<String>) {
    let Some(mut reader) = reader else {
        return (String::new(), false, None);
    };
    let mut output = Vec::new();
    let mut truncated = false;
    let mut last_update_len = 0;
    let mut last_update_truncated = false;
    let mut last_update_at: Option<Instant> = None;
    let update_interval = Duration::from_millis(250);
    let update_byte_step = 64 * 1024;
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
                let now = Instant::now();
                let first_update = last_update_at.is_none();
                let enough_bytes = output.len().saturating_sub(last_update_len) >= update_byte_step;
                let enough_time =
                    last_update_at.is_some_and(|last| now.duration_since(last) >= update_interval);
                let truncation_changed = truncated != last_update_truncated;
                if first_update || enough_bytes || enough_time || truncation_changed {
                    let text = String::from_utf8_lossy(&output).to_string();
                    on_update(&text, truncated);
                    last_update_len = output.len();
                    last_update_truncated = truncated;
                    last_update_at = Some(now);
                }
            }
            Err(error) => {
                return (
                    String::from_utf8_lossy(&output).to_string(),
                    truncated,
                    Some(format!("failed to read process output: {error}")),
                );
            }
        }
    }
    (
        String::from_utf8_lossy(&output).to_string(),
        truncated,
        None,
    )
}

fn record_first_error(target: &Mutex<Option<String>>, error: String) {
    let mut current = target.lock().expect("persistence error lock poisoned");
    if current.is_none() {
        *current = Some(error);
    }
}

fn request_cancel(active: &ActiveRun, reason: CancelReason) {
    *active.cancel_reason.lock().expect("cancel lock poisoned") = Some(reason);
    if let Some(pgid) = *active.pgid.lock().expect("pgid lock poisoned") {
        begin_termination(active, pgid, Duration::from_secs(5));
    }
}

fn begin_termination(active: &ActiveRun, pgid: i32, grace: Duration) {
    terminate_process_group(pgid);
    *active
        .kill_deadline
        .lock()
        .expect("kill deadline lock poisoned") = Some(Instant::now() + grace);
}

fn maybe_kill_after_grace(active: &ActiveRun, pgid: i32) {
    let mut deadline = active
        .kill_deadline
        .lock()
        .expect("kill deadline lock poisoned");
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        kill_process_group(pgid);
        *deadline = None;
    }
}

fn terminate_process_group(pgid: i32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-pgid, libc::SIGTERM);
    }
}

fn kill_process_group(pgid: i32) {
    #[cfg(unix)]
    unsafe {
        libc::kill(-pgid, libc::SIGKILL);
    }
}

/// Desktop-launched apps often lack `SSH_AUTH_SOCK` even when a user agent is
/// running. Forward a live agent socket into child processes so git/SSH work.
fn apply_ssh_agent_env(command: &mut Command) {
    if let Some(sock) = resolve_ssh_auth_sock() {
        command.env("SSH_AUTH_SOCK", sock);
    }
}

fn resolve_ssh_auth_sock() -> Option<PathBuf> {
    if let Ok(existing) = env::var("SSH_AUTH_SOCK") {
        let path = PathBuf::from(&existing);
        if is_usable_ssh_sock(&path) {
            return Some(path);
        }
    }

    let runtime_dir = env::var_os("XDG_RUNTIME_DIR").map(PathBuf::from)?;
    [
        runtime_dir.join("ssh-agent.socket"),
        runtime_dir.join("keyring/ssh"),
    ]
    .into_iter()
    .find(|candidate| is_usable_ssh_sock(candidate))
}

fn is_usable_ssh_sock(path: &Path) -> bool {
    path.exists()
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
    fn expands_script_args_without_model() {
        let runner = RunnerConfig {
            id: "script".to_string(),
            label: "Script".to_string(),
            command: "bash".to_string(),
            kind: RunnerKind::Script,
            args: vec!["-lc".to_string(), "{{prompt}}".to_string()],
            dangerous_flag: None,
            default_model: None,
            default_effort: None,
            model_options: vec![],
            effort_options: vec![],
            stdin: StdinMode::Null,
            default_timeout_seconds: None,
        };
        let routine = RoutineConfig {
            id: Some("rtn_script".to_string()),
            title: "Script".to_string(),
            description: String::new(),
            prompt: "echo hello".to_string(),
            runner: "script".to_string(),
            model: None,
            effort: None,
            cwd: PathBuf::from("/tmp"),
            schedule: "0 7 * * *".to_string(),
            timezone: None,
            paused: false,
            dangerous: false,
            timeout_seconds: None,
        };

        assert_eq!(
            expand_args(&runner, &routine).unwrap(),
            vec!["-lc", "echo hello"]
        );
    }

    #[test]
    fn resolves_ssh_auth_sock_from_xdg_runtime_dir() {
        let runtime = tempfile::tempdir().unwrap();
        let sock = runtime.path().join("ssh-agent.socket");
        std::os::unix::net::UnixListener::bind(&sock).unwrap();

        let previous_sock = env::var_os("SSH_AUTH_SOCK");
        let previous_runtime = env::var_os("XDG_RUNTIME_DIR");
        env::remove_var("SSH_AUTH_SOCK");
        env::set_var("XDG_RUNTIME_DIR", runtime.path());

        let resolved = resolve_ssh_auth_sock();

        match previous_sock {
            Some(value) => env::set_var("SSH_AUTH_SOCK", value),
            None => env::remove_var("SSH_AUTH_SOCK"),
        }
        match previous_runtime {
            Some(value) => env::set_var("XDG_RUNTIME_DIR", value),
            None => env::remove_var("XDG_RUNTIME_DIR"),
        }

        assert_eq!(resolved.as_deref(), Some(sock.as_path()));
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
    fn stores_stdout_while_process_is_running() {
        let store = Arc::new(RunStore::in_memory().unwrap());
        let manager = ProcessManager::default();
        let runner = RunnerConfig {
            id: "shell".to_string(),
            label: "Shell".to_string(),
            command: "/bin/sh".to_string(),
            kind: RunnerKind::Custom,
            args: vec![
                "-c".to_string(),
                "printf start; sleep 1; printf end".to_string(),
            ],
            dangerous_flag: None,
            default_model: Some("none".to_string()),
            default_effort: None,
            model_options: vec![],
            effort_options: vec![],
            stdin: StdinMode::Null,
            default_timeout_seconds: Some(5),
        };
        let routine = RoutineConfig {
            id: Some("rtn_live".to_string()),
            title: "Live".to_string(),
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
        let deadline = Instant::now() + Duration::from_secs(3);
        let partial = loop {
            let run = store.get_run(&queued.id).unwrap().unwrap();
            if run.stdout == "start" {
                break run;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(25));
        };

        assert_eq!(partial.status, RunStatus::Running);
        assert!(manager.has_active_routine("rtn_live"));

        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let run = store.get_run(&queued.id).unwrap().unwrap();
            if !matches!(run.status, RunStatus::Queued | RunStatus::Running) {
                assert_eq!(run.stdout, "startend");
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn manual_run_rejects_existing_active_run() {
        let store = Arc::new(RunStore::in_memory().unwrap());
        let manager = ProcessManager::default();
        let runner = RunnerConfig {
            id: "shell".to_string(),
            label: "Shell".to_string(),
            command: "/bin/sh".to_string(),
            kind: RunnerKind::Custom,
            args: vec!["-c".to_string(), "sleep 1".to_string()],
            dangerous_flag: None,
            default_model: Some("none".to_string()),
            default_effort: None,
            model_options: vec![],
            effort_options: vec![],
            stdin: StdinMode::Null,
            default_timeout_seconds: Some(5),
        };
        let routine = RoutineConfig {
            id: Some("rtn_manual".to_string()),
            title: "Manual".to_string(),
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
            .start_manual_run(
                store.clone(),
                Settings::default(),
                runner.clone(),
                routine.clone(),
            )
            .unwrap();
        let err = manager
            .start_manual_run(store.clone(), Settings::default(), runner, routine)
            .unwrap_err()
            .to_string();

        assert!(err.contains("already running"));
        assert_eq!(store.list_runs_for_routine("rtn_manual").unwrap().len(), 1);

        manager.cancel_routine("rtn_manual", CancelReason::User);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let run = store.get_run(&queued.id).unwrap().unwrap();
            if !matches!(run.status, RunStatus::Queued | RunStatus::Running) {
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn finishing_superseded_run_does_not_untrack_newer_run() {
        let manager = ProcessManager::default();
        let routine_id = "rtn_same".to_string();
        let old = ActiveRun {
            run_id: "run_old".to_string(),
            pgid: Arc::new(Mutex::new(None)),
            cancel_reason: Arc::new(Mutex::new(None)),
            kill_deadline: Arc::new(Mutex::new(None)),
        };
        let new = ActiveRun {
            run_id: "run_new".to_string(),
            pgid: Arc::new(Mutex::new(None)),
            cancel_reason: Arc::new(Mutex::new(None)),
            kill_deadline: Arc::new(Mutex::new(None)),
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
