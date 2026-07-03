use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::{env, fs};

use chrono::Utc;
use chrono_tz::Tz;
use cron::Schedule as CronSchedule;
use nanoid::nanoid;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const DEFAULT_MAX_RUNS_PER_ROUTINE: u32 = 25;
const DEFAULT_MAX_RUN_AGE_DAYS: u32 = 90;
const DEFAULT_TIMEOUT_SECONDS: u64 = 1_800;
const DEFAULT_STREAM_CAP_BYTES: u64 = 5 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("failed to read config {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("failed to write config {path}: {source}")]
    Write {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("config validation failed: {0}")]
    Validation(String),
    #[error("failed to serialize config: {0}")]
    Serialize(#[from] toml::ser::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AppConfig {
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub runners: Vec<RunnerConfig>,
    #[serde(default)]
    pub routines: Vec<RoutineConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Settings {
    #[serde(default = "default_timezone")]
    pub timezone: String,
    #[serde(default = "default_max_runs_per_routine")]
    pub max_runs_per_routine: u32,
    #[serde(default = "default_max_run_age_days")]
    pub max_run_age_days: u32,
    #[serde(default = "default_timeout_seconds")]
    pub default_timeout_seconds: u64,
    #[serde(default = "default_stream_cap_bytes")]
    pub stream_cap_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RunnerConfig {
    pub id: String,
    pub label: String,
    pub command: String,
    #[serde(default)]
    pub kind: RunnerKind,
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub dangerous_flag: Option<String>,
    #[serde(default)]
    pub default_model: Option<String>,
    #[serde(default)]
    pub default_effort: Option<String>,
    #[serde(default)]
    pub model_options: Vec<OptionValue>,
    #[serde(default)]
    pub effort_options: Vec<OptionValue>,
    #[serde(default = "default_stdin")]
    pub stdin: StdinMode,
    #[serde(default)]
    pub default_timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum RunnerKind {
    Codex,
    Claude,
    Cursor,
    #[default]
    Custom,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OptionValue {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum StdinMode {
    Null,
    Inherit,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RoutineConfig {
    #[serde(default)]
    pub id: Option<String>,
    pub title: String,
    #[serde(default)]
    pub description: String,
    pub prompt: String,
    pub runner: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    pub cwd: PathBuf,
    pub schedule: String,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub paused: bool,
    #[serde(default)]
    pub dangerous: bool,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedConfig {
    pub config: AppConfig,
    pub changed: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            timezone: default_timezone(),
            max_runs_per_routine: DEFAULT_MAX_RUNS_PER_ROUTINE,
            max_run_age_days: DEFAULT_MAX_RUN_AGE_DAYS,
            default_timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
            stream_cap_bytes: DEFAULT_STREAM_CAP_BYTES,
        }
    }
}

fn default_timezone() -> String {
    "UTC".to_string()
}

fn default_max_runs_per_routine() -> u32 {
    DEFAULT_MAX_RUNS_PER_ROUTINE
}

fn default_max_run_age_days() -> u32 {
    DEFAULT_MAX_RUN_AGE_DAYS
}

fn default_timeout_seconds() -> u64 {
    DEFAULT_TIMEOUT_SECONDS
}

fn default_stream_cap_bytes() -> u64 {
    DEFAULT_STREAM_CAP_BYTES
}

fn default_stdin() -> StdinMode {
    StdinMode::Null
}

pub fn load_config(path: impl AsRef<Path>) -> Result<LoadedConfig, ConfigError> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|source| ConfigError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    load_config_from_str(&text)
}

pub fn load_config_from_str(text: &str) -> Result<LoadedConfig, ConfigError> {
    let mut config: AppConfig = toml::from_str(text)?;
    let changed = normalize_config(&mut config);
    validate_config(&config)?;
    Ok(LoadedConfig { config, changed })
}

pub fn load_raw_config_preserving_text(text: &str) -> Result<AppConfig, ConfigError> {
    let mut config: AppConfig = toml::from_str(text)?;
    if normalize_routine_ids(&mut config) {
        return validation(
            "raw config save requires explicit, unique routine ids; use the form editor to generate ids before editing raw TOML",
        );
    }
    normalize_routine_cwds(&mut config);
    validate_config(&config)?;
    Ok(config)
}

pub fn save_config(path: impl AsRef<Path>, config: &AppConfig) -> Result<(), ConfigError> {
    validate_config(config)?;
    let text = toml::to_string_pretty(config)?;
    save_config_text(path, &text)
}

pub fn save_config_text(path: impl AsRef<Path>, text: &str) -> Result<(), ConfigError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| ConfigError::Write {
            path: parent.to_path_buf(),
            source,
        })?;
    }
    backup_existing_config(path)?;
    fs::write(path, text).map_err(|source| ConfigError::Write {
        path: path.to_path_buf(),
        source,
    })
}

pub fn normalize_config(config: &mut AppConfig) -> bool {
    normalize_routine_ids(config) | normalize_routine_cwds(config)
}

fn normalize_routine_ids(config: &mut AppConfig) -> bool {
    let mut changed = false;
    let mut seen_ids = HashSet::new();
    for routine in &mut config.routines {
        let regenerate = routine
            .id
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
            || routine
                .id
                .as_ref()
                .is_some_and(|id| seen_ids.contains(id.as_str()));
        if regenerate {
            let id = generate_routine_id(&seen_ids);
            routine.id = Some(id);
            changed = true;
        }
        if let Some(id) = &routine.id {
            seen_ids.insert(id.clone());
        }
    }
    changed
}

fn normalize_routine_cwds(config: &mut AppConfig) -> bool {
    let mut changed = false;
    for routine in &mut config.routines {
        if let Some(expanded) = expand_home_path(&routine.cwd) {
            if expanded != routine.cwd {
                routine.cwd = expanded;
                changed = true;
            }
        }
    }
    changed
}

fn expand_home_path(path: &Path) -> Option<PathBuf> {
    let value = path.to_str()?;
    let home = env::var_os("HOME").map(PathBuf::from)?;
    if value == "~" {
        return Some(home);
    }
    value.strip_prefix("~/").map(|suffix| home.join(suffix))
}

fn backup_existing_config(path: &Path) -> Result<(), ConfigError> {
    if !path.exists() {
        return Ok(());
    }
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return Ok(());
    };
    let timestamp = Utc::now().format("%Y%m%dT%H%M%S%9fZ");
    let backup_name = format!("{file_name}.bak-{timestamp}");
    let backup_path = path.with_file_name(backup_name);
    fs::copy(path, &backup_path).map_err(|source| ConfigError::Write {
        path: backup_path,
        source,
    })?;
    Ok(())
}

#[cfg(test)]
fn backup_files_for(path: &Path) -> Vec<PathBuf> {
    let Some(parent) = path.parent() else {
        return vec![];
    };
    let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
        return vec![];
    };
    let prefix = format!("{file_name}.bak-");
    let mut backups = fs::read_dir(parent)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|entry| {
            entry
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with(&prefix))
        })
        .collect::<Vec<_>>();
    backups.sort();
    backups
}

