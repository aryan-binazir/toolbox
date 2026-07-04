use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;
use std::thread;

use axum::extract::{Path as RoutePath, State};
use axum::http::{header, HeaderMap, HeaderName, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::config::{OptionValue, RoutineConfig};
use crate::process::ProcessError;
use crate::scheduler::RoutineScheduleInfo;
use crate::store::{RunRecord, RunStatus};
use crate::{AppError, AppState};

const INDEX_HTML: &str = include_str!("../mobile/index.html");
const MOBILE_CSS: &str = include_str!("../mobile/mobile.css");
const MOBILE_JS: &str = include_str!("../mobile/mobile.js");
const ASSET_VERSION: &str = "20260704-schedule-day-checkboxes";
const MUTATION_HEADER: &str = "x-ai-scheduler-mobile";
const MUTATION_HEADER_VALUE: &str = "1";
const OUTPUT_PREVIEW_BYTES: usize = 6 * 1024;
const RUN_HISTORY_LIMIT: usize = 20;

pub struct MobileServerHandle {
    port: u16,
    shutdown: Option<oneshot::Sender<()>>,
}

impl MobileServerHandle {
    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn stop(mut self) {
        self.stop_inner();
    }

    fn stop_inner(&mut self) {
        if let Some(shutdown) = self.shutdown.take() {
            let _ = shutdown.send(());
        }
    }
}

impl Drop for MobileServerHandle {
    fn drop(&mut self) {
        self.stop_inner();
    }
}

#[derive(Clone)]
struct MobileServerState {
    app: AppState,
}

#[derive(Debug, Serialize)]
struct MobileSnapshot {
    timezone: String,
    scheduler_last_checked: Option<DateTime<Utc>>,
    routines: Vec<MobileRoutine>,
    runners: Vec<MobileRunner>,
}

#[derive(Debug, Serialize)]
struct MobileRunner {
    id: String,
    label: String,
    available: bool,
    models: Vec<OptionValue>,
    efforts: Vec<OptionValue>,
    dangerous_supported: bool,
    default_model: Option<String>,
    default_effort: Option<String>,
    default_timeout_seconds: Option<u64>,
}

#[derive(Debug, Serialize)]
struct MobileRoutine {
    id: String,
    title: String,
    description: String,
    prompt: String,
    runner_id: String,
    runner_label: String,
    runner_available: bool,
    model: Option<String>,
    effort: Option<String>,
    cwd: String,
    project_label: String,
    schedule: String,
    timezone: String,
    paused: bool,
    dangerous: bool,
    timeout_seconds: Option<u64>,
    next_run_at: Option<DateTime<Utc>>,
    schedule_error: Option<String>,
    active_run: Option<MobileRunSummary>,
    latest_run: Option<MobileRunSummary>,
}

#[derive(Debug, Serialize)]
struct MobileRuns {
    runs: Vec<MobileRunSummary>,
}

#[derive(Debug, Serialize)]
struct MobileRunSummary {
    id: String,
    status: RunStatus,
    scheduled_for: Option<DateTime<Utc>>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    exit_code: Option<i32>,
    cancel_reason: Option<String>,
    stdout_preview: Option<String>,
    stderr_preview: Option<String>,
    stdout_truncated: bool,
    stderr_truncated: bool,
}

#[derive(Debug, Deserialize)]
struct PauseRequest {
    paused: bool,
}

#[derive(Debug, Serialize)]
struct ErrorBody {
    error: String,
}

#[derive(Debug)]
struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn forbidden(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::FORBIDDEN,
            message: message.into(),
        }
    }
}

