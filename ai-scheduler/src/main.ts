import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

type OptionValue = { value: string; label: string };

type Settings = {
  timezone: string;
  max_runs_per_routine: number;
  max_run_age_days: number;
  default_timeout_seconds: number;
  stream_cap_bytes: number;
};

type RunnerConfig = {
  id: string;
  label: string;
  command: string;
  kind: string;
  dangerous_flag?: string | null;
  default_model?: string;
  default_effort?: string;
  model_options: OptionValue[];
  effort_options: OptionValue[];
};

type RoutineConfig = {
  id?: string | null;
  title: string;
  description: string;
  prompt: string;
  runner: string;
  model?: string | null;
  effort?: string | null;
  cwd: string;
  schedule: string;
  timezone?: string | null;
  paused: boolean;
  dangerous: boolean;
  timeout_seconds?: number | null;
};

type AppConfig = {
  settings: Settings;
  runners: RunnerConfig[];
  routines: RoutineConfig[];
};

type RunnerCapability = {
  id: string;
  label: string;
  command: string;
  resolved_path?: string | null;
  path_env?: string | null;
  probe_command: string[];
  available: boolean;
  version?: string | null;
  models: OptionValue[];
  efforts: OptionValue[];
  dangerous_supported: boolean;
  error?: string | null;
};

type RunStatus =
  | "queued"
  | "running"
  | "succeeded"
  | "failed"
  | "cancelled"
  | "timed_out"
  | "missed"
  | "superseded";

type RunRecord = {
  id: string;
  routine_id: string;
  routine_title: string;
  status: RunStatus;
  scheduled_for?: string | null;
  started_at?: string | null;
  finished_at?: string | null;
  exit_code?: number | null;
  signal?: number | null;
  cancel_reason?: string | null;
  command: string[];
  cwd: string;
  stdout: string;
  stderr: string;
  stdout_truncated: boolean;
  stderr_truncated: boolean;
};

type RoutineScheduleInfo = {
  routine_id: string;
  next_run_at?: string | null;
  error?: string | null;
};

type SchedulePreview = {
  next_run_at?: string | null;
  error?: string | null;
};

type Snapshot = {
  config_path: string;
  db_path: string;
  config: AppConfig;
  runner_capabilities: RunnerCapability[];
  scheduler_last_checked?: string | null;
  routine_schedules: RoutineScheduleInfo[];
  mobile_server_error?: string | null;
};

type State = {
  snapshot?: Snapshot;
  selectedRoutineId?: string;
  renderSignature?: string;
  runs: RunRecord[];
  query: string;
  mode: "details" | "edit" | "new";
  rawOpen: boolean;
  rawText: string;
  formDraft?: RoutineConfig;
  schedulePreview?: SchedulePreview & { key: string; checking?: boolean };
  schedulePreviewTimer?: number;
  schedulePreviewSeq: number;
  openRunIds: Set<string>;
  copiedRunId?: string;
  error?: string;
  errorTimer?: number;
  busy: boolean;
  snapshotSequence: number;
  runsSequence: number;
};

type RenderOptions = {
  preserveScroll?: boolean;
  skipIfUnchanged?: boolean;
};

type ScrollSnapshot = {
  detailTop: number;
  sidebarTop: number;
  runOutputPositions: {
    runId: string;
    stream: "stdout" | "stderr";
    top: number;
    left: number;
  }[];
};

type ScheduleControls = {
  days: string[];
  time: string;
  custom: string;
  customEnabled: boolean;
};

const state: State = {
  runs: [],
  query: "",
  mode: "details",
  rawOpen: false,
  rawText: "",
  schedulePreviewSeq: 0,
  openRunIds: new Set(),
  busy: false,
  snapshotSequence: 0,
  runsSequence: 0,
};

const app = document.querySelector<HTMLDivElement>("#app")!;
const DAY_VALUES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const CRON_DAY_ORDER = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const WEEKDAY_VALUES = ["Mon", "Tue", "Wed", "Thu", "Fri"];
const DAY_LABELS: Record<string, string> = {
  Mon: "Monday",
  Tue: "Tuesday",
  Wed: "Wednesday",
  Thu: "Thursday",
  Fri: "Friday",
  Sat: "Saturday",
  Sun: "Sunday",
};
const DAY_PRESETS = [
  { id: "weekdays", label: "Mon-Fri", days: WEEKDAY_VALUES },
  { id: "mon-sun", label: "Mon-Sun", days: DAY_VALUES },
  { id: "sun-sat", label: "Sun-Sat", days: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] },
];
const TIME_OPTIONS = buildTimeOptions();
const ACTIVE_RUN_STATUSES: RunStatus[] = ["queued", "running"];

async function loadSnapshot(keepSelection = true, renderOptions: RenderOptions = {}, clearExistingError = true) {
  const sequence = ++state.snapshotSequence;
  if (clearExistingError) clearError();
  try {
    const snapshot = await invoke<Snapshot>("get_snapshot");
    if (sequence !== state.snapshotSequence) return;
    state.snapshot = snapshot;
    const routines = state.snapshot.config.routines;
    if (!keepSelection || !state.selectedRoutineId || !routines.some((r) => r.id === state.selectedRoutineId)) {
      state.selectedRoutineId = routines[0]?.id ?? undefined;
    }
    await loadRuns();
  } catch (error) {
    if (sequence === state.snapshotSequence) setError(error);
  }
  if (sequence === state.snapshotSequence) render(renderOptions);
}

function clearError() {
  state.error = undefined;
  if (state.errorTimer) {
    window.clearTimeout(state.errorTimer);
    state.errorTimer = undefined;
  }
}

function setError(error: unknown) {
  state.error = String(error);
  if (state.errorTimer) window.clearTimeout(state.errorTimer);
  state.errorTimer = window.setTimeout(() => {
    state.error = undefined;
    state.errorTimer = undefined;
    render({ preserveScroll: true });
  }, 8_000);
}

