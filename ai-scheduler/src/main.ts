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

type Snapshot = {
  config_path: string;
  db_path: string;
  config: AppConfig;
  runner_capabilities: RunnerCapability[];
};

type State = {
  snapshot?: Snapshot;
  selectedRoutineId?: string;
  runs: RunRecord[];
  query: string;
  mode: "details" | "edit" | "new";
  rawOpen: boolean;
  rawText: string;
  error?: string;
  busy: boolean;
};

const state: State = {
  runs: [],
  query: "",
  mode: "details",
  rawOpen: false,
  rawText: "",
  busy: false,
};

const app = document.querySelector<HTMLDivElement>("#app")!;

async function loadSnapshot(keepSelection = true) {
  state.error = undefined;
  try {
    state.snapshot = await invoke<Snapshot>("get_snapshot");
    const routines = state.snapshot.config.routines;
    if (!keepSelection || !state.selectedRoutineId || !routines.some((r) => r.id === state.selectedRoutineId)) {
      state.selectedRoutineId = routines[0]?.id ?? undefined;
    }
    await loadRuns();
  } catch (error) {
    state.error = String(error);
  }
  render();
}

async function loadRuns() {
  if (!state.selectedRoutineId) {
    state.runs = [];
    return;
  }
  state.runs = await invoke<RunRecord[]>("list_runs", { routineId: state.selectedRoutineId });
}

function selectedRoutine(): RoutineConfig | undefined {
  return state.snapshot?.config.routines.find((routine) => routine.id === state.selectedRoutineId);
}

function runnerFor(routine: RoutineConfig | undefined): RunnerConfig | undefined {
  if (!routine) return undefined;
  return state.snapshot?.config.runners.find((runner) => runner.id === routine.runner);
}