pub fn validate_config(config: &AppConfig) -> Result<(), ConfigError> {
    validate_timezone(&config.settings.timezone)?;
    if config.settings.max_runs_per_routine == 0 {
        return validation("settings.max_runs_per_routine must be greater than 0");
    }
    if config.settings.max_run_age_days == 0 {
        return validation("settings.max_run_age_days must be greater than 0");
    }
    if config.settings.default_timeout_seconds == 0 {
        return validation("settings.default_timeout_seconds must be greater than 0");
    }
    if config.settings.stream_cap_bytes == 0 {
        return validation("settings.stream_cap_bytes must be greater than 0");
    }

    let mut runner_ids = HashSet::new();
    for runner in &config.runners {
        if runner.id.trim().is_empty() {
            return validation("runner id must not be empty");
        }
        if !runner_ids.insert(runner.id.as_str()) {
            return validation(format!("duplicate runner id `{}`", runner.id));
        }
        if runner.label.trim().is_empty() {
            return validation(format!("runner `{}` label must not be empty", runner.id));
        }
        if runner.command.trim().is_empty() {
            return validation(format!("runner `{}` command must not be empty", runner.id));
        }
        if runner
            .default_timeout_seconds
            .is_some_and(|timeout| timeout == 0)
        {
            return validation(format!(
                "runner `{}` default_timeout_seconds must be greater than 0",
                runner.id
            ));
        }
    }

    let mut routine_ids = HashSet::new();
    for routine in &config.routines {
        let id = routine
            .id
            .as_deref()
            .ok_or_else(|| ConfigError::Validation("routine id was not generated".to_string()))?;
        if !routine_ids.insert(id) {
            return validation(format!("duplicate routine id `{id}`"));
        }
        if routine.title.trim().is_empty() {
            return validation(format!("routine `{id}` title must not be empty"));
        }
        if routine.prompt.trim().is_empty() {
            return validation(format!("routine `{id}` prompt must not be empty"));
        }
        if !runner_ids.contains(routine.runner.as_str()) {
            return validation(format!(
                "routine `{}` references unknown runner `{}`",
                id, routine.runner
            ));
        }
        if routine
            .model
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
        {
            return validation(format!("routine `{id}` model must not be empty"));
        }
        if !routine.cwd.is_dir() {
            return validation(format!(
                "routine `{}` cwd is not a directory: {}",
                id,
                routine.cwd.display()
            ));
        }
        validate_cron(&routine.schedule)
            .map_err(|message| ConfigError::Validation(format!("routine `{id}` {message}")))?;
        validate_timezone(
            routine
                .timezone
                .as_deref()
                .unwrap_or(config.settings.timezone.as_str()),
        )
        .map_err(|err| ConfigError::Validation(format!("routine `{id}` {err}")))?;
        if routine.timeout_seconds.is_some_and(|timeout| timeout == 0) {
            return validation(format!(
                "routine `{}` timeout_seconds must be greater than 0",
                id
            ));
        }
    }

    Ok(())
}