async function loadRuns() {
  const routineId = state.selectedRoutineId;
  const sequence = ++state.runsSequence;
  if (!routineId) {
    state.runs = [];
    return;
  }
  const runs = await invoke<RunRecord[]>("list_runs", { routineId });
  if (sequence === state.runsSequence && state.selectedRoutineId === routineId) {
    state.runs = runs;
  }
}

function selectedRoutine(): RoutineConfig | undefined {
  return state.snapshot?.config.routines.find((routine) => routine.id === state.selectedRoutineId);
}

function runnerFor(routine: RoutineConfig | undefined): RunnerConfig | undefined {
  if (!routine) return undefined;
  return state.snapshot?.config.runners.find((runner) => runner.id === routine.runner);
}

function isScriptLikeRunner(runner?: RunnerConfig | null) {
  if (!runner) return false;
  if (runner.kind === "script") return true;
  return (runner.kind === "custom" || !runner.kind) && !(runner.default_model || runner.model_options?.length);
}

function capabilityFor(runnerId: string): RunnerCapability | undefined {
  return state.snapshot?.runner_capabilities.find((runner) => runner.id === runnerId);
}

function scheduleInfoFor(routineId?: string | null): RoutineScheduleInfo | undefined {
  if (!routineId) return undefined;
  return state.snapshot?.routine_schedules.find((schedule) => schedule.routine_id === routineId);
}

function isActiveStatus(status: RunStatus) {
  return ACTIVE_RUN_STATUSES.includes(status);
}

function activeRun() {
  return state.runs.find((run) => isActiveStatus(run.status));
}

function filteredRoutines(paused: boolean) {
  const query = state.query.trim().toLowerCase();
  return (state.snapshot?.config.routines ?? [])
    .filter((routine) => routine.paused === paused)
    .filter((routine) => {
      if (!query) return true;
      return [routine.title, routine.description, routine.prompt, routine.cwd]
        .join(" ")
        .toLowerCase()
        .includes(query);
    });
}

function projectLabel(cwd: string) {
  return cwd.split("/").filter(Boolean).at(-1) ?? cwd;
}

function scheduleLabel(routine: RoutineConfig) {
  const timezone = routine.timezone || state.snapshot?.config.settings.timezone || "UTC";
  return `${friendlySchedule(routine.schedule)} · ${timezone}`;
}

function nextRunLabel(routine: RoutineConfig) {
  const info = scheduleInfoFor(routine.id);
  if (info?.error) return info.error;
  if (!info?.next_run_at) return "—";
  return formatDate(info.next_run_at);
}

function friendlySchedule(schedule: string) {
  const parsed = parseSimpleSchedule(schedule);
  if (!parsed) return schedule;
  return `${daySetLabel(parsed.days)} at ${timeLabel(parsed.time)}`;
}

function statusClass(status: RunStatus) {
  return `status status-${status.replace("_", "-")}`;
}

function formatDate(value?: string | null) {
  if (!value) return "—";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(new Date(value));
}

function render(options: RenderOptions = {}) {
  const snapshot = state.snapshot;
  if (!snapshot) {
    app.innerHTML = `<main class="shell"><div class="empty">Loading</div></main>`;
    return;
  }

  const signature = renderSignature(snapshot);
  if (options.skipIfUnchanged && signature === state.renderSignature) {
    return;
  }
  state.renderSignature = signature;

  const scrollSnapshot = options.preserveScroll ? captureScrollSnapshot() : undefined;

  const current = filteredRoutines(false);
  const paused = filteredRoutines(true);
  const routine = selectedRoutine();
  const runner = runnerFor(routine);
  const capability = runner ? capabilityFor(runner.id) : undefined;

  app.innerHTML = `
    <main class="shell">
      <aside class="sidebar">
        <nav class="tabs">
          <button class="tab active">Routines</button>
          <button class="tab" data-action="open-raw">Config</button>
        </nav>
        <header class="hero">
          <h1>Scheduled</h1>
          <p>${snapshot.config_path}</p>
        </header>
        <label class="search">
          <span>⌕</span>
          <input id="search" value="${escapeHtml(state.query)}" placeholder="Search routines" />
        </label>
        ${renderRoutineSection("Current", current)}
        ${renderRoutineSection("Paused", paused)}
        <section class="runner-panel">
          <div class="section-head">
            <h2>Runners</h2>
            <button class="icon-button runner-refresh" data-action="refresh-runners" title="Refresh runners" aria-label="Refresh runners">
              <svg viewBox="0 0 24 24" aria-hidden="true">
                <path d="M21 12a9 9 0 0 1-15 6.7L3 16" />
                <path d="M3 21v-5h5" />
                <path d="M3 12a9 9 0 0 1 15-6.7L21 8" />
                <path d="M21 3v5h-5" />
              </svg>
            </button>
          </div>
          ${snapshot.runner_capabilities.map(renderRunnerStatus).join("")}
        </section>
      </aside>
      <section class="detail">
        ${snapshot.mobile_server_error ? `<div class="banner">Mobile web unavailable: ${escapeHtml(snapshot.mobile_server_error)}</div>` : ""}
        ${state.error ? `<div class="banner">${escapeHtml(state.error)}</div>` : ""}
        ${renderDetailContent(routine, runner, capability)}
      </section>
      ${state.rawOpen ? renderRawPanel() : ""}
    </main>
  `;

  wireEvents();
  restoreScrollSnapshot(scrollSnapshot);
}

