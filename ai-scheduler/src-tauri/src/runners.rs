use std::env;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::config::{OptionValue, RunnerConfig, RunnerKind};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RunnerCapability {
    pub id: String,
    pub label: String,
    pub command: String,
    pub resolved_path: Option<String>,
    pub path_env: Option<String>,
    pub probe_command: Vec<String>,
    pub available: bool,
    pub version: Option<String>,
    pub models: Vec<OptionValue>,
    pub efforts: Vec<OptionValue>,
    pub dangerous_supported: bool,
    pub error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CodexModelCatalog {
    models: Vec<CodexModel>,
}

#[derive(Debug, Deserialize)]
struct CodexModel {
    slug: String,
    display_name: String,
    #[serde(default)]
    visibility: Option<String>,
}

pub fn probe_runners(runners: &[RunnerConfig]) -> Vec<RunnerCapability> {
    let handles = runners
        .iter()
        .cloned()
        .map(|runner| {
            let probe_runner_config = runner.clone();
            (
                runner,
                thread::spawn(move || probe_runner(&probe_runner_config)),
            )
        })
        .collect::<Vec<_>>();

    handles
        .into_iter()
        .map(|(runner, handle)| match handle.join() {
            Ok(capability) => capability,
            Err(_) => RunnerCapability {
                error: Some("runner probe panicked".to_string()),
                ..configured_runner_capabilities(&[runner]).remove(0)
            },
        })
        .collect()
}

pub fn configured_runner_capabilities(runners: &[RunnerConfig]) -> Vec<RunnerCapability> {
    runners
        .iter()
        .map(|runner| RunnerCapability {
            id: runner.id.clone(),
            label: runner.label.clone(),
            command: runner.command.clone(),
            resolved_path: resolve_command_path(&runner.command)
                .map(|path| path.display().to_string()),
            path_env: env::var("PATH").ok(),
            probe_command: vec![runner.command.clone(), "--version".to_string()],
            available: false,
            version: None,
            models: runner.model_options.clone(),
            efforts: runner.effort_options.clone(),
            dangerous_supported: runner.dangerous_flag.is_some(),
            error: Some("Not probed yet".to_string()),
        })
        .collect()
}

pub fn probe_runner(runner: &RunnerConfig) -> RunnerCapability {
    let mut capability = RunnerCapability {
        id: runner.id.clone(),
        label: runner.label.clone(),
        command: runner.command.clone(),
        resolved_path: resolve_command_path(&runner.command).map(|path| path.display().to_string()),
        path_env: env::var("PATH").ok(),
        probe_command: vec![runner.command.clone(), "--version".to_string()],
        available: false,
        version: None,
        models: runner.model_options.clone(),
        efforts: runner.effort_options.clone(),
        dangerous_supported: runner.dangerous_flag.is_some(),
        error: None,
    };

    match run_command(&runner.command, &["--version"], Duration::from_secs(3)) {
        Ok(output) => {
            capability.available = true;
            capability.version = first_line(&output);
        }
        Err(error) => {
            capability.error = Some(error);
            return capability;
        }
    }

    match runner.kind {
        RunnerKind::Cursor => {
            if let Ok(output) =
                run_command(&runner.command, &["--list-models"], Duration::from_secs(5))
            {
                let models = parse_cursor_models(&output);
                if !models.is_empty() {
                    capability.models = models;
                }
            }
        }
        RunnerKind::Claude => {
            if capability.efforts.is_empty() {
                capability.efforts = vec![
                    option("low", "Low"),
                    option("medium", "Medium"),
                    option("high", "High"),
                    option("xhigh", "Extra High"),
                    option("max", "Max"),
                ];
            }
        }
        RunnerKind::Codex => {
            if let Ok(output) = run_command(
                &runner.command,
                &["debug", "models"],
                Duration::from_secs(5),
            ) {
                let models = parse_codex_models(&output);
                if !models.is_empty() {
                    capability.models = models;
                }
            }
            if capability.efforts.is_empty() {
                capability.efforts = vec![
                    option("low", "Low"),
                    option("medium", "Medium"),
                    option("high", "High"),
                    option("xhigh", "Extra High"),
                ];
            }
        }
        RunnerKind::Script | RunnerKind::Custom => {}
    }

    capability
}

pub fn parse_cursor_models(output: &str) -> Vec<OptionValue> {
    output
        .lines()
        .filter_map(|line| {
            let (id, label) = line.split_once(" - ")?;
            let id = id.trim();
            if id.is_empty() || id == "Available models" || id.starts_with("Tip:") {
                return None;
            }
            Some(option(id, label.trim()))
        })
        .collect()
}

pub fn parse_codex_models(output: &str) -> Vec<OptionValue> {
    let Ok(catalog) = serde_json::from_str::<CodexModelCatalog>(output) else {
        return vec![];
    };
    catalog
        .models
        .into_iter()
        .filter(|model| model.visibility.as_deref().unwrap_or("list") == "list")
        .map(|model| option(&model.slug, &model.display_name))
        .collect()
}

fn run_command(command: &str, args: &[&str], timeout: Duration) -> Result<String, String> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| err.to_string())?;
    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| "probe stdout pipe was unavailable".to_string())?;
    let mut stderr = child
        .stderr
        .take()
        .ok_or_else(|| "probe stderr pipe was unavailable".to_string())?;
    let stdout_reader = thread::spawn(move || {
        let mut output = Vec::new();
        stdout.read_to_end(&mut output).map(|_| output)
    });
    let stderr_reader = thread::spawn(move || {
        let mut output = Vec::new();
        stderr.read_to_end(&mut output).map(|_| output)
    });

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if start.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(format!("probe timed out after {}s", timeout.as_secs()));
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(err) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(err.to_string());
            }
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| "probe stdout reader panicked".to_string())?
        .map_err(|error| error.to_string())?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| "probe stderr reader panicked".to_string())?
        .map_err(|error| error.to_string())?;
    let mut text = String::from_utf8_lossy(&stdout).to_string();
    if text.trim().is_empty() {
        text = String::from_utf8_lossy(&stderr).to_string();
    }
    if !status.success() {
        let status = status
            .code()
            .map(|code| code.to_string())
            .unwrap_or_else(|| "signal".to_string());
        let detail = text.trim();
        return Err(if detail.is_empty() {
            format!("probe exited with status {status}")
        } else {
            format!("probe exited with status {status}: {detail}")
        });
    }
    Ok(text)
}

