import {
  applySchedulePreset,
  ensureAtLeastOneScheduleDay,
  isScheduleField,
  newRoutine,
  renderMobileApp,
  routineDraftFromForm,
  routineFromForm,
} from "/mobile-view.js?v=20260704-mobile-sidebar-shell-polish";

const app = document.querySelector("#app");

const state = {
  snapshot: null,
  selectedId: null,
  runs: [],
  runsFor: null,
  mode: "list",
  query: "",
  draft: null,
  busy: false,
  error: "",
  snapshotSequence: 0,
  runsSequence: 0,
};

const mutationHeaders = {
  "Content-Type": "application/json",
  "X-AI-Scheduler-Mobile": "1",
};

loadSnapshot(false);
setInterval(() => {
  if (!state.busy && (state.mode === "list" || state.mode === "detail")) loadSnapshot(true, true);
}, 8000);
document.addEventListener("visibilitychange", () => {
  if (!document.hidden && (state.mode === "list" || state.mode === "detail")) loadSnapshot(true, true);
});

app.addEventListener("input", (event) => {
  if (event.target instanceof HTMLInputElement && event.target.name === "search") {
    state.query = event.target.value;
    render();
  }
});

app.addEventListener("click", async (event) => {
  const presetButton = event.target.closest("[data-schedule-preset]");
  if (presetButton && state.draft) {
    const form = presetButton.closest("#routine-form");
    if (form instanceof HTMLFormElement) {
      applySchedulePreset(form, presetButton.dataset.schedulePreset || "");
      state.draft = draftFromCurrentForm(form);
      render();
    }
    return;
  }

  const button = event.target.closest("[data-action]");
  if (!button) return;
  const action = button.dataset.action;
  const id = button.dataset.id;
  const routine = id ? routineById(id) : selectedRoutine();
  const guarded = [
    "refresh",
    "pause",
    "run",
    "cancel",
    "delete-routine",
    "logout",
    "revoke-all-browsers",
  ].includes(action);
  if (guarded && state.busy) return;
  if (guarded) setBusy(true);

  try {
    if (action === "select" && id) {
      state.selectedId = id;
      state.mode = "detail";
      state.draft = null;
      state.runs = [];
      state.runsFor = null;
      render();
      await loadRuns(id);
    } else if (action === "back-to-list") {
      state.mode = "list";
      state.draft = null;
      render();
    } else if (action === "back-to-detail") {
      state.mode = state.selectedId ? "detail" : "list";
      state.draft = null;
      render();
    } else if (action === "refresh") {
      await mutate("/api/runners/refresh");
      await loadSnapshot(true);
    } else if (action === "new-routine") {
      state.mode = "new";
      state.draft = newRoutine(state.snapshot);
      render();
    } else if (action === "edit-routine" && routine) {
      state.mode = "edit";
      state.draft = { ...routine };
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
      await loadRuns(routine.id, true);
    } else if (action === "cancel" && routine) {
      if (!confirm(`Cancel "${routine.title}"?`)) return;
      await mutate(`/api/routines/${encodeURIComponent(routine.id)}/cancel`);
      await loadSnapshot(true);
      await loadRuns(routine.id, true);
    } else if (action === "delete-routine" && routine) {
      if (!confirm(`Delete "${routine.title}" and its stored runs?`)) return;
      await mutate(`/api/routines/${encodeURIComponent(routine.id)}/delete`);
      state.selectedId = null;
      state.mode = "list";
      state.runs = [];
      state.runsFor = null;
      await loadSnapshot(false);
    } else if (action === "logout") {
      await mutate("/api/logout");
      window.location.reload();
    } else if (action === "revoke-all-browsers") {
      if (!confirm("Revoke access for every remembered browser?")) return;
      await mutate("/api/trusted-browsers/revoke-all");
      window.location.reload();
    }
  } catch (error) {
    if (sequence === state.snapshotSequence) setError(error);
  } finally {
    if (guarded) setBusy(false);
  }
});