function capabilityFor(runnerId: string): RunnerCapability | undefined {
  return state.snapshot?.runner_capabilities.find((runner) => runner.id === runnerId);
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
  return `${routine.schedule} · ${timezone}`;
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

function render() {
  const snapshot = state.snapshot;
  if (!snapshot) {
    app.innerHTML = `<main class="shell"><div class="empty">Loading</div></main>`;
    return;
  }

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
        ${state.error ? `<div class="banner">${escapeHtml(state.error)}</div>` : ""}
        ${routine ? renderDetail(routine, runner, capability) : renderEmptyDetail()}
      </section>
      ${state.rawOpen ? renderRawPanel() : ""}
    </main>
  `;

  wireEvents();
}

function renderRoutineSection(title: string, routines: RoutineConfig[]) {
  return `
    <section class="routine-section">
      <div class="section-head">
        <h2>${title}</h2>
        ${title === "Current" ? `<button class="icon-button" data-action="new-routine" title="New routine">+</button>` : ""}
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
        <span class="routine-schedule">${routine.paused ? escapeHtml(scheduleLabel(routine)) : `Next · ${escapeHtml(scheduleLabel(routine))}`}</span>
      </span>
    </button>
  `;
}

function renderRunnerStatus(runner: RunnerCapability) {
  const status = runner.available ? "ok" : "bad";
  return `
    <div class="runner-status">
      <span class="runner-light ${status}"></span>
      <span>
        <strong>${escapeHtml(runner.label)}</strong>
        <small>${escapeHtml(runner.version || runner.error || "Not available")}</small>
      </span>
    </div>
  `;
}

function renderDetail(routine: RoutineConfig, runner?: RunnerConfig, capability?: RunnerCapability) {
  if (state.mode === "edit") return renderRoutineForm(routine);
  if (state.mode === "new") return renderRoutineForm(newRoutine());
  const latest = state.runs[0];
  return `
    <div class="detail-toolbar">
      <button class="primary" data-action="run">▷ Run now</button>
      <button data-action="toggle-selected-pause">${routine.paused ? "Resume" : "Pause"}</button>
      <button data-action="edit-routine">Edit</button>
      <button class="danger" data-action="delete-routine">Delete</button>
    </div>
    <article class="routine-detail">
      <h1>${escapeHtml(routine.title)}</h1>
      <p>${escapeHtml(routine.description || "No description")}</p>
      <pre class="prompt">${escapeHtml(routine.prompt)}</pre>
      <dl class="meta-grid">
        <div><dt>Status</dt><dd>${routine.paused ? "Paused" : "Active"}</dd></div>
        <div><dt>Runner</dt><dd>${escapeHtml(runner?.label ?? routine.runner)}</dd></div>
        <div><dt>Available</dt><dd>${capability?.available ? "Yes" : "No"}</dd></div>
        <div><dt>Model</dt><dd>${escapeHtml(routine.model || runner?.default_model || "—")}</dd></div>
        <div><dt>Effort</dt><dd>${escapeHtml(routine.effort || runner?.default_effort || "—")}</dd></div>
        <div><dt>Dangerous</dt><dd>${routine.dangerous ? "On" : "Off"}</dd></div>
        <div><dt>Working directory</dt><dd>${escapeHtml(routine.cwd)}</dd></div>
        <div><dt>Schedule</dt><dd>${escapeHtml(scheduleLabel(routine))}</dd></div>
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
  return `
    <details class="run">
      <summary>
        <span class="${statusClass(run.status)}">${run.status.replace("_", " ")}</span>
        <span>${formatDate(run.started_at || run.scheduled_for)}</span>
        <span>${escapeHtml(run.cancel_reason || "")}</span>
      </summary>
      <div class="run-body">
        <div class="command">${escapeHtml(run.command.join(" "))}</div>
        <div class="output-grid">
          <section>
            <h3>stdout${run.stdout_truncated ? " · truncated" : ""}</h3>
            <pre>${escapeHtml(run.stdout || "")}</pre>
          </section>
          <section>
            <h3>stderr${run.stderr_truncated ? " · truncated" : ""}</h3>
            <pre>${escapeHtml(run.stderr || "")}</pre>
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
  return `
    <form class="routine-form" id="routine-form">
      <div class="detail-toolbar">
        <button class="primary" type="submit">Save</button>
        <button type="button" data-action="cancel-edit">Cancel</button>
      </div>
      <input type="hidden" name="id" value="${escapeHtml(routine.id || "")}" />
      <label>Title<input name="title" value="${escapeHtml(routine.title)}" required /></label>
      <label>Description<textarea name="description">${escapeHtml(routine.description)}</textarea></label>
      <label>Prompt<textarea name="prompt" class="prompt-input" required>${escapeHtml(routine.prompt)}</textarea></label>
      <div class="form-grid">
        <label>Runner<select name="runner">${config.runners.map((item) => optionHtml(item.id, item.label, routine.runner)).join("")}</select></label>
        <label>Model<select name="model">${models.map((item) => optionHtml(item.value, item.label, routine.model || runner?.default_model)).join("")}</select></label>
        <label>Effort<select name="effort"><option value="">—</option>${efforts.map((item) => optionHtml(item.value, item.label, routine.effort || runner?.default_effort)).join("")}</select></label>
        <label>Schedule<input name="schedule" value="${escapeHtml(routine.schedule)}" required /></label>
        <label>Timezone<input name="timezone" value="${escapeHtml(routine.timezone || config.settings.timezone)}" required /></label>
        <label>Timeout seconds<input name="timeout_seconds" type="number" min="1" value="${timeoutSeconds}" /></label>
      </div>
      <label>Working directory<input name="cwd" value="${escapeHtml(routine.cwd)}" required /></label>
      <div class="toggles">
        <label><input type="checkbox" name="paused" ${routine.paused ? "checked" : ""} /> Paused</label>
        <label><input type="checkbox" name="dangerous" ${routine.dangerous ? "checked" : ""} /> Yolo</label>
      </div>
    </form>
  `;
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
    schedule: "0 7 * * Sat",
    timezone: config.settings.timezone,
    paused: true,
    dangerous: false,
    timeout_seconds: config.settings.default_timeout_seconds,
  };
}

function optionHtml(value: string, label: string, selected?: string | null) {
  return `<option value="${escapeHtml(value)}" ${value === selected ? "selected" : ""}>${escapeHtml(label)}</option>`;
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
      await loadRuns();
      render();
    });
  });

  document.querySelectorAll<HTMLElement>("[data-action]").forEach((element) => {
    element.addEventListener("click", async (event) => {
      event.preventDefault();
      await handleAction((event.currentTarget as HTMLElement).dataset.action!, event.currentTarget as HTMLElement);
    });
  });

  document.querySelector<HTMLFormElement>("#routine-form")?.addEventListener("submit", async (event) => {
    event.preventDefault();
    await saveRoutineFromForm(event.currentTarget as HTMLFormElement);
  });
}

async function handleAction(action: string, element: HTMLElement) {
  const routine = selectedRoutine();
  try {
    if (action === "new-routine") {
      state.mode = "new";
      render();
    } else if (action === "edit-routine") {
      state.mode = "edit";
      render();
    } else if (action === "cancel-edit") {
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
      await invoke("run_routine", { routineId: routine.id });
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
    }
  } catch (error) {
    state.error = String(error);
    render();
  }
}

async function saveRoutineFromForm(form: HTMLFormElement) {
  const data = new FormData(form);
  const timeoutRaw = String(data.get("timeout_seconds") || "").trim();
  const routine: RoutineConfig = {
    id: String(data.get("id") || "") || null,
    title: String(data.get("title") || ""),
    description: String(data.get("description") || ""),
    prompt: String(data.get("prompt") || ""),
    runner: String(data.get("runner") || ""),
    model: String(data.get("model") || "") || null,
    effort: String(data.get("effort") || "") || null,
    cwd: String(data.get("cwd") || ""),
    schedule: String(data.get("schedule") || ""),
    timezone: String(data.get("timezone") || "") || null,
    paused: data.get("paused") === "on",
    dangerous: data.get("dangerous") === "on",
    timeout_seconds: timeoutRaw ? Number(timeoutRaw) : null,
  };

  try {
    await invoke("save_routine", { routine });
    state.mode = "details";
    await loadSnapshot();
    if (routine.id) state.selectedRoutineId = routine.id;
  } catch (error) {
    state.error = String(error);
    render();
  }
}

function escapeHtml(value: unknown) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

loadSnapshot(false);
setTimeout(() => loadSnapshot(), 1_000);
setTimeout(() => loadSnapshot(), 8_000);
setInterval(() => {
  if (!state.rawOpen && state.mode === "details") {
    loadRuns()
      .then(render)
      .catch((error) => {
        state.error = String(error);
        render();
      });
  }
}, 3_000);
