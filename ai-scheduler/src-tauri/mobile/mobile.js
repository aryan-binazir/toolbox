const app = document.querySelector("#app");

const state = {
  snapshot: null,
  selectedId: null,
  runs: [],
  runsFor: null,
  mode: "details",
  draft: null,
  busy: false,
  error: "",
};

const mutationHeaders = {
  "Content-Type": "application/json",
  "X-AI-Scheduler-Mobile": "1",
};

loadSnapshot(false);
setInterval(() => {
  if (!state.busy && state.mode === "details") loadSnapshot(true, true);
}, 8000);
document.addEventListener("visibilitychange", () => {
  if (!document.hidden && state.mode === "details") loadSnapshot(true, true);
});

app.addEventListener("click", async (event) => {
  const button = event.target.closest("[data-action]");
  if (!button) return;
  const action = button.dataset.action;
  const id = button.dataset.id;
  const routine = id ? routineById(id) : selectedRoutine();

  try {
    if (action === "select" && id) {
      state.selectedId = id;
      state.mode = "details";
      state.draft = null;
      state.runs = [];
      state.runsFor = null;
      render();
      await loadRuns(id);
    } else if (action === "refresh") {
      await loadSnapshot(true);
    } else if (action === "refresh-runners") {
      await mutate("/api/runners/refresh");
      await loadSnapshot(true);
    } else if (action === "refresh-runs" && routine) {
      await loadRuns(routine.id);
    } else if (action === "new-routine") {
      state.mode = "new";
      state.draft = newRoutine();
      render();
    } else if (action === "edit-routine" && routine) {
      state.mode = "edit";
      state.draft = { ...routine };
      render();
    } else if (action === "cancel-edit") {
      state.mode = "details";
      state.draft = null;
      render();
    } else if (action === "pause" && routine) {
      await mutate(`/api/routines/${encodeURIComponent(routine.id)}/pause`, {
        paused: !routine.paused,
      });
      await loadSnapshot(true);
    } else if (action === "run" && routine) {
      if (!confirm(`Run "${routine.title}" now?`)) return;
      await mutate(`/api/routines/${encodeURIComponent(routine.id)}/run`);
      await loadSnapshot(true);
    } else if (action === "cancel" && routine) {
      if (!confirm(`Cancel "${routine.title}"?`)) return;
      await mutate(`/api/routines/${encodeURIComponent(routine.id)}/cancel`);
      await loadSnapshot(true);
    } else if (action === "delete-routine" && routine) {
      if (!confirm(`Delete "${routine.title}" and its stored runs?`)) return;
      await mutate(`/api/routines/${encodeURIComponent(routine.id)}/delete`);
      state.selectedId = null;
      state.runs = [];
      state.runsFor = null;
      await loadSnapshot(false);
    }
  } catch (error) {
    setError(error);
  }
});

app.addEventListener("change", (event) => {
  if (!(event.target instanceof HTMLSelectElement)) return;
  if (event.target.name !== "runner" || !state.draft) return;
  const runner = runnerById(event.target.value);
  if (!runner) return;
  state.draft.runner_id = runner.id;
  state.draft.runner_label = runner.label;
  state.draft.model = runner.default_model || runner.models?.[0]?.value || "";
  state.draft.effort = runner.default_effort || runner.efforts?.[0]?.value || "";
  state.draft.timeout_seconds = runner.default_timeout_seconds || state.draft.timeout_seconds || null;
  render();
});

app.addEventListener("submit", async (event) => {
  const form = event.target.closest("#routine-form");
  if (!form) return;
  event.preventDefault();
  try {
    const routine = routineFromForm(form);
    await mutate("/api/routines", routine);
    state.selectedId = routine.id;
    state.mode = "details";
    state.draft = null;
    await loadSnapshot(true);
  } catch (error) {
    setError(error);
  }
});