impl From<AppError> for ApiError {
    fn from(error: AppError) -> Self {
        let status = match &error {
            AppError::Message(_) => StatusCode::BAD_REQUEST,
            AppError::Process(ProcessError::AlreadyRunning(_)) => StatusCode::CONFLICT,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self {
            status,
            message: error.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (
            self.status,
            Json(ErrorBody {
                error: self.message,
            }),
        )
            .into_response()
    }
}

pub fn start_mobile_server(app: AppState, port: u16) -> MobileServerHandle {
    let (shutdown_tx, shutdown_rx) = oneshot::channel();
    thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("failed to start AI Scheduler mobile runtime: {error}");
                return;
            }
        };

        if let Err(error) = runtime.block_on(serve(app, port, shutdown_rx)) {
            eprintln!("AI Scheduler mobile server stopped: {error}");
        }
    });

    MobileServerHandle {
        port,
        shutdown: Some(shutdown_tx),
    }
}

async fn serve(
    app: AppState,
    port: u16,
    shutdown: oneshot::Receiver<()>,
) -> Result<(), std::io::Error> {
    let state = MobileServerState { app };
    let router = Router::new()
        .route("/", get(index))
        .route("/mobile.css", get(styles))
        .route("/mobile.js", get(script))
        .route("/api/snapshot", get(api_snapshot))
        .route("/api/routines", post(api_save_routine))
        .route("/api/routines/{routine_id}/runs", get(api_runs))
        .route("/api/routines/{routine_id}/pause", post(api_pause))
        .route("/api/routines/{routine_id}/run", post(api_run))
        .route("/api/routines/{routine_id}/cancel", post(api_cancel))
        .route(
            "/api/routines/{routine_id}/delete",
            post(api_delete_routine),
        )
        .route("/api/runners/refresh", post(api_refresh_runners))
        .fallback(not_found)
        .with_state(state);
    let address = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);
    let listener = tokio::net::TcpListener::bind(address).await?;
    eprintln!("AI Scheduler mobile web listening on http://{address}");
    axum::serve(listener, router)
        .with_graceful_shutdown(async {
            let _ = shutdown.await;
        })
        .await
}

async fn index() -> impl IntoResponse {
    (
        no_store_headers("text/html; charset=utf-8"),
        Html(INDEX_HTML.replace("__MOBILE_ASSET_VERSION__", ASSET_VERSION)),
    )
}

async fn styles() -> impl IntoResponse {
    (no_store_headers("text/css; charset=utf-8"), MOBILE_CSS)
}

async fn script() -> impl IntoResponse {
    (
        no_store_headers("application/javascript; charset=utf-8"),
        MOBILE_JS,
    )
}

async fn not_found() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "not found")
}

fn no_store_headers(content_type: &'static str) -> [(HeaderName, &'static str); 4] {
    [
        (header::CONTENT_TYPE, content_type),
        (
            header::CACHE_CONTROL,
            "no-store, no-cache, must-revalidate, max-age=0",
        ),
        (header::PRAGMA, "no-cache"),
        (header::EXPIRES, "0"),
    ]
}

async fn api_snapshot(
    State(state): State<MobileServerState>,
) -> Result<Json<MobileSnapshot>, ApiError> {
    Ok(Json(mobile_snapshot(&state.app)?))
}

async fn api_runs(
    State(state): State<MobileServerState>,
    RoutePath(routine_id): RoutePath<String>,
) -> Result<Json<MobileRuns>, ApiError> {
    let runs = state.app.list_runs(&routine_id)?;
    Ok(Json(MobileRuns {
        runs: runs
            .iter()
            .take(RUN_HISTORY_LIMIT)
            .map(|run| mobile_run_summary(run, true))
            .collect(),
    }))
}

async fn api_pause(
    State(state): State<MobileServerState>,
    RoutePath(routine_id): RoutePath<String>,
    headers: HeaderMap,
    Json(request): Json<PauseRequest>,
) -> Result<Json<MobileSnapshot>, ApiError> {
    verify_mutation_request(&headers)?;
    state.app.set_routine_paused(&routine_id, request.paused)?;
    Ok(Json(mobile_snapshot(&state.app)?))
}