pub fn canonical_toml(config: &AppConfig) -> Result<String, ConfigError> {
    Ok(toml::to_string_pretty(config)?)
}

pub fn builtin_default_config() -> AppConfig {
    AppConfig {
        settings: Settings {
            timezone: "America/New_York".to_string(),
            ..Settings::default()
        },
        runners: vec![
            RunnerConfig {
                id: "codex".to_string(),
                label: "Codex".to_string(),
                command: "codex".to_string(),
                kind: RunnerKind::Codex,
                args: vec![
                    "exec".to_string(),
                    "{{dangerous_flag}}".to_string(),
                    "--model".to_string(),
                    "{{model}}".to_string(),
                    "-c".to_string(),
                    "model_reasoning_effort=\"{{effort}}\"".to_string(),
                    "{{prompt}}".to_string(),
                ],
                dangerous_flag: Some("--dangerously-bypass-approvals-and-sandbox".to_string()),
                default_model: Some("gpt-5.5".to_string()),
                default_effort: Some("xhigh".to_string()),
                model_options: vec![
                    option("gpt-5.5", "GPT-5.5"),
                    option("gpt-5.3-codex-spark", "GPT-5.3 Codex Spark"),
                ],
                effort_options: vec![
                    option("low", "Low"),
                    option("medium", "Medium"),
                    option("high", "High"),
                    option("xhigh", "Extra High"),
                ],
                stdin: StdinMode::Null,
                default_timeout_seconds: Some(DEFAULT_TIMEOUT_SECONDS),
            },
            RunnerConfig {
                id: "claude".to_string(),
                label: "Claude Code".to_string(),
                command: "claude".to_string(),
                kind: RunnerKind::Claude,
                args: vec![
                    "{{dangerous_flag}}".to_string(),
                    "-p".to_string(),
                    "--model".to_string(),
                    "{{model}}".to_string(),
                    "--effort".to_string(),
                    "{{effort}}".to_string(),
                    "{{prompt}}".to_string(),
                ],
                dangerous_flag: Some("--dangerously-skip-permissions".to_string()),
                default_model: Some("sonnet".to_string()),
                default_effort: Some("high".to_string()),
                model_options: vec![
                    option("sonnet", "Sonnet"),
                    option("opus", "Opus"),
                    option("fable", "Fable"),
                ],
                effort_options: vec![
                    option("low", "Low"),
                    option("medium", "Medium"),
                    option("high", "High"),
                    option("xhigh", "Extra High"),
                    option("max", "Max"),
                ],
                stdin: StdinMode::Null,
                default_timeout_seconds: Some(DEFAULT_TIMEOUT_SECONDS),
            },
            RunnerConfig {
                id: "cursor".to_string(),
                label: "Cursor Agent".to_string(),
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
                model_options: vec![
                    option("composer-2.5", "Composer 2.5"),
                    option("composer-2.5-fast", "Composer 2.5 Fast"),
                ],
                effort_options: vec![],
                stdin: StdinMode::Null,
                default_timeout_seconds: Some(DEFAULT_TIMEOUT_SECONDS),
            },
        ],
        routines: vec![],
    }
}

fn option(value: &str, label: &str) -> OptionValue {
    OptionValue {
        value: value.to_string(),
        label: label.to_string(),
    }
}

fn generate_routine_id(existing: &HashSet<String>) -> String {
    loop {
        let candidate = format!("rtn_{}", nanoid!(16));
        if !existing.contains(&candidate) {
            return candidate;
        }
    }
}

fn validate_timezone(value: &str) -> Result<(), ConfigError> {
    value
        .parse::<Tz>()
        .map(|_| ())
        .map_err(|_| ConfigError::Validation(format!("invalid timezone `{value}`")))
}

fn validate_cron(value: &str) -> Result<(), String> {
    normalize_cron(value)
        .parse::<CronSchedule>()
        .map(|_| ())
        .map_err(|err| format!("schedule is invalid: {err}"))
}

pub fn normalize_cron(value: &str) -> String {
    let fields = value.split_whitespace().count();
    if fields == 5 {
        format!("0 {value}")
    } else {
        value.to_string()
    }
}