async function loadSnapshot(preserveSelection = true, quiet = false) {
  if (!quiet) setBusy(true);
  try {
    const snapshot = await requestJson("/api/snapshot");
    state.snapshot = snapshot;
    const routines = snapshot.routines || [];
    if (
      !preserveSelection ||
      !state.selectedId ||
      !routines.some((routine) => routine.id === state.selectedId)
    ) {
      state.selectedId = routines[0]?.id || null;
      state.runs = [];
      state.runsFor = null;
    }
    state.error = "";
    render();
    if (state.selectedId && state.runsFor !== state.selectedId && state.mode === "details") {
      await loadRuns(state.selectedId, true);
    }
  } catch (error) {
    setError(error);
  } finally {
    if (!quiet) setBusy(false);
  }
}

async function loadRuns(routineId, quiet = false) {
  if (!routineId) return;
  if (!quiet) setBusy(true);
  try {
    const result = await requestJson(`/api/routines/${encodeURIComponent(routineId)}/runs`);
    state.runs = result.runs || [];
    state.runsFor = routineId;
    state.error = "";
    render();
  } catch (error) {
    setError(error);
  } finally {
    if (!quiet) setBusy(false);
  }
}

async function requestJson(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    headers: {
      ...(options.headers || {}),
    },
  });
  if (!response.ok) {
    let message = `${response.status} ${response.statusText}`;
    try {
      const body = await response.json();
      if (body.error) message = body.error;
    } catch (_) {
      // Keep the HTTP status fallback.
    }
    throw new Error(message);
  }
  return response.json();
}

async function mutate(path, body) {
  return requestJson(path, {
    method: "POST",
    headers: mutationHeaders,
    body: body === undefined ? undefined : JSON.stringify(body),
  });
}

function render() {
  const snapshot = state.snapshot;
  if (!snapshot) {
    app.innerHTML = `
      <main class="boot">
        <span class="boot-mark"></span>
        <span>Loading AI Scheduler</span>
      </main>
    `;
    return;
  }

  const renderSnapshot = captureRenderSnapshot();
  const routines = snapshot.routines || [];
  const activeCount = routines.filter((routine) => routine.active_run).length;
  const pausedCount = routines.filter((routine) => routine.paused).length;

  app.innerHTML = `
    <main class="app">
      <header class="topbar">
        <div class="brand">
          <h1>AI Scheduler</h1>
          <p>${routines.length} routines - ${activeCount} running - ${pausedCount} paused</p>
        </div>
        <button class="refresh" data-action="refresh" ${state.busy ? "disabled" : ""}>Refresh</button>
      </header>
      ${state.error ? `<div class="alert">${escapeHtml(state.error)}</div>` : ""}
      <div class="layout">
        <aside class="sidebar">
          <div class="sidebar-actions">
            <button class="primary" data-action="new-routine">New routine</button>
          </div>
          ${renderRoutineSection("Current", routines.filter((routine) => !routine.paused))}
          ${renderRoutineSection("Paused", routines.filter((routine) => routine.paused))}
          ${renderRunnerPanel(snapshot.runners || [])}
        </aside>
        <section class="detail">
          ${renderDetailPane()}
        </section>
      </div>
      ${renderSidePanel()}
    </main>
  `;
  restoreRenderSnapshot(renderSnapshot);
}

function captureRenderSnapshot() {
  return {
    windowX: window.scrollX,
    windowY: window.scrollY,
    openRunIds: Array.from(app.querySelectorAll("details.run[data-run-id][open]")).map(
      (element) => element.dataset.runId,
    ),
    runOutputPositions: Array.from(app.querySelectorAll("details.run[data-run-id] pre[data-run-output]")).map(
      (element) => ({
        runId: element.closest("details.run")?.dataset.runId || "",
        stream: element.dataset.runOutput || "",
        top: element.scrollTop,
        left: element.scrollLeft,
      }),
    ),
  };
}

function restoreRenderSnapshot(snapshot) {
  for (const runId of snapshot.openRunIds) {
    const details = app.querySelector(`details.run[data-run-id="${selectorEscape(runId)}"]`);
    if (details) details.open = true;
  }
  for (const position of snapshot.runOutputPositions) {
    if (!position.runId || !position.stream) continue;
    const output = app.querySelector(
      `details.run[data-run-id="${selectorEscape(position.runId)}"] pre[data-run-output="${position.stream}"]`,
    );
    if (output) {
      output.scrollTop = position.top;
      output.scrollLeft = position.left;
    }
  }
  window.scrollTo(snapshot.windowX, snapshot.windowY);
}