async fn api_save_routine(
    State(state): State<MobileServerState>,
    headers: HeaderMap,
    Json(routine): Json<RoutineConfig>,
) -> Result<Json<MobileSnapshot>, ApiError> {
    verify_mutation_request(&headers)?;
    state.app.save_routine(routine)?;
    Ok(Json(mobile_snapshot(&state.app)?))
}

async fn api_run(
    State(state): State<MobileServerState>,
    RoutePath(routine_id): RoutePath<String>,
    headers: HeaderMap,
) -> Result<Json<MobileSnapshot>, ApiError> {
    verify_mutation_request(&headers)?;
    state.app.run_routine(&routine_id)?;
    Ok(Json(mobile_snapshot(&state.app)?))
}

async fn api_cancel(
    State(state): State<MobileServerState>,
    RoutePath(routine_id): RoutePath<String>,
    headers: HeaderMap,
) -> Result<Json<MobileSnapshot>, ApiError> {
    verify_mutation_request(&headers)?;
    state.app.cancel_routine(&routine_id);
    Ok(Json(mobile_snapshot(&state.app)?))
}

async fn api_delete_routine(
    State(state): State<MobileServerState>,
    RoutePath(routine_id): RoutePath<String>,
    headers: HeaderMap,
) -> Result<Json<MobileSnapshot>, ApiError> {
    verify_mutation_request(&headers)?;
    state.app.delete_routine(&routine_id)?;
    Ok(Json(mobile_snapshot(&state.app)?))
}

async fn api_refresh_runners(
    State(state): State<MobileServerState>,
    headers: HeaderMap,
) -> Result<Json<MobileSnapshot>, ApiError> {
    verify_mutation_request(&headers)?;
    state.app.refresh_runner_capabilities();
    Ok(Json(mobile_snapshot(&state.app)?))
}

fn verify_mutation_request(headers: &HeaderMap) -> Result<(), ApiError> {
    let header_ok = headers
        .get(MUTATION_HEADER)
        .and_then(|value| value.to_str().ok())
        == Some(MUTATION_HEADER_VALUE);
    if !header_ok {
        return Err(ApiError::forbidden("missing mobile mutation header"));
    }

    let cross_site = headers
        .get("sec-fetch-site")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("cross-site"));
    if cross_site {
        return Err(ApiError::forbidden("cross-site mutation rejected"));
    }

    Ok(())
}