function captureScrollSnapshot(): ScrollSnapshot {
  return {
    detailTop: document.querySelector<HTMLElement>(".detail")?.scrollTop ?? 0,
    sidebarTop: document.querySelector<HTMLElement>(".sidebar")?.scrollTop ?? 0,
    runOutputPositions: Array.from(document.querySelectorAll<HTMLElement>("details.run[data-run-id] pre[data-run-output]"))
      .map((element) => ({
        runId: element.closest<HTMLElement>("details.run")?.dataset.runId ?? "",
        stream: element.dataset.runOutput as "stdout" | "stderr",
        top: element.scrollTop,
        left: element.scrollLeft,
      }))
      .filter((position) => position.runId && position.stream),
  };
}

function restoreScrollSnapshot(snapshot?: ScrollSnapshot) {
  if (!snapshot) return;
  const detail = document.querySelector<HTMLElement>(".detail");
  const sidebar = document.querySelector<HTMLElement>(".sidebar");
  if (detail) detail.scrollTop = snapshot.detailTop;
  if (sidebar) sidebar.scrollTop = snapshot.sidebarTop;
  for (const position of snapshot.runOutputPositions) {
    const output = document.querySelector<HTMLElement>(
      `details.run[data-run-id="${selectorEscape(position.runId)}"] pre[data-run-output="${position.stream}"]`,
    );
    if (output) {
      output.scrollTop = position.top;
      output.scrollLeft = position.left;
    }
  }
}

function renderSignature(snapshot: Snapshot) {
  return JSON.stringify({
    selectedRoutineId: state.selectedRoutineId,
    query: state.query,
    mode: state.mode,
    rawOpen: state.rawOpen,
    rawText: state.rawOpen ? state.rawText : "",
    formDraft: state.formDraft,
    schedulePreview: state.schedulePreview,
    openRunIds: Array.from(state.openRunIds).sort(),
    copiedRunId: state.copiedRunId,
    error: state.error,
    snapshot: {
      config_path: snapshot.config_path,
      db_path: snapshot.db_path,
      config: snapshot.config,
      runner_capabilities: snapshot.runner_capabilities,
      routine_schedules: snapshot.routine_schedules,
    },
    runs: state.runs,
  });
}

function renderDetailContent(routine?: RoutineConfig, runner?: RunnerConfig, capability?: RunnerCapability) {
  if (state.mode === "new") return renderRoutineForm(state.formDraft ?? newRoutine());
  if (!routine) return renderEmptyDetail();
  return renderDetail(routine, runner, capability);
}

function renderRoutineSection(title: string, routines: RoutineConfig[]) {
  return `
    <section class="routine-section">
      <div class="section-head">
        <h2>${title}</h2>
        ${title === "Current" ? `<button class="icon-button add-button" data-action="new-routine" title="New routine" aria-label="New routine">+</button>` : ""}
      </div>
      <div class="routine-list">
        ${
          routines.length
            ? routines.map(renderRoutineRow).join("")
            : `<div class="muted-row">No ${title.toLowerCase()} routines</div>`
        }
      </div>
    </section>
  `;
}

function renderRoutineRow(routine: RoutineConfig) {
  const selected = routine.id === state.selectedRoutineId ? " selected" : "";
  const pauseTitle = routine.paused ? "Resume routine" : "Pause routine";
  const routineId = escapeHtml(routine.id || "");
  return `
    <button class="routine-row${selected}" data-routine-id="${routineId}">
      <span class="pause-dot" data-action="toggle-pause" data-id="${routineId}" title="${escapeHtml(pauseTitle)}">
        ${routine.paused ? "▷" : ""}
      </span>
      <span class="routine-copy">
        <span class="routine-title">${escapeHtml(routine.title)}</span>
        <span class="routine-project">${escapeHtml(projectLabel(routine.cwd))}</span>
        <span class="routine-schedule">${
          routine.paused
            ? escapeHtml(scheduleLabel(routine))
            : `Next · ${escapeHtml(nextRunLabel(routine))}`
        }</span>
      </span>
    </button>
  `;
}

function renderRunnerStatus(runner: RunnerCapability) {
  const status = runner.available ? "ok" : "bad";
  const command = runner.probe_command?.length ? shellJoin(runner.probe_command) : `${runner.command} --version`;
  const readiness = runner.available ? "Version check passed; auth not verified" : "Version check failed";
  return `
    <div class="runner-status">
      <span class="runner-light ${status}"></span>
      <span>
        <strong>${escapeHtml(runner.label)}</strong>
        <small>${escapeHtml(runner.version || runner.error || "Not available")}</small>
        <small>${escapeHtml(runner.resolved_path ? `Path ${runner.resolved_path}` : `Command ${runner.command} not resolved`)}</small>
        <small>${escapeHtml(`${readiness} · ${command}`)}</small>
      </span>
    </div>
  `;
}