fn first_line(value: &str) -> Option<String> {
    value
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(ToOwned::to_owned)
}

fn option(value: &str, label: &str) -> OptionValue {
    OptionValue {
        value: value.to_string(),
        label: label.to_string(),
    }
}

fn resolve_command_path(command: &str) -> Option<PathBuf> {
    let command_path = Path::new(command);
    if command_path.components().count() > 1 {
        return command_path.is_file().then(|| command_path.to_path_buf());
    }

    let paths = env::var_os("PATH")?;
    env::split_paths(&paths)
        .map(|path| path.join(command))
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn nonzero_version_probe_is_not_reported_as_available() {
        use std::fs;
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let command = temp.path().join("broken-runner");
        fs::write(&command, "#!/bin/sh\necho broken >&2\nexit 2\n").unwrap();
        fs::set_permissions(&command, fs::Permissions::from_mode(0o700)).unwrap();
        let runner = RunnerConfig {
            id: "broken".to_string(),
            label: "Broken".to_string(),
            command: command.display().to_string(),
            kind: RunnerKind::Custom,
            args: vec![],
            dangerous_flag: None,
            default_model: None,
            default_effort: None,
            model_options: vec![],
            effort_options: vec![],
            stdin: crate::config::StdinMode::Null,
            default_timeout_seconds: None,
        };

        let capability = probe_runner(&runner);

        assert!(!capability.available);
        assert!(capability
            .error
            .as_deref()
            .is_some_and(|error| { error.contains("status 2") && error.contains("broken") }));
    }

    #[test]
    fn parses_cursor_model_output() {
        let models = parse_cursor_models(
            r#"
Available models

auto - Auto
composer-2.5 - Composer 2.5
composer-2.5-fast - Composer 2.5 Fast (default)
gpt-5.5-extra-high-fast - GPT-5.5 Extra High Fast

Tip: use --model <id> to switch.
"#,
        );

        assert_eq!(models[0].value, "auto");
        assert_eq!(models[1].value, "composer-2.5");
        assert_eq!(models[2].label, "Composer 2.5 Fast (default)");
        assert_eq!(models[3].value, "gpt-5.5-extra-high-fast");
    }

    #[test]
    fn parses_codex_model_catalog() {
        let models = parse_codex_models(
            r#"
{
  "models": [
    {
      "slug": "gpt-5.5",
      "display_name": "GPT-5.5",
      "visibility": "list"
    },
    {
      "slug": "gpt-5.3-codex-spark",
      "display_name": "GPT-5.3 Codex Spark",
      "visibility": "list"
    },
    {
      "slug": "internal-model",
      "display_name": "Internal",
      "visibility": "hidden"
    }
  ]
}
"#,
        );

        assert_eq!(
            models,
            vec![
                option("gpt-5.5", "GPT-5.5"),
                option("gpt-5.3-codex-spark", "GPT-5.3 Codex Spark")
            ]
        );
    }

    #[test]
    fn resolves_command_from_path() {
        let path = resolve_command_path("sh").unwrap();

        assert!(path.display().to_string().contains("sh"));
    }
}