fn mobile_snapshot(state: &AppState) -> Result<MobileSnapshot, AppError> {
    let snapshot = state.snapshot()?;
    let runner_labels = snapshot
        .config
        .runners
        .iter()
        .map(|runner| (runner.id.clone(), runner.label.clone()))
        .collect::<HashMap<_, _>>();
    let runner_availability = snapshot
        .runner_capabilities
        .iter()
        .map(|runner| (runner.id.clone(), runner.available))
        .collect::<HashMap<_, _>>();
    let schedules = snapshot
        .routine_schedules
        .iter()
        .map(|info| (info.routine_id.as_str(), info))
        .collect::<HashMap<_, _>>();
    let runners = snapshot
        .config
        .runners
        .iter()
        .map(|runner| MobileRunner {
            id: runner.id.clone(),
            label: runner.label.clone(),
            available: runner_availability
                .get(&runner.id)
                .copied()
                .unwrap_or(false),
            models: runner.model_options.clone(),
            efforts: runner.effort_options.clone(),
            dangerous_supported: runner.dangerous_flag.is_some(),
            default_model: runner.default_model.clone(),
            default_effort: runner.default_effort.clone(),
            default_timeout_seconds: runner.default_timeout_seconds,
        })
        .collect();
    let routines = snapshot
        .config
        .routines
        .iter()
        .filter_map(|routine| {
            let routine_id = routine.id.as_deref()?;
            Some(mobile_routine(
                state,
                routine,
                routine_id,
                &runner_labels,
                &runner_availability,
                &schedules,
                &snapshot.config.settings.timezone,
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(MobileSnapshot {
        timezone: snapshot.config.settings.timezone,
        scheduler_last_checked: snapshot.scheduler_last_checked,
        routines,
        runners,
    })
}

fn mobile_routine(
    state: &AppState,
    routine: &RoutineConfig,
    routine_id: &str,
    runner_labels: &HashMap<String, String>,
    runner_availability: &HashMap<String, bool>,
    schedules: &HashMap<&str, &RoutineScheduleInfo>,
    default_timezone: &str,
) -> Result<MobileRoutine, AppError> {
    let runs = state.list_runs(routine_id)?;
    let latest_run = runs.first().map(|run| mobile_run_summary(run, false));
    let active_run = runs
        .iter()
        .find(|run| is_active_status(&run.status))
        .map(|run| mobile_run_summary(run, false));
    let schedule = schedules.get(routine_id).copied();

    Ok(MobileRoutine {
        id: routine_id.to_string(),
        title: routine.title.clone(),
        description: routine.description.clone(),
        prompt: routine.prompt.clone(),
        runner_id: routine.runner.clone(),
        runner_label: runner_labels
            .get(&routine.runner)
            .cloned()
            .unwrap_or_else(|| routine.runner.clone()),
        runner_available: runner_availability
            .get(&routine.runner)
            .copied()
            .unwrap_or(false),
        model: routine.model.clone(),
        effort: routine.effort.clone(),
        cwd: routine.cwd.display().to_string(),
        project_label: project_label(&routine.cwd),
        schedule: routine.schedule.clone(),
        timezone: routine
            .timezone
            .clone()
            .unwrap_or_else(|| default_timezone.to_string()),
        paused: routine.paused,
        dangerous: routine.dangerous,
        timeout_seconds: routine.timeout_seconds,
        next_run_at: schedule.and_then(|info| info.next_run_at),
        schedule_error: schedule.and_then(|info| info.error.clone()),
        active_run,
        latest_run,
    })
}

fn mobile_run_summary(run: &RunRecord, include_output: bool) -> MobileRunSummary {
    let (stdout_preview, stdout_truncated) = if include_output {
        let (value, capped) = capped_preview(&run.stdout, OUTPUT_PREVIEW_BYTES);
        (Some(value), capped || run.stdout_truncated)
    } else {
        (None, run.stdout_truncated)
    };
    let (stderr_preview, stderr_truncated) = if include_output {
        let (value, capped) = capped_preview(&run.stderr, OUTPUT_PREVIEW_BYTES);
        (Some(value), capped || run.stderr_truncated)
    } else {
        (None, run.stderr_truncated)
    };

    MobileRunSummary {
        id: run.id.clone(),
        status: run.status.clone(),
        scheduled_for: run.scheduled_for,
        started_at: run.started_at,
        finished_at: run.finished_at,
        exit_code: run.exit_code,
        cancel_reason: run.cancel_reason.clone(),
        stdout_preview,
        stderr_preview,
        stdout_truncated,
        stderr_truncated,
    }
}

fn is_active_status(status: &RunStatus) -> bool {
    matches!(status, RunStatus::Queued | RunStatus::Running)
}

fn capped_preview(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_string(), false);
    }

    let mut boundary = max_bytes;
    while boundary > 0 && !value.is_char_boundary(boundary) {
        boundary -= 1;
    }
    (value[..boundary].to_string(), true)
}

fn project_label(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn caps_output_preview_on_char_boundary() {
        let (value, capped) = capped_preview("abcde", 3);
        assert_eq!(value, "abc");
        assert!(capped);

        let (value, capped) = capped_preview("a\u{00e9}bc", 2);
        assert_eq!(value, "a");
        assert!(capped);
    }

    #[test]
    fn rejects_missing_mutation_header() {
        let headers = HeaderMap::new();
        let error = verify_mutation_request(&headers).unwrap_err();
        assert_eq!(error.status, StatusCode::FORBIDDEN);
    }
}