function renderDetail(routine: RoutineConfig, runner?: RunnerConfig, capability?: RunnerCapability) {
  if (state.mode === "edit") return renderRoutineForm(state.formDraft ?? routine);
  const latest = state.runs[0];
  const running = activeRun();
  const nextRun = routine.paused ? "Paused" : nextRunLabel(routine);
  const schedulerChecked = formatDate(state.snapshot?.scheduler_last_checked);
  return `
    <div class="detail-toolbar">
      ${running ? `<span class="toolbar-status">Running · ${formatDate(running.started_at || running.scheduled_for)}</span>` : ""}
      <button class="primary" data-action="run" ${running ? "disabled" : ""}>▷ Run now</button>
      ${running ? `<button class="danger" data-action="cancel-run">Cancel run</button>` : ""}
      <button data-action="toggle-selected-pause">${routine.paused ? "Resume" : "Pause"}</button>
      <button data-action="edit-routine">Edit</button>
      <button class="danger" data-action="delete-routine">Delete</button>
    </div>
    <article class="routine-detail">
      <h1>${escapeHtml(routine.title)}</h1>
      <p>${escapeHtml(routine.description || "No description")}</p>
      <pre class="prompt">${escapeHtml(routine.prompt)}</pre>
      <dl class="meta-grid">
        <div><dt>Status</dt><dd>${running ? `Running · ${running.status}` : routine.paused ? "Paused" : "Active"}</dd></div>
        <div><dt>Runner</dt><dd>${escapeHtml(runner?.label ?? routine.runner)}</dd></div>
        <div><dt>Available</dt><dd>${capability?.available ? "Yes" : "No"}</dd></div>
        ${
          isScriptLikeRunner(runner)
            ? ""
            : `<div><dt>Model</dt><dd>${escapeHtml(routine.model || runner?.default_model || "—")}</dd></div>
        <div><dt>Effort</dt><dd>${escapeHtml(routine.effort || runner?.default_effort || "—")}</dd></div>
        <div><dt>Dangerous</dt><dd>${escapeHtml(routine.dangerous ? `On · ${runner?.dangerous_flag || "runner flag"}` : "Off")}</dd></div>`
        }
        <div><dt>Working directory</dt><dd>${escapeHtml(routine.cwd)}</dd></div>
        <div><dt>Schedule</dt><dd>${escapeHtml(scheduleLabel(routine))}</dd></div>
        <div><dt>Next run</dt><dd>${escapeHtml(nextRun)}</dd></div>
        <div><dt>Scheduler checked</dt><dd>${schedulerChecked}</dd></div>
        <div><dt>Last run</dt><dd>${latest ? `${latest.status} · ${formatDate(latest.finished_at || latest.started_at || latest.scheduled_for)}` : "—"}</dd></div>
      </dl>
    </article>
    <section class="runs">
      <h2>Runs</h2>
      ${state.runs.length ? state.runs.map(renderRun).join("") : `<div class="muted-row">No runs yet</div>`}
    </section>
  `;
}

function renderRun(run: RunRecord) {
  const runId = escapeHtml(run.id);
  const copyPayload = copyPayloadForRun(run);
  const copyLabel = state.copiedRunId === run.id ? "Copied" : copyPayload.label;
  return `
    <details class="run" data-run-id="${runId}" ${state.openRunIds.has(run.id) ? "open" : ""}>
      <summary>
        <span class="${statusClass(run.status)}">${run.status.replace("_", " ")}</span>
        <span>${formatDate(run.started_at || run.scheduled_for)}</span>
        <span>${escapeHtml(run.cancel_reason || "")}</span>
        <button class="copy-run" data-action="copy-run" data-run-id="${runId}" title="${escapeHtml(copyPayload.title)}">${escapeHtml(copyLabel)}</button>
      </summary>
      <div class="run-body">
        <div class="command">${escapeHtml(run.command.join(" "))}</div>
        <div class="output-grid">
          <section>
            <h3>stdout${run.stdout_truncated ? " · truncated" : ""}</h3>
            <pre data-run-output="stdout">${escapeHtml(run.stdout || "")}</pre>
          </section>
          <section>
            <h3>stderr${run.stderr_truncated ? " · truncated" : ""}</h3>
            <pre data-run-output="stderr">${escapeHtml(run.stderr || "")}</pre>
          </section>
        </div>
      </div>
    </details>
  `;
}

function renderEmptyDetail() {
  return `
    <div class="empty-detail">
      <button class="primary" data-action="new-routine">New routine</button>
    </div>
  `;
}

function renderRoutineForm(routine: RoutineConfig) {
  const config = state.snapshot!.config;
  const runner = config.runners.find((item) => item.id === routine.runner) ?? config.runners[0];
  const capability = capabilityFor(runner?.id ?? "");
  const models = capability?.models.length ? capability.models : runner?.model_options ?? [];
  const efforts = capability?.efforts.length ? capability.efforts : runner?.effort_options ?? [];
  const timeoutSeconds = routine.timeout_seconds ?? config.settings.default_timeout_seconds;
  const schedule = parseScheduleControls(routine.schedule);
  const modelListId = `model-options-${escapeAttribute(runner?.id ?? "runner")}`;
  const scriptLike = isScriptLikeRunner(runner);
  return `
    <form class="routine-form" id="routine-form">
      <div class="detail-toolbar">
        <button class="primary" type="submit">Save</button>
        <button type="button" data-action="cancel-edit">Cancel</button>
      </div>
      <input type="hidden" name="id" value="${escapeHtml(routine.id || "")}" />
      <label>Title<input name="title" value="${escapeHtml(routine.title)}" required /></label>
      <label>Description<textarea name="description">${escapeHtml(routine.description)}</textarea></label>
      <label>${scriptLike ? "Command" : "Prompt"}<textarea name="prompt" class="prompt-input" required placeholder="${scriptLike ? "echo hello || /path/to/script.sh" : ""}">${escapeHtml(routine.prompt)}</textarea></label>
      <div class="form-grid">
        <label>Runner<select name="runner">${config.runners.map((item) => optionHtml(item.id, item.label, routine.runner)).join("")}</select></label>
        ${
          scriptLike
            ? ""
            : `<label>Model<input name="model" list="${modelListId}" value="${escapeHtml(routine.model || runner?.default_model || "")}" required /></label>
        <datalist id="${modelListId}">${models.map((item) => `<option value="${escapeHtml(item.value)}">${escapeHtml(item.label)}</option>`).join("")}</datalist>
        <label>Effort<select name="effort"><option value="">—</option>${efforts.map((item) => optionHtml(item.value, item.label, routine.effort || runner?.default_effort)).join("")}</select></label>`
        }
        ${renderScheduleDayControls(schedule)}
        ${
          schedule.customEnabled
            ? `<label class="custom-schedule">Cron<input name="schedule_custom" value="${escapeHtml(schedule.custom)}" required /></label>`
            : `<label>Time<select name="schedule_time">${TIME_OPTIONS.map((item) => optionHtml(item.value, item.label, schedule.time)).join("")}</select></label>`
        }
        <label>Timezone<input name="timezone" value="${escapeHtml(routine.timezone || config.settings.timezone)}" required /></label>
        <label>Timeout seconds<input name="timeout_seconds" type="number" min="1" value="${timeoutSeconds}" /></label>
      </div>
      ${renderSchedulePreview(routine)}
      <label>Working directory
        <span class="path-row">
          <input name="cwd" value="${escapeHtml(routine.cwd)}" required />
          <button type="button" data-action="choose-cwd">Browse</button>
        </span>
      </label>
      <div class="toggles">
        <label><input type="checkbox" name="paused" ${routine.paused ? "checked" : ""} /> Paused</label>
        ${
          scriptLike
            ? ""
            : `<label><input type="checkbox" name="dangerous" ${routine.dangerous ? "checked" : ""} /> Dangerous mode</label>`
        }
      </div>
      ${
        !scriptLike && runner?.dangerous_flag
          ? `<div class="inline-note">Danger flag · ${escapeHtml(runner.dangerous_flag)}</div>`
          : scriptLike
            ? `<div class="inline-note">Runs as bash -lc in the working directory</div>`
            : ""
      }
    </form>
  `;
}