function renderRoutineSection(title, routines) {
  return `
    <section class="routine-section">
      <div class="section-head">
        <h2>${escapeHtml(title)}</h2>
        <span>${routines.length}</span>
      </div>
      <div class="routines">
        ${routines.length ? routines.map(renderRoutineButton).join("") : `<div class="empty">No ${title.toLowerCase()} routines</div>`}
      </div>
    </section>
  `;
}

function renderRunnerPanel(runners) {
  return `
    <section class="runner-panel">
      <div class="section-head">
        <h2>Runners</h2>
        <button class="small" data-action="refresh-runners" ${state.busy ? "disabled" : ""}>Refresh</button>
      </div>
      <div class="runner-list">
        ${runners.length ? runners.map(renderRunner).join("") : `<div class="empty">No runners configured</div>`}
      </div>
    </section>
  `;
}

function renderRunner(runner) {
  return `
    <div class="runner-status">
      <span class="status-light ${runner.available ? "ok" : "bad"}"></span>
      <span>
        <strong>${escapeHtml(runner.label)}</strong>
        <small>${runner.available ? "Version check passed" : "Version check failed"}</small>
      </span>
    </div>
  `;
}

function renderRoutineButton(routine) {
  const selected = routine.id === state.selectedId ? " selected" : "";
  const stateClass = routine.dangerous ? " dangerous" : routine.paused ? " paused" : "";
  const stateLabel = routine.active_run ? "Running" : routine.dangerous ? "Dangerous" : "";
  const stateLabelClass = routine.active_run ? "running" : routine.dangerous ? "dangerous" : "";

  return `
    <button class="routine-button${selected}${stateClass}" data-action="select" data-id="${escapeAttribute(routine.id)}">
      <span class="routine-rail"></span>
      <span class="routine-copy">
        <span class="routine-title">${escapeHtml(routine.title)}</span>
        <span class="routine-meta">${escapeHtml(routine.project_label)} - ${escapeHtml(routine.runner_label)}</span>
        ${stateLabel ? `<span class="routine-state ${stateLabelClass}">${escapeHtml(stateLabel)}</span>` : ""}
      </span>
    </button>
  `;
}

function renderDetailPane() {
  const routine = selectedRoutine();
  return routine ? renderDetail(routine) : `<div class="empty">No routine selected</div>`;
}

function renderSidePanel() {
  if (state.mode !== "new" && state.mode !== "edit") return "";
  return `
    <div class="side-panel-layer">
      <button class="side-panel-scrim" data-action="cancel-edit" aria-label="Close editor"></button>
      <aside class="side-panel" aria-label="${state.mode === "new" ? "New routine" : "Edit routine"}">
        <header class="side-panel-head">
          <h2>${state.mode === "new" ? "New routine" : "Edit routine"}</h2>
          <button type="button" data-action="cancel-edit">Close</button>
        </header>
        ${renderRoutineForm(state.draft || newRoutine())}
      </aside>
    </div>
  `;
}