fn validation<T>(message: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError::Validation(message.into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_config_generates_ids_and_validates_routines() {
        let cwd = tempfile::tempdir().unwrap();
        let text = format!(
            r#"
[settings]
timezone = "America/New_York"

[[runners]]
id = "codex"
label = "Codex"
command = "codex"
kind = "codex"
args = ["exec", "{{{{dangerous_flag}}}}", "--model", "{{{{model}}}}", "{{{{prompt}}}}"]
dangerous_flag = "--dangerously-bypass-approvals-and-sandbox"

[[routines]]
title = "Go server concept harvest"
description = "Weekly concept harvest"
prompt = "Do the work directly."
runner = "codex"
model = "gpt-5.5"
effort = "xhigh"
cwd = "{}"
schedule = "0 7 * * Sat"
"#,
            cwd.path().display()
        );

        let loaded = load_config_from_str(&text).unwrap();

        assert!(loaded.changed);
        assert_eq!(loaded.config.settings.max_runs_per_routine, 25);
        assert_eq!(loaded.config.settings.max_run_age_days, 90);
        assert_eq!(loaded.config.settings.default_timeout_seconds, 1_800);
        assert_eq!(loaded.config.settings.stream_cap_bytes, 5 * 1024 * 1024);

        let routine = &loaded.config.routines[0];
        assert!(routine.id.as_ref().unwrap().starts_with("rtn_"));
        assert!(!routine.paused);
        assert!(!routine.dangerous);
        assert_eq!(routine.runner, "codex");
    }

    #[test]
    fn rejects_unknown_runner() {
        let cwd = tempfile::tempdir().unwrap();
        let text = format!(
            r#"
[[routines]]
title = "Bad routine"
prompt = "No runner."
runner = "missing"
model = "gpt-5.5"
cwd = "{}"
schedule = "0 7 * * Sat"
"#,
            cwd.path().display()
        );

        let err = load_config_from_str(&text).unwrap_err().to_string();
        assert!(err.contains("unknown runner"));
    }

    #[test]
    fn raw_config_preserves_manual_text_only_when_ids_are_explicit() {
        let cwd = tempfile::tempdir().unwrap();
        let text = format!(
            r#"
# keep this comment
[[runners]]
id = "codex"
label = "Codex"
command = "codex"
kind = "codex"
args = ["exec", "{{{{prompt}}}}"]

[[routines]]
id = "rtn_manual"
title = "Manual routine"
prompt = "Do it."
runner = "codex"
model = "gpt-5.5"
cwd = "{}"
schedule = "0 7 * * Sat"
"#,
            cwd.path().display()
        );

        let loaded = load_raw_config_preserving_text(&text).unwrap();

        assert_eq!(loaded.routines[0].id.as_deref(), Some("rtn_manual"));

        let missing_id = text.replace("id = \"rtn_manual\"\n", "");
        let err = load_raw_config_preserving_text(&missing_id)
            .unwrap_err()
            .to_string();
        assert!(err.contains("explicit, unique routine ids"));
    }

    #[test]
    fn expands_home_relative_cwd_before_validation() {
        let home = tempfile::tempdir().unwrap();
        let repos = home.path().join("repos");
        fs::create_dir(&repos).unwrap();
        let previous_home = env::var_os("HOME");
        env::set_var("HOME", home.path());

        let text = r#"
[[runners]]
id = "codex"
label = "Codex"
command = "codex"
kind = "codex"
args = ["exec", "{{prompt}}"]

[[routines]]
id = "rtn_manual"
title = "Manual routine"
prompt = "Do it."
runner = "codex"
model = "gpt-5.5"
cwd = "~/repos"
schedule = "0 7 * * Sat"
"#;

        let loaded = load_raw_config_preserving_text(text).unwrap();

        assert_eq!(loaded.routines[0].cwd, repos);

        if let Some(home) = previous_home {
            env::set_var("HOME", home);
        } else {
            env::remove_var("HOME");
        }
    }

    #[test]
    fn accepts_weekday_range_schedule_generated_by_form() {
        let cwd = tempfile::tempdir().unwrap();
        let text = format!(
            r#"
[[runners]]
id = "codex"
label = "Codex"
command = "codex"
kind = "codex"
args = ["exec", "{{{{prompt}}}}"]

[[routines]]
id = "rtn_weekdays"
title = "Weekday routine"
prompt = "Do it."
runner = "codex"
model = "gpt-5.5"
cwd = "{}"
schedule = "0 9 * * Mon-Fri"
"#,
            cwd.path().display()
        );

        load_config_from_str(&text).unwrap();
    }

    #[test]
    fn save_config_text_creates_backup_before_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        fs::write(&path, "old").unwrap();

        save_config_text(&path, "new").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        let backups = backup_files_for(&path);
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read_to_string(&backups[0]).unwrap(), "old");
    }
}