function renderScheduleDayControls(schedule: ScheduleControls) {
  return `
    <fieldset class="schedule-days">
      <legend>Days</legend>
      ${
        schedule.customEnabled
          ? ""
          : `<div class="schedule-preset-row">${DAY_PRESETS.map((preset) => `<button type="button" data-schedule-preset="${escapeHtml(preset.id)}">${escapeHtml(preset.label)}</button>`).join("")}</div>
             <div class="day-checkbox-grid">${DAY_VALUES.map((day) => renderDayCheckbox(day, schedule.days.includes(day))).join("")}</div>`
      }
      <label class="schedule-custom-toggle"><input type="checkbox" name="schedule_custom_enabled" ${schedule.customEnabled ? "checked" : ""} /> Custom cron</label>
    </fieldset>
  `;
}

function renderDayCheckbox(day: string, checked: boolean) {
  return `<label><input type="checkbox" name="schedule_days" value="${escapeHtml(day)}" ${checked ? "checked" : ""} /> ${escapeHtml(day)}</label>`;
}

function renderSchedulePreview(routine: RoutineConfig) {
  const key = schedulePreviewKey(routine);
  const preview = state.schedulePreview?.key === key ? state.schedulePreview : undefined;
  const className = preview?.error ? "bad" : preview?.next_run_at ? "ok" : "";
  const value = preview?.checking
    ? "Checking schedule"
    : preview?.error
      ? preview.error
      : preview?.next_run_at
        ? `Next run · ${formatDate(preview.next_run_at)}`
        : "Schedule not checked";
  return `<div class="schedule-preview ${className}">${escapeHtml(value)}</div>`;
}

function renderRawPanel() {
  return `
    <div class="modal-backdrop">
      <section class="raw-panel">
        <div class="panel-head">
          <h2>Config</h2>
          <button class="icon-button" data-action="close-raw">×</button>
        </div>
        <textarea id="raw-config" spellcheck="false">${escapeHtml(state.rawText)}</textarea>
        <div class="panel-actions">
          <button data-action="reload-raw">Reload</button>
          <button class="primary" data-action="save-raw">Save</button>
        </div>
      </section>
    </div>
  `;
}

function newRoutine(): RoutineConfig {
  const config = state.snapshot!.config;
  const runner = config.runners[0];
  return {
    id: null,
    title: "",
    description: "",
    prompt: "",
    runner: runner?.id ?? "",
    model: runner?.default_model ?? "",
    effort: runner?.default_effort ?? null,
    cwd: "",
    schedule: "0 7 * * *",
    timezone: config.settings.timezone,
    paused: true,
    dangerous: false,
    timeout_seconds: config.settings.default_timeout_seconds,
  };
}

function optionHtml(value: string, label: string, selected?: string | null) {
  return `<option value="${escapeHtml(value)}" ${value === selected ? "selected" : ""}>${escapeHtml(label)}</option>`;
}