function renderDetail(routine) {
  const active = routine.active_run;
  const runDisabled = state.busy || active;
  const cancelDisabled = state.busy || !active;
  const description = routine.description
    ? `<p class="description">${escapeHtml(routine.description)}</p>`
    : "";
  const latest = routine.latest_run;

  return `
    <div class="detail-head">
      <h2>${escapeHtml(routine.title)}</h2>
      ${description}
    </div>
    <div class="actions">
      ${
        active
          ? `<button class="danger wide" data-action="cancel" data-id="${escapeAttribute(routine.id)}" ${cancelDisabled ? "disabled" : ""}>Cancel run</button>`
          : `<button class="primary wide" data-action="run" data-id="${escapeAttribute(routine.id)}" ${runDisabled ? "disabled" : ""}>Run now</button>`
      }
      <button data-action="pause" data-id="${escapeAttribute(routine.id)}" ${state.busy ? "disabled" : ""}>${routine.paused ? "Resume" : "Pause"}</button>
      <button data-action="edit-routine" data-id="${escapeAttribute(routine.id)}" ${state.busy ? "disabled" : ""}>Edit</button>
      <button data-action="refresh-runs" data-id="${escapeAttribute(routine.id)}" ${state.busy ? "disabled" : ""}>Reload runs</button>
      <button class="danger" data-action="delete-routine" data-id="${escapeAttribute(routine.id)}" ${state.busy ? "disabled" : ""}>Delete</button>
    </div>
    <div class="facts">
      ${fact("Status", routineStatus(routine))}
      ${fact("Next", routine.paused ? "Paused" : formatDate(routine.next_run_at) || "None")}
      ${fact("Runner", `${routine.runner_label}${routine.runner_available ? "" : " unavailable"}`)}
      ${fact("Project", routine.project_label)}
      ${fact("Schedule", routine.schedule_error || routine.schedule)}
      ${fact("Latest", latest ? `${statusText(latest.status)} - ${formatDate(latest.started_at || latest.scheduled_for)}` : "No runs")}
    </div>
    <div class="section-title">
      <h3>Runs</h3>
      <button data-action="refresh-runs" data-id="${escapeAttribute(routine.id)}" ${state.busy ? "disabled" : ""}>Refresh</button>
    </div>
    <div class="runs">
      ${state.runsFor === routine.id && state.runs.length ? state.runs.map(renderRun).join("") : `<div class="empty">No run history loaded</div>`}
    </div>
  `;
}

function renderRoutineForm(routine) {
  const runner = runnerById(routine.runner_id) || state.snapshot?.runners?.[0] || null;
  const models = runner?.models || [];
  const efforts = runner?.efforts || [];

  return `
    <form id="routine-form" class="routine-form">
      <input type="hidden" name="id" value="${escapeAttribute(routine.id || "")}" />
      <label>Title<input name="title" value="${escapeAttribute(routine.title || "")}" required /></label>
      <label>Description<textarea name="description">${escapeHtml(routine.description || "")}</textarea></label>
      <label>Prompt<textarea class="prompt-input" name="prompt" required>${escapeHtml(routine.prompt || "")}</textarea></label>
      <div class="form-grid">
        <label>Runner<select name="runner">${(state.snapshot?.runners || []).map((item) => optionHtml(item.id, item.label, routine.runner_id)).join("")}</select></label>
        <label>Model<select name="model">${models.map((item) => optionHtml(item.value, item.label, routine.model)).join("")}</select></label>
        <label>Effort<select name="effort">${efforts.length ? efforts.map((item) => optionHtml(item.value, item.label, routine.effort)).join("") : `<option value="">None</option>`}</select></label>
      </div>
      <label>Working directory<input name="cwd" value="${escapeAttribute(routine.cwd || "")}" required /></label>
      <div class="form-grid">
        <label>Schedule<input name="schedule" value="${escapeAttribute(routine.schedule || "")}" required /></label>
        <label>Timezone<input name="timezone" value="${escapeAttribute(routine.timezone || state.snapshot?.timezone || "UTC")}" /></label>
        <label>Timeout seconds<input type="number" min="1" name="timeout_seconds" value="${escapeAttribute(routine.timeout_seconds || "")}" /></label>
      </div>
      <div class="check-row">
        <label><input type="checkbox" name="paused" ${routine.paused ? "checked" : ""} /> Paused</label>
        <label><input type="checkbox" name="dangerous" ${routine.dangerous ? "checked" : ""} /> Dangerous</label>
      </div>
      <div class="form-actions">
        <button type="button" data-action="cancel-edit">Cancel</button>
        <button class="primary" type="submit" ${state.busy ? "disabled" : ""}>Save routine</button>
      </div>
    </form>
  `;
}