app.addEventListener("change", (event) => {
  if (!(event.target instanceof HTMLSelectElement) && !(event.target instanceof HTMLInputElement)) return;
  if (!state.draft) return;
  const form = event.target.closest("#routine-form");
  if (!(form instanceof HTMLFormElement)) return;

  if (event.target.name === "schedule_days") {
    ensureAtLeastOneScheduleDay(form, event.target);
  }

  if (event.target.name === "runner") {
    const runner = runnerById(event.target.value);
    if (!runner) return;
    state.draft = draftFromCurrentForm(form);
    state.draft.runner_id = runner.id;
    state.draft.runner_label = runner.label;
    if (runner.kind === "script" || runner.uses_model === false) {
      state.draft.model = null;
      state.draft.effort = null;
      state.draft.dangerous = false;
    } else {
      state.draft.model = runner.default_model || runner.models?.[0]?.value || "";
      state.draft.effort = runner.default_effort || runner.efforts?.[0]?.value || "";
    }
    state.draft.timeout_seconds = runner.default_timeout_seconds || state.draft.timeout_seconds || null;
    render();
  } else if (event.target.name === "schedule_custom_enabled") {
    state.draft = draftFromCurrentForm(form);
    render();
  } else if (isScheduleField(event.target.name)) {
    state.draft = draftFromCurrentForm(form);
  }
});

app.addEventListener("submit", async (event) => {
  const form = event.target.closest("#routine-form");
  if (!form) return;
  event.preventDefault();
  if (state.busy) return;
  setBusy(true);
  try {
    const routine = routineFromForm(form, currentSchedule());
    await mutate("/api/routines", routine);
    state.selectedId = routine.id;
    state.mode = "detail";
    state.draft = null;
    await loadSnapshot(true);
    await loadRuns(routine.id, true);
  } catch (error) {
    if (sequence === state.runsSequence && state.selectedId === routineId) setError(error);
  } finally {
    setBusy(false);
  }
});

async function loadSnapshot(preserveSelection = true, quiet = false) {
  const sequence = ++state.snapshotSequence;
  if (!quiet) setBusy(true);
  try {
    const snapshot = await requestJson("/api/snapshot");
    if (sequence !== state.snapshotSequence) return;
    state.snapshot = snapshot;
    const routines = snapshot.routines || [];
    if (state.selectedId && !routines.some((routine) => routine.id === state.selectedId)) {
      state.selectedId = null;
      state.mode = "list";
      state.runs = [];
      state.runsFor = null;
    }
    if (!preserveSelection) {
      state.selectedId = null;
      state.mode = "list";
      state.runs = [];
      state.runsFor = null;
    }
    state.error = "";
    render();
    if (state.selectedId && state.runsFor !== state.selectedId && state.mode === "detail") {
      await loadRuns(state.selectedId, true);
    }
  } catch (error) {
    setError(error);
  } finally {
    if (!quiet && sequence === state.snapshotSequence) setBusy(false);
  }
}

async function loadRuns(routineId, quiet = false) {
  if (!routineId) return;
  const sequence = ++state.runsSequence;
  if (!quiet) setBusy(true);
  try {
    const result = await requestJson(`/api/routines/${encodeURIComponent(routineId)}/runs`);
    if (sequence !== state.runsSequence || state.selectedId !== routineId) return;
    state.runs = result.runs || [];
    state.runsFor = routineId;
    state.error = "";
    render();
  } catch (error) {
    setError(error);
  } finally {
    if (!quiet && sequence === state.runsSequence) setBusy(false);
  }
}

async function requestJson(path, options = {}) {
  const response = await fetch(path, {
    ...options,
    headers: {
      ...(options.headers || {}),
    },
  });
  if (response.status === 401) {
    window.location.reload();
    return new Promise(() => {});
  }
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
  if (response.status === 204) return null;
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
  const renderSnapshot = captureRenderSnapshot();
  app.innerHTML = renderMobileApp(state);
  restoreRenderSnapshot(renderSnapshot);
}

function captureRenderSnapshot() {
  const active = document.activeElement;
  return {
    windowX: window.scrollX,
    windowY: window.scrollY,
    activeName: active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement ? active.name : "",
    activeSelectionStart:
      active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement ? active.selectionStart : null,
    activeSelectionEnd:
      active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement ? active.selectionEnd : null,
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
  if (snapshot.activeName) {
    const active = app.querySelector(`[name="${selectorEscape(snapshot.activeName)}"]`);
    if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement) {
      active.focus();
      if (snapshot.activeSelectionStart !== null && snapshot.activeSelectionEnd !== null) {
        active.setSelectionRange(snapshot.activeSelectionStart, snapshot.activeSelectionEnd);
      }
    }
  }
  window.scrollTo(snapshot.windowX, snapshot.windowY);
}

function draftFromCurrentForm(form) {
  return routineDraftFromForm(form, {
    snapshot: state.snapshot,
    currentSchedule: currentSchedule(),
  });
}

function currentSchedule() {
  return state.draft?.schedule || selectedRoutine()?.schedule || "0 7 * * *";
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

function selectorEscape(value) {
  if (window.CSS?.escape) return CSS.escape(String(value));
  return String(value).replace(/["\\]/g, "\\$&");
}
