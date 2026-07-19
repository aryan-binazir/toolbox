pub mod config;
pub mod mobile;
pub mod paths;
pub mod process;
pub mod runners;
pub mod scheduler;
pub mod store;

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use config::{
    builtin_default_config, canonical_toml, load_config, load_raw_config_preserving_text,
    normalize_config, save_config, save_config_text, AppConfig, RoutineConfig,
};
use paths::AppPaths;
use runners::{configured_runner_capabilities, probe_runners, RunnerCapability};
use scheduler::{RoutineScheduleInfo, SchedulePreview};
use serde::Serialize;
use store::{RunRecord, RunStore};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Config(#[from] config::ConfigError),
    #[error(transparent)]
    Store(#[from] store::StoreError),
    #[error(transparent)]
    Process(#[from] process::ProcessError),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Message(String),
}

impl Serialize for AppError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

#[derive(Clone)]
pub struct AppState {
    paths: AppPaths,
    store: Arc<RunStore>,
    config: Arc<Mutex<AppConfig>>,
    runner_capabilities: Arc<Mutex<Vec<RunnerCapability>>>,
    process_manager: process::ProcessManager,
    mobile_server: Arc<Mutex<Option<mobile::MobileServerHandle>>>,
}

#[derive(Debug, Clone, Serialize)]
pub struct AppSnapshot {
    pub config_path: PathBuf,
    pub db_path: PathBuf,
    pub config: AppConfig,
    pub runner_capabilities: Vec<RunnerCapability>,
    pub scheduler_last_checked: Option<DateTime<Utc>>,
    pub routine_schedules: Vec<RoutineScheduleInfo>,
}

impl AppState {
    pub fn bootstrap(paths: AppPaths) -> Result<Self, AppError> {
        ensure_config_exists(&paths.config_file)?;
        let loaded = load_config(&paths.config_file)?;
        if loaded.changed {
            save_config(&paths.config_file, &loaded.config)?;
        }
        let store = Arc::new(RunStore::open(&paths.db_file)?);
        store.cancel_active_runs_on_startup()?;
        store.prune(
            loaded.config.settings.max_runs_per_routine,
            loaded.config.settings.max_run_age_days,
        )?;
        let runner_capabilities = configured_runner_capabilities(&loaded.config.runners);
        Ok(Self {
            paths,
            store,
            config: Arc::new(Mutex::new(loaded.config)),
            runner_capabilities: Arc::new(Mutex::new(runner_capabilities)),
            process_manager: process::ProcessManager::default(),
            mobile_server: Arc::new(Mutex::new(None)),
        })
    }

    pub fn config(&self) -> AppConfig {
        self.config.lock().expect("config lock poisoned").clone()
    }

    pub fn replace_config(&self, config: AppConfig) -> Result<(), AppError> {
        save_config(&self.paths.config_file, &config)?;
        self.set_config(config);
        Ok(())
    }

    pub fn store(&self) -> Arc<RunStore> {
        self.store.clone()
    }

    pub fn process_manager(&self) -> process::ProcessManager {
        self.process_manager.clone()
    }

    pub fn runner_capabilities(&self) -> Vec<RunnerCapability> {
        self.runner_capabilities
            .lock()
            .expect("runner capability lock poisoned")
            .clone()
    }

    pub fn refresh_runner_capabilities(&self) -> Vec<RunnerCapability> {
        let config = self.config();
        let capabilities = probe_runners(&config.runners);
        *self
            .runner_capabilities
            .lock()
            .expect("runner capability lock poisoned") = capabilities.clone();
        capabilities
    }

    pub fn snapshot(&self) -> Result<AppSnapshot, AppError> {
        let config = self.config();
        let scheduler_last_checked = self.store.scheduler_last_checked()?;
        let routine_schedules = scheduler::routine_schedule_infos(&config, Utc::now());
        Ok(AppSnapshot {
            config_path: self.paths.config_file.clone(),
            db_path: self.paths.db_file.clone(),
            config,
            runner_capabilities: self.runner_capabilities(),
            scheduler_last_checked,
            routine_schedules,
        })
    }

    pub fn raw_config(&self) -> Result<String, AppError> {
        Ok(fs::read_to_string(&self.paths.config_file)?)
    }

    pub fn save_raw_config(&self, raw: String) -> Result<AppConfig, AppError> {
        let config = load_raw_config_preserving_text(&raw)?;
        save_config_text(&self.paths.config_file, &raw)?;
        self.set_config(config.clone());
        Ok(config)
    }

    pub fn preview_schedule(&self, routine: &RoutineConfig) -> SchedulePreview {
        scheduler::preview_schedule(&self.config(), routine, Utc::now())
    }

    pub fn list_runs(&self, routine_id: &str) -> Result<Vec<RunRecord>, AppError> {
        self.store
            .list_runs_for_routine(routine_id)
            .map_err(Into::into)
    }

    pub fn save_routine(&self, routine: RoutineConfig) -> Result<AppConfig, AppError> {
        let mut config = self.config();
        let incoming = routine;
        let incoming_id = incoming.id.clone();
        if let Some(id) = incoming_id.as_deref() {
            if let Some(existing) = config
                .routines
                .iter_mut()
                .find(|routine| routine.id.as_deref() == Some(id))
            {
                *existing = incoming;
            } else {
                config.routines.push(incoming);
            }
        } else {
            config.routines.push(incoming);
        }
        normalize_config(&mut config);
        self.replace_config(config.clone())?;
        Ok(config)
    }

    pub fn set_routine_paused(
        &self,
        routine_id: &str,
        paused: bool,
    ) -> Result<AppConfig, AppError> {
        let mut config = self.config();
        let routine = config
            .routines
            .iter_mut()
            .find(|routine| routine.id.as_deref() == Some(routine_id))
            .ok_or_else(|| AppError::Message(format!("routine `{routine_id}` not found")))?;
        routine.paused = paused;
        self.replace_config(config.clone())?;
        Ok(config)
    }

    pub fn delete_routine(&self, routine_id: &str) -> Result<AppConfig, AppError> {
        let mut config = self.config();
        let existed = config
            .routines
            .iter()
            .any(|routine| routine.id.as_deref() == Some(routine_id));
        config
            .routines
            .retain(|routine| routine.id.as_deref() != Some(routine_id));
        self.replace_config(config.clone())?;
        if existed {
            self.process_manager()
                .cancel_routine(routine_id, process::CancelReason::User);
            self.store.delete_runs_for_routine(routine_id)?;
        }
        Ok(config)
    }

    pub fn run_routine(&self, routine_id: &str) -> Result<RunRecord, AppError> {
        let config = self.config();
        let routine = config
            .routines
            .iter()
            .find(|routine| routine.id.as_deref() == Some(routine_id))
            .cloned()
            .ok_or_else(|| AppError::Message(format!("routine `{routine_id}` not found")))?;
        let runner = config
            .runners
            .iter()
            .find(|runner| runner.id == routine.runner)
            .cloned()
            .ok_or_else(|| AppError::Message(format!("runner `{}` not found", routine.runner)))?;
        self.process_manager()
            .start_manual_run(self.store(), config.settings.clone(), runner, routine)
            .map_err(Into::into)
    }

    pub fn cancel_routine(&self, routine_id: &str) -> Option<String> {
        self.process_manager()
            .cancel_routine(routine_id, process::CancelReason::User)
    }

    pub fn reconcile_mobile_server(&self) {
        let settings = self.config().settings;
        let mut current = self.mobile_server.lock().expect("mobile lock poisoned");
        if !settings.mobile_web_enabled {
            if let Some(handle) = current.take() {
                handle.stop();
                eprintln!("AI Scheduler mobile web disabled");
            }
            return;
        }

        if current
            .as_ref()
            .is_some_and(|handle| handle.port() == settings.mobile_web_port)
        {
            return;
        }

        if let Some(handle) = current.take() {
            handle.stop();
        }
        match mobile::start_mobile_server(self.clone(), settings.mobile_web_port) {
            Ok(handle) => *current = Some(handle),
            Err(error) => eprintln!("AI Scheduler mobile web not started: {error}"),
        }
    }

    fn set_config(&self, config: AppConfig) {
        let mut current = self.config.lock().expect("config lock poisoned");
        let runners_changed = current.runners != config.runners;
        *current = config.clone();
        drop(current);
        if runners_changed {
            *self
                .runner_capabilities
                .lock()
                .expect("runner capability lock poisoned") =
                configured_runner_capabilities(&config.runners);
        }
        self.reconcile_mobile_server();
    }
}

fn ensure_config_exists(path: &PathBuf) -> Result<(), AppError> {
    if path.exists() {
        return Ok(());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let config = builtin_default_config();
    fs::write(path, canonical_toml(&config)?)?;
    Ok(())
}

#[tauri::command]
async fn get_snapshot(state: tauri::State<'_, AppState>) -> Result<AppSnapshot, AppError> {
    state.snapshot()
}

#[tauri::command]
async fn refresh_runner_capabilities(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<RunnerCapability>, AppError> {
    Ok(state.refresh_runner_capabilities())
}

#[tauri::command]
async fn get_raw_config(state: tauri::State<'_, AppState>) -> Result<String, AppError> {
    state.raw_config()
}

#[tauri::command]
async fn save_raw_config(
    state: tauri::State<'_, AppState>,
    raw: String,
) -> Result<AppConfig, AppError> {
    state.save_raw_config(raw)
}

#[tauri::command]
async fn preview_schedule(
    state: tauri::State<'_, AppState>,
    routine: RoutineConfig,
) -> Result<SchedulePreview, AppError> {
    Ok(state.preview_schedule(&routine))
}

#[tauri::command]
async fn choose_working_directory(initial: Option<String>) -> Result<Option<PathBuf>, AppError> {
    let mut dialog = rfd::FileDialog::new();
    if let Some(initial) = initial {
        let path = PathBuf::from(initial);
        if path.is_dir() {
            dialog = dialog.set_directory(path);
        }
    }
    Ok(dialog.pick_folder())
}

#[tauri::command]
async fn list_runs(
    state: tauri::State<'_, AppState>,
    routine_id: String,
) -> Result<Vec<RunRecord>, AppError> {
    state.list_runs(&routine_id)
}

#[tauri::command]
async fn save_routine(
    state: tauri::State<'_, AppState>,
    routine: RoutineConfig,
) -> Result<AppConfig, AppError> {
    state.save_routine(routine)
}

#[tauri::command]
async fn set_routine_paused(
    state: tauri::State<'_, AppState>,
    routine_id: String,
    paused: bool,
) -> Result<AppConfig, AppError> {
    state.set_routine_paused(&routine_id, paused)
}

#[tauri::command]
async fn delete_routine(
    state: tauri::State<'_, AppState>,
    routine_id: String,
) -> Result<AppConfig, AppError> {
    state.delete_routine(&routine_id)
}

#[tauri::command]
async fn run_routine(
    state: tauri::State<'_, AppState>,
    routine_id: String,
) -> Result<RunRecord, AppError> {
    state.run_routine(&routine_id)
}

#[tauri::command]
async fn cancel_routine(
    state: tauri::State<'_, AppState>,
    routine_id: String,
) -> Result<Option<String>, AppError> {
    Ok(state.cancel_routine(&routine_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_paths(temp: &tempfile::TempDir) -> AppPaths {
        AppPaths {
            config_file: temp.path().join("config.toml"),
            data_dir: temp.path().join("data"),
            db_file: temp.path().join("runs.db"),
            state_dir: temp.path().join("state"),
        }
    }

    fn available_port() -> u16 {
        std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    #[test]
    fn mobile_server_is_disabled_by_default() {
        let temp = tempfile::tempdir().unwrap();
        let state = AppState::bootstrap(test_paths(&temp)).unwrap();

        state.reconcile_mobile_server();

        assert!(state.mobile_server.lock().unwrap().is_none());
    }

    #[test]
    fn disabling_mobile_web_config_clears_server_handle() {
        let temp = tempfile::tempdir().unwrap();
        let state = AppState::bootstrap(test_paths(&temp)).unwrap();
        let mut config = state.config();
        config.settings.mobile_web_enabled = true;
        config.settings.mobile_web_port = available_port();
        state.set_config(config);

        assert!(state.mobile_server.lock().unwrap().is_some());

        let mut config = state.config();
        config.settings.mobile_web_enabled = false;
        state.set_config(config);

        assert!(state.mobile_server.lock().unwrap().is_none());
    }
}

pub fn run() {
    let paths = AppPaths::discover();
    let state = AppState::bootstrap(paths).expect("failed to bootstrap app state");
    state.reconcile_mobile_server();
    let setup_state = state.clone();
    let close_process_manager = state.process_manager();

    tauri::Builder::default()
        .manage(state.clone())
        .setup(move |app| {
            let app_handle = app.handle().clone();
            let scheduler_state = setup_state.clone();
            let probe_state = setup_state.clone();
            thread::spawn(move || {
                probe_state.refresh_runner_capabilities();
            });
            scheduler::start_scheduler(scheduler_state, app_handle);
            Ok(())
        })
        .on_window_event(move |_window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                close_process_manager.cancel_all_and_wait(
                    process::CancelReason::AppClosed,
                    Duration::from_secs(5),
                    Duration::from_secs(10),
                );
            }
        })
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            refresh_runner_capabilities,
            get_raw_config,
            save_raw_config,
            preview_schedule,
            choose_working_directory,
            list_runs,
            save_routine,
            set_routine_paused,
            delete_routine,
            run_routine,
            cancel_routine
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