function renderRun(run) {
  const statusClassName = statusClass(run.status);
  const when = formatDate(run.started_at || run.scheduled_for || run.finished_at);
  const stdout = run.stdout_preview || "";
  const stderr = run.stderr_preview || "";
  return `
    <details class="run" data-run-id="${escapeAttribute(run.id)}">
      <summary>
        <span class="run-title">
          <strong>${escapeHtml(when || "Unstarted")}</strong>
          <span class="run-meta">${escapeHtml(run.id)}</span>
        </span>
        <span class="run-status ${statusClassName}">${escapeHtml(statusText(run.status))}</span>
      </summary>
      <div class="run-output">
        <div class="stream">
          <h4>stderr${run.stderr_truncated ? " - capped" : ""}</h4>
          <pre data-run-output="stderr">${escapeHtml(stderr || "No stderr")}</pre>
        </div>
        <div class="stream">
          <h4>stdout${run.stdout_truncated ? " - capped" : ""}</h4>
          <pre data-run-output="stdout">${escapeHtml(stdout || "No stdout")}</pre>
        </div>
      </div>
    </details>
  `;
}

function routineFromForm(form) {
  const data = new FormData(form);
  const id = String(data.get("id") || "").trim() || newRoutineId();
  const timeoutRaw = String(data.get("timeout_seconds") || "").trim();
  return {
    id,
    title: String(data.get("title") || "").trim(),
    description: String(data.get("description") || ""),
    prompt: String(data.get("prompt") || ""),
    runner: String(data.get("runner") || ""),
    model: nullableString(data.get("model")),
    effort: nullableString(data.get("effort")),
    cwd: String(data.get("cwd") || "").trim(),
    schedule: String(data.get("schedule") || "").trim(),
    timezone: nullableString(data.get("timezone")),
    paused: data.has("paused"),
    dangerous: data.has("dangerous"),
    timeout_seconds: timeoutRaw ? Number(timeoutRaw) : null,
  };
}

function newRoutine() {
  const runner = state.snapshot?.runners?.[0] || null;
  return {
    id: newRoutineId(),
    title: "",
    description: "",
    prompt: "",
    runner_id: runner?.id || "",
    runner_label: runner?.label || "",
    model: runner?.default_model || runner?.models?.[0]?.value || "",
    effort: runner?.default_effort || runner?.efforts?.[0]?.value || "",
    cwd: "",
    schedule: "0 7 * * Sat",
    timezone: state.snapshot?.timezone || "UTC",
    paused: false,
    dangerous: false,
    timeout_seconds: runner?.default_timeout_seconds || null,
  };
}

function newRoutineId() {
  return `rtn_mobile_${Date.now().toString(36)}`;
}

function optionHtml(value, label, selected) {
  return `<option value="${escapeAttribute(value)}" ${value === selected ? "selected" : ""}>${escapeHtml(label)}</option>`;
}

function nullableString(value) {
  const text = String(value || "").trim();
  return text ? text : null;
}

function fact(label, value) {
  return `
    <div class="key-value">
      <span>${escapeHtml(label)}</span>
      <strong>${escapeHtml(value || "--")}</strong>
    </div>
  `;
}

function routineStatus(routine) {
  if (routine.active_run) return statusText(routine.active_run.status);
  if (routine.paused) return "Paused";
  if (routine.dangerous) return "Dangerous";
  return "Active";
}

function selectedRoutine() {
  return routineById(state.selectedId);
}

function routineById(id) {
  return state.snapshot?.routines?.find((routine) => routine.id === id) || null;
}

function runnerById(id) {
  return state.snapshot?.runners?.find((runner) => runner.id === id) || null;
}

function setBusy(value) {
  state.busy = value;
  render();
}

function setError(error) {
  state.error = error instanceof Error ? error.message : String(error);
  state.busy = false;
  render();
}

function formatDate(value) {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return "";
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  }).format(date);
}

function statusText(status) {
  return String(status || "")
    .replace(/_/g, " ")
    .replace(/\b\w/g, (char) => char.toUpperCase());
}

function statusClass(status) {
  return String(status || "").replace(/_/g, "-");
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#39;");
}

function escapeAttribute(value) {
  return escapeHtml(value).replaceAll("`", "&#96;");
}

function selectorEscape(value) {
  if (window.CSS?.escape) return CSS.escape(String(value));
  return String(value).replace(/["\\]/g, "\\$&");
}