function buildTimeOptions() {
  const options: OptionValue[] = [];
  for (let hour = 0; hour < 24; hour += 1) {
    for (let minute = 0; minute < 60; minute += 5) {
      const value = `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
      options.push({ value, label: timeLabel(value) });
    }
  }
  return options;
}

function parseScheduleControls(schedule: string): ScheduleControls {
  const parsed = parseSimpleSchedule(schedule);
  if (parsed) return { ...parsed, custom: "", customEnabled: false };
  return { days: [...DAY_VALUES], time: "07:00", custom: schedule, customEnabled: true };
}

function parseSimpleSchedule(schedule: string) {
  const fields = schedule.trim().split(/\s+/).filter(Boolean);
  const cron = fields.length === 6 && fields[0] === "0" ? fields.slice(1) : fields;
  if (cron.length !== 5) return undefined;

  const [minute, hour, dayOfMonth, month, dayField] = cron;
  if (dayOfMonth !== "*" || month !== "*") return undefined;
  const days = parseDayField(dayField);
  if (!days?.length) return undefined;

  const hourNumber = Number(hour);
  const minuteNumber = Number(minute);
  if (!Number.isInteger(hourNumber) || !Number.isInteger(minuteNumber)) return undefined;
  if (hourNumber < 0 || hourNumber > 23 || minuteNumber < 0 || minuteNumber > 59) return undefined;
  if (minuteNumber % 5 !== 0) return undefined;

  return {
    days,
    time: `${String(hourNumber).padStart(2, "0")}:${String(minuteNumber).padStart(2, "0")}`,
  };
}

function buildSimpleSchedule(days: string[], time: string) {
  const [hour = "7", minute = "0"] = time.split(":");
  return `${Number(minute)} ${Number(hour)} * * ${buildDayField(days)}`;
}

function buildDayField(days: string[]) {
  const ordered = orderDays(days);
  if (sameDays(ordered, DAY_VALUES)) return "*";
  if (sameDays(ordered, WEEKDAY_VALUES)) return "Mon-Fri";
  return ordered.join(",");
}

function parseDayField(value: string) {
  if (value === "*") return [...DAY_VALUES];
  const days: string[] = [];
  for (const part of value.split(",")) {
    const expanded = expandDayPart(part.trim());
    if (!expanded.length) return undefined;
    days.push(...expanded);
  }
  return orderDays(days);
}

function expandDayPart(value: string) {
  if (!value) return [];
  if (value.includes("-")) {
    const [startRaw, endRaw] = value.split("-", 2);
    const start = normalizeDayValue(startRaw);
    const end = normalizeDayValue(endRaw);
    if (!start || !end) return [];
    const startIndex = CRON_DAY_ORDER.indexOf(start);
    const endIndex = CRON_DAY_ORDER.indexOf(end);
    if (startIndex < 0 || endIndex < 0) return [];
    const ordered =
      startIndex <= endIndex
        ? CRON_DAY_ORDER.slice(startIndex, endIndex + 1)
        : [...CRON_DAY_ORDER.slice(startIndex), ...CRON_DAY_ORDER.slice(0, endIndex + 1)];
    return ordered;
  }
  const day = normalizeDayValue(value);
  return day ? [day] : [];
}

function normalizeDayValue(value: string) {
  const lower = value.trim().toLowerCase();
  return DAY_VALUES.find((day) => day.toLowerCase() === lower) ?? undefined;
}

function orderDays(days: string[]) {
  const selected = new Set(days.filter((day) => DAY_VALUES.includes(day)));
  return DAY_VALUES.filter((day) => selected.has(day));
}

function sameDays(left: string[], right: string[]) {
  return left.length === right.length && left.every((day, index) => day === right[index]);
}

function schedulePreviewKey(routine: RoutineConfig) {
  return `${routine.schedule}::${routine.timezone || state.snapshot?.config.settings.timezone || ""}`;
}

function queueSchedulePreview(routine: RoutineConfig) {
  const key = schedulePreviewKey(routine);
  state.schedulePreview = { key, checking: true };
  if (state.schedulePreviewTimer) window.clearTimeout(state.schedulePreviewTimer);
  const sequence = ++state.schedulePreviewSeq;
  state.schedulePreviewTimer = window.setTimeout(async () => {
    try {
      const preview = await invoke<SchedulePreview>("preview_schedule", { routine });
      if (sequence !== state.schedulePreviewSeq) return;
      state.schedulePreview = { key, ...preview };
    } catch (error) {
      if (sequence !== state.schedulePreviewSeq) return;
      state.schedulePreview = { key, error: String(error) };
    }
    if (state.mode === "edit" || state.mode === "new") {
      render({ preserveScroll: true });
    }
  }, 250);
}

function daySetLabel(days: string[]) {
  if (sameDays(days, DAY_VALUES)) return "Every day";
  if (sameDays(days, WEEKDAY_VALUES)) return "Mon-Fri";
  if (days.length === 1) return DAY_LABELS[days[0]] ?? days[0];
  return days.join(", ");
}

function timeLabel(value: string) {
  const [hourRaw = "0", minute = "00"] = value.split(":");
  const hour = Number(hourRaw);
  const displayHour = hour % 12 || 12;
  const period = hour < 12 ? "AM" : "PM";
  return `${displayHour}:${minute.padStart(2, "0")} ${period}`;
}

function wireEvents() {
  document.querySelector<HTMLInputElement>("#search")?.addEventListener("input", (event) => {
    state.query = (event.currentTarget as HTMLInputElement).value;
    render();
  });

  document.querySelectorAll<HTMLElement>("[data-routine-id]").forEach((row) => {
    row.addEventListener("click", async (event) => {
      if ((event.target as HTMLElement).closest("[data-action='toggle-pause']")) return;
      state.selectedRoutineId = row.dataset.routineId;
      state.mode = "details";
      state.openRunIds.clear();
      try {
        await loadRuns();
        render();
      } catch (error) {
        setError(error);
        render({ preserveScroll: true });
      }
    });
  });

  document.querySelectorAll<HTMLElement>("[data-action]").forEach((element) => {
    element.addEventListener("click", async (event) => {
      event.preventDefault();
      event.stopPropagation();
      await handleAction((event.currentTarget as HTMLElement).dataset.action!, event.currentTarget as HTMLElement);
    });
  });

  document.querySelectorAll<HTMLDetailsElement>("details.run").forEach((details) => {
    details.addEventListener("toggle", () => {
      const runId = details.dataset.runId;
      if (!runId) return;
      if (details.open) {
        state.openRunIds.add(runId);
      } else {
        state.openRunIds.delete(runId);
      }
    });
  });

  const form = document.querySelector<HTMLFormElement>("#routine-form");
  form?.addEventListener("click", (event) => {
    const button = (event.target as HTMLElement | null)?.closest("[data-schedule-preset]") as HTMLButtonElement | null;
    if (!button) return;
    event.preventDefault();
    applySchedulePreset(form, button.dataset.schedulePreset || "");
    const draft = routineFromForm(form);
    state.formDraft = draft;
    render();
    queueSchedulePreview(draft);
  });
  form?.addEventListener("input", (event) => {
    state.formDraft = routineFromForm(form);
    if (isSchedulePreviewField(event.target)) {
      queueSchedulePreview(state.formDraft);
    }
  });
  form?.addEventListener("change", (event) => {
    if ((event.target as HTMLInputElement | HTMLSelectElement).name === "schedule_days") {
      ensureAtLeastOneScheduleDay(form, event.target as HTMLInputElement);
    }
    const draft = routineFromForm(form);
    if ((event.target as HTMLInputElement | HTMLSelectElement).name === "runner") {
      applyRunnerDefaults(draft);
      state.formDraft = draft;
      render();
      queueSchedulePreview(draft);
      return;
    }
    if ((event.target as HTMLInputElement | HTMLSelectElement).name === "schedule_custom_enabled") {
      state.formDraft = draft;
      render();
      queueSchedulePreview(draft);
      return;
    }
    state.formDraft = draft;
    if (isSchedulePreviewField(event.target)) {
      queueSchedulePreview(draft);
    }
  });
  form?.addEventListener("submit", async (event) => {
    event.preventDefault();
    await saveRoutineFromForm(event.currentTarget as HTMLFormElement);
  });
}

function applySchedulePreset(form: HTMLFormElement, presetId: string) {
  const preset = DAY_PRESETS.find((item) => item.id === presetId);
  if (!preset) return;
  form.querySelectorAll<HTMLInputElement>('input[name="schedule_days"]').forEach((input) => {
    input.checked = preset.days.includes(input.value);
  });
}

function ensureAtLeastOneScheduleDay(form: HTMLFormElement, fallback?: HTMLInputElement) {
  const inputs = Array.from(form.querySelectorAll<HTMLInputElement>('input[name="schedule_days"]'));
  if (!inputs.length || inputs.some((input) => input.checked)) return;
  const nextChecked = inputs.find((input) => input.value === fallback?.value) ?? inputs[0];
  nextChecked.checked = true;
}

async function handleAction(action: string, element: HTMLElement) {
  const routine = selectedRoutine();
  const guarded = [
    "toggle-pause",
    "toggle-selected-pause",
    "run",
    "cancel-run",
    "delete-routine",
    "save-raw",
    "refresh-runners",
  ].includes(action);
  if (guarded && state.busy) return;
  if (guarded) state.busy = true;
  try {
    if (action === "new-routine") {
      state.formDraft = newRoutine();
      state.mode = "new";
      render();
      queueSchedulePreview(state.formDraft);
    } else if (action === "edit-routine") {
      if (routine) state.formDraft = { ...routine };
      state.mode = "edit";
      render();
      if (state.formDraft) queueSchedulePreview(state.formDraft);
    } else if (action === "cancel-edit") {
      state.formDraft = undefined;
      state.schedulePreview = undefined;
      state.mode = "details";
      render();
    } else if (action === "toggle-pause") {
      const id = element.dataset.id!;
      const target = state.snapshot!.config.routines.find((item) => item.id === id)!;
      await invoke("set_routine_paused", { routineId: id, paused: !target.paused });
      await loadSnapshot();
    } else if (action === "toggle-selected-pause" && routine?.id) {
      await invoke("set_routine_paused", { routineId: routine.id, paused: !routine.paused });
      await loadSnapshot();
    } else if (action === "run" && routine?.id) {
      if (activeRun()) {
        setError("Cancel the active run before starting another one.");
        render({ preserveScroll: true });
        return;
      }
      await invoke("run_routine", { routineId: routine.id });
      await loadSnapshot();
    } else if (action === "cancel-run" && routine?.id) {
      await invoke("cancel_routine", { routineId: routine.id });
      await loadSnapshot();
    } else if (action === "delete-routine" && routine?.id) {
      if (confirm(`Delete "${routine.title}" and its stored runs?`)) {
        await invoke("delete_routine", { routineId: routine.id });
        state.selectedRoutineId = undefined;
        await loadSnapshot(false);
      }
    } else if (action === "open-raw") {
      state.rawText = await invoke<string>("get_raw_config");
      state.rawOpen = true;
      render();
    } else if (action === "close-raw") {
      state.rawOpen = false;
      render();
    } else if (action === "reload-raw") {
      state.rawText = await invoke<string>("get_raw_config");
      render();
    } else if (action === "save-raw") {
      const raw = document.querySelector<HTMLTextAreaElement>("#raw-config")!.value;
      await invoke("save_raw_config", { raw });
      state.rawOpen = false;
      await loadSnapshot();
    } else if (action === "refresh-runners") {
      await invoke("refresh_runner_capabilities");
      await loadSnapshot();
    } else if (action === "choose-cwd") {
      const form = document.querySelector<HTMLFormElement>("#routine-form");
      const input = form?.elements.namedItem("cwd") as HTMLInputElement | null;
      const selected = await invoke<string | null>("choose_working_directory", {
        initial: input?.value || null,
      });
      if (form && input && selected) {
        input.value = selected;
        state.formDraft = routineFromForm(form);
        render({ preserveScroll: true });
        queueSchedulePreview(state.formDraft);
      }
    } else if (action === "copy-run") {
      const run = state.runs.find((item) => item.id === element.dataset.runId);
      if (!run) return;
      await copyText(copyPayloadForRun(run).text);
      state.copiedRunId = run.id;
      render({ preserveScroll: true });
      window.setTimeout(() => {
        if (state.copiedRunId === run.id) {
          state.copiedRunId = undefined;
          render({ preserveScroll: true });
        }
      }, 1_500);
    }
  } catch (error) {
    setError(error);
    render({ preserveScroll: true });
  } finally {
    if (guarded) state.busy = false;
  }
}

async function saveRoutineFromForm(form: HTMLFormElement) {
  if (state.busy) return;
  state.busy = true;
  const routine = routineFromForm(form);
  state.formDraft = routine;
  const existingIds = new Set((state.snapshot?.config.routines ?? []).map((item) => item.id).filter(Boolean));

  try {
    const config = await invoke<AppConfig>("save_routine", { routine });
    const savedId =
      routine.id ||
      config.routines.find((item) => item.id && !existingIds.has(item.id))?.id ||
      state.selectedRoutineId;
    state.selectedRoutineId = savedId ?? undefined;
    state.formDraft = undefined;
    state.mode = "details";
    await loadSnapshot();
  } catch (error) {
    setError(error);
    render({ preserveScroll: true });
  } finally {
    state.busy = false;
  }
}

function routineFromForm(form: HTMLFormElement): RoutineConfig {
  const data = new FormData(form);
  const timeoutRaw = String(data.get("timeout_seconds") || "").trim();
  const scheduleTime = String(data.get("schedule_time") || "");
  const customScheduleEnabled = data.get("schedule_custom_enabled") === "on";
  const currentSchedule = state.formDraft?.schedule || selectedRoutine()?.schedule || "0 7 * * *";
  const currentControls = parseScheduleControls(currentSchedule);
  const schedule =
    !customScheduleEnabled
      ? buildSimpleSchedule(scheduleDaysFromForm(data, currentControls.days), scheduleTime || currentControls.time)
      : String(data.get("schedule_custom") || currentSchedule);
  return {
    id: String(data.get("id") || "") || null,
    title: String(data.get("title") || ""),
    description: String(data.get("description") || ""),
    prompt: String(data.get("prompt") || ""),
    runner: String(data.get("runner") || ""),
    model: String(data.get("model") || "") || null,
    effort: String(data.get("effort") || "") || null,
    cwd: String(data.get("cwd") || ""),
    schedule,
    timezone: String(data.get("timezone") || "") || null,
    paused: data.get("paused") === "on",
    dangerous: data.get("dangerous") === "on",
    timeout_seconds: timeoutRaw ? Number(timeoutRaw) : null,
  };
}

function scheduleDaysFromForm(data: FormData, fallbackDays: string[]) {
  const selected = data.getAll("schedule_days").map(String);
  const days = orderDays(selected);
  return days.length ? days : fallbackDays;
}

function applyRunnerDefaults(routine: RoutineConfig) {
  const runner = state.snapshot?.config.runners.find((item) => item.id === routine.runner);
  if (isScriptLikeRunner(runner)) {
    routine.model = null;
    routine.effort = null;
    routine.dangerous = false;
    return;
  }
  routine.model = runner?.default_model ?? null;
  routine.effort = runner?.default_effort ?? null;
}

function isSchedulePreviewField(target: EventTarget | null) {
  const name = (target as HTMLInputElement | HTMLSelectElement | null)?.name;
  return (
    name === "schedule_days" ||
    name === "schedule_custom_enabled" ||
    name === "schedule_time" ||
    name === "schedule_custom" ||
    name === "timezone"
  );
}

function copyPayloadForRun(run: RunRecord) {
  const commandName = commandBasename(run.command[0] || "");
  const output = `${run.stderr}\n${run.stdout}`;
  const sessionId = extractSessionId(output);
  const chatId = extractChatId(output);
  const cwdPrefix = run.cwd ? `cd ${shellQuote(run.cwd)} && ` : "";

  if (commandName === "codex" && sessionId) {
    return {
      label: "Copy resume",
      title: "Copy Codex resume command",
      text: `${cwdPrefix}codex resume --include-non-interactive ${shellQuote(sessionId)}`,
    };
  }

  if (commandName === "claude" && sessionId) {
    return {
      label: "Copy resume",
      title: "Copy Claude resume command",
      text: `${cwdPrefix}claude --resume ${shellQuote(sessionId)}`,
    };
  }

  if ((commandName === "cursor-agent" || commandName === "agent") && chatId) {
    const command = run.command[0] || "cursor-agent";
    return {
      label: "Copy resume",
      title: "Copy Cursor Agent resume command",
      text: `${cwdPrefix}${shellQuote(command)} --resume ${shellQuote(chatId)}`,
    };
  }

  return {
    label: "Copy command",
    title: "Copy original run command",
    text: `${cwdPrefix}${shellJoin(run.command)}`,
  };
}

function extractSessionId(output: string) {
  return output.match(/session[\s_-]*id["'\s:=]+([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})/i)?.[1];
}

function extractChatId(output: string) {
  return output.match(/chat[\s_-]*id["'\s:=]+([A-Za-z0-9_-]+)/i)?.[1];
}

function commandBasename(command: string) {
  return command.split("/").filter(Boolean).at(-1) ?? command;
}

function shellJoin(args: string[]) {
  return args.map(shellQuote).join(" ");
}

function shellQuote(value: string) {
  if (/^[A-Za-z0-9_/:.,@%+=-]+$/.test(value)) return value;
  return `'${value.replaceAll("'", "'\"'\"'")}'`;
}

async function copyText(text: string) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }

  const textarea = document.createElement("textarea");
  textarea.value = text;
  textarea.style.position = "fixed";
  textarea.style.left = "-9999px";
  document.body.append(textarea);
  textarea.focus();
  textarea.select();
  const copied = document.execCommand("copy");
  textarea.remove();
  if (!copied) throw new Error("clipboard copy failed");
}

function escapeHtml(value: unknown) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function escapeAttribute(value: unknown) {
  return escapeHtml(value).replaceAll(" ", "_");
}

function selectorEscape(value: string) {
  const css = globalThis.CSS as { escape?: (text: string) => string } | undefined;
  if (css?.escape) return css.escape(value);
  return value.replace(/["\\]/g, "\\$&");
}

loadSnapshot(false);
setTimeout(() => loadSnapshot(true, { preserveScroll: true }), 1_000);
setTimeout(() => loadSnapshot(true, { preserveScroll: true }), 8_000);
setInterval(() => {
  if (!state.rawOpen && state.mode === "details") {
    loadSnapshot(true, { preserveScroll: true, skipIfUnchanged: true }, false);
  }
}, 3_000);
