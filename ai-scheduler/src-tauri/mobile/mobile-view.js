const DAY_VALUES = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
const CRON_DAY_ORDER = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const WEEKDAY_VALUES = ["Mon", "Tue", "Wed", "Thu", "Fri"];
const DAY_PRESETS = [
  { id: "weekdays", label: "Mon-Fri", days: WEEKDAY_VALUES },
  { id: "mon-sun", label: "Mon-Sun", days: DAY_VALUES },
  { id: "sun-sat", label: "Sun-Sat", days: ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"] },
];
const TIME_OPTIONS = buildTimeOptions();

export function renderMobileApp(state) {
  const snapshot = state.snapshot;
  if (!snapshot) {
    return `
      <main class="boot">
        <span class="boot-mark"></span>
        <span>Loading AI Scheduler</span>
      </main>
    `;
  }

  if (state.mode === "list") return renderListPage(state);
  if (state.mode === "detail") return renderDetailPage(state);
  if (state.mode === "new" || state.mode === "edit") return renderEditorPage(state);
  return `<main class="mobile-page" data-view="${escapeAttribute(state.mode || "detail")}"></main>`;
}

export function newRoutine(snapshot) {
  const runner = snapshot?.runners?.[0] || null;
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
    schedule: "0 7 * * *",
    timezone: snapshot?.timezone || "UTC",
    paused: false,
    dangerous: false,
    timeout_seconds: runner?.default_timeout_seconds || null,
  };
}

export function routineFromForm(form, currentSchedule = "0 7 * * *") {
  const data = new FormData(form);
  const id = String(data.get("id") || "").trim() || newRoutineId();
  const timeoutRaw = String(data.get("timeout_seconds") || "").trim();
  const scheduleTime = String(data.get("schedule_time") || "");
  const customScheduleEnabled = data.get("schedule_custom_enabled") === "on";
  const currentControls = parseScheduleControls(currentSchedule);
  const schedule =
    !customScheduleEnabled
      ? buildSimpleSchedule(scheduleDaysFromForm(data, currentControls.days), scheduleTime || currentControls.time)
      : String(data.get("schedule_custom") || currentSchedule);

  return {
    id,
    title: String(data.get("title") || "").trim(),
    description: String(data.get("description") || ""),
    prompt: String(data.get("prompt") || ""),
    runner: String(data.get("runner") || ""),
    model: nullableString(data.get("model")),
    effort: nullableString(data.get("effort")),
    cwd: String(data.get("cwd") || "").trim(),
    schedule,
    timezone: nullableString(data.get("timezone")),
    paused: data.has("paused"),
    dangerous: data.has("dangerous"),
    timeout_seconds: timeoutRaw ? Number(timeoutRaw) : null,
  };
}

export function routineDraftFromForm(form, options = {}) {
  const routine = routineFromForm(form, options.currentSchedule);
  const runner = options.snapshot?.runners?.find((item) => item.id === routine.runner);
  return {
    id: routine.id,
    title: routine.title,
    description: routine.description,
    prompt: routine.prompt,
    runner_id: routine.runner,
    runner_label: runner?.label || routine.runner,
    model: routine.model,
    effort: routine.effort,
    cwd: routine.cwd,
    schedule: routine.schedule,
    timezone: routine.timezone || options.snapshot?.timezone || "UTC",
    paused: routine.paused,
    dangerous: routine.dangerous,
    timeout_seconds: routine.timeout_seconds,
  };
}

export function applySchedulePreset(form, presetId) {
  const preset = DAY_PRESETS.find((item) => item.id === presetId);
  if (!preset) return;
  form.querySelectorAll('input[name="schedule_days"]').forEach((input) => {
    input.checked = preset.days.includes(input.value);
  });
}

export function ensureAtLeastOneScheduleDay(form, fallback) {
  const inputs = Array.from(form.querySelectorAll('input[name="schedule_days"]'));
  if (!inputs.length || inputs.some((input) => input.checked)) return;
  const nextChecked = inputs.find((input) => input.value === fallback?.value) || inputs[0];
  nextChecked.checked = true;
}

export function isScheduleField(name) {
  return (
    name === "schedule_days" ||
    name === "schedule_custom_enabled" ||
    name === "schedule_time" ||
    name === "schedule_custom"
  );
}

function renderListPage(state) {
  const routines = filteredRoutines(state);
  const current = routines.filter((routine) => !routine.paused);
  const paused = routines.filter((routine) => routine.paused);
  return `
    <main class="mobile-shell" data-view="list">
      <header class="mobile-hero">
        <div>
          <h1>AI Scheduler</h1>
          <p>${state.snapshot.routines.length} routines</p>
        </div>
        <button class="icon-button add-button" data-action="new-routine" title="New routine" aria-label="New routine">+</button>
      </header>
      ${state.error ? `<div class="alert">${escapeHtml(state.error)}</div>` : ""}
      <label class="search">
        <span>⌕</span>
        <input name="search" value="${escapeAttribute(state.query || "")}" placeholder="Search" />
      </label>
      ${renderRoutineSection("Current", current)}
      ${renderRoutineSection("Paused", paused)}
      ${renderRunnerPanel(state.snapshot.runners || [])}
      <section class="runner-panel">
        <div class="section-head"><h2>Browser trust</h2></div>
        <div class="actions">
          <button data-action="logout" ${state.busy ? "disabled" : ""}>Forget this browser</button>
          <button class="danger" data-action="revoke-all-browsers" ${state.busy ? "disabled" : ""}>Revoke all browsers</button>
        </div>
      </section>
    </main>
  `;
}

function filteredRoutines(state) {
  const query = String(state.query || "").trim().toLowerCase();
  const routines = state.snapshot?.routines || [];
  if (!query) return routines;
  return routines.filter((routine) =>
    [routine.title, routine.description, routine.prompt, routine.project_label, routine.runner_label, routine.cwd]
      .join(" ")
      .toLowerCase()
      .includes(query),
  );
}

function renderRoutineSection(title, routines) {
  return `
    <section class="routine-section">
      <div class="section-head">
        <h2>${escapeHtml(title)}</h2>
        <span>${routines.length}</span>
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

function renderRoutineRow(routine) {
  const pauseTitle = routine.paused ? "Resume routine" : "Pause routine";
  return `
    <button class="routine-row" data-action="select" data-id="${escapeAttribute(routine.id)}">
      <span class="pause-dot" data-action="pause" data-id="${escapeAttribute(routine.id)}" title="${escapeAttribute(pauseTitle)}" aria-label="${escapeAttribute(pauseTitle)}">
        ${routine.paused ? "▷" : ""}
      </span>
      <span class="routine-copy">
        <span class="routine-title">${escapeHtml(routine.title)}</span>
        <span class="routine-project">${escapeHtml(`${routine.project_label} - ${routine.runner_label}`)}</span>
        <span class="routine-schedule">${
          routine.paused
            ? escapeHtml(scheduleLabel(routine))
            : `Next · ${escapeHtml(formatDate(routine.next_run_at) || "—")}`
        }</span>
      </span>
    </button>
  `;
}

function renderRunnerPanel(runners) {
  return `
    <section class="runner-panel">
      <div class="section-head">
        <h2>Runners</h2>
        <button data-action="refresh" title="Refresh runner status">Refresh</button>
      </div>
      <div class="runner-list">
        ${runners.length ? runners.map(renderRunnerStatus).join("") : `<div class="muted-row">No runners configured</div>`}
      </div>
    </section>
  `;
}

function renderRunnerStatus(runner) {
  return `
    <div class="runner-status">
      <span class="runner-light ${runner.available ? "ok" : "bad"}"></span>
      <span>
        <strong>${escapeHtml(runner.label)}</strong>
        <small>${runner.available ? "Version check passed" : "Version check failed"}</small>
      </span>
    </div>
  `;
}

function newRoutineId() {
  if (globalThis.crypto?.randomUUID) {
    return `rtn_mobile_${globalThis.crypto.randomUUID().replaceAll("-", "")}`;
  }
  if (globalThis.crypto?.getRandomValues) {
    const bytes = globalThis.crypto.getRandomValues(new Uint8Array(16));
    return `rtn_mobile_${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
  }
  return `rtn_mobile_${Date.now().toString(36)}_${Math.random().toString(36).slice(2)}`;
}

function renderDetailPage(state) {
  const routine = selectedRoutine(state);
  if (!routine) return renderListPage({ ...state, mode: "list" });
  const active = routine.active_run;
  const latest = routine.latest_run;
  const runs = state.runsFor === routine.id ? state.runs || [] : [];
  return `
    <main class="mobile-shell detail-page" data-view="detail">
      <header class="page-head">
        <button class="back-button" data-action="back-to-list">Back</button>
      </header>
      ${state.error ? `<div class="alert">${escapeHtml(state.error)}</div>` : ""}
      <article class="routine-detail">
        <h1>${escapeHtml(routine.title)}</h1>
        <p>${escapeHtml(routine.description || "No description")}</p>
        <pre class="prompt">${escapeHtml(routine.prompt)}</pre>
        <div class="actions">
          ${
            active
              ? `<button class="danger wide" data-action="cancel" data-id="${escapeAttribute(routine.id)}" ${state.busy ? "disabled" : ""}>Cancel run</button>`
              : `<button class="primary wide" data-action="run" data-id="${escapeAttribute(routine.id)}" ${state.busy ? "disabled" : ""}>Run now</button>`
          }
          <button data-action="pause" data-id="${escapeAttribute(routine.id)}" ${state.busy ? "disabled" : ""}>${routine.paused ? "Resume" : "Pause"}</button>
          <button data-action="edit-routine" data-id="${escapeAttribute(routine.id)}" ${state.busy ? "disabled" : ""}>Edit</button>
          <button class="danger" data-action="delete-routine" data-id="${escapeAttribute(routine.id)}" ${state.busy ? "disabled" : ""}>Delete</button>
        </div>
        <dl class="meta-grid">
          <div><dt>Status</dt><dd>${escapeHtml(active ? `${statusText(active.status)} · ${formatDate(active.started_at || active.scheduled_for)}` : routine.paused ? "Paused" : "Active")}</dd></div>
          <div><dt>Runner</dt><dd>${escapeHtml(`${routine.runner_label}${routine.runner_available ? "" : " unavailable"}`)}</dd></div>
          <div><dt>Project</dt><dd>${escapeHtml(routine.project_label)}</dd></div>
          <div><dt>Working directory</dt><dd>${escapeHtml(routine.cwd)}</dd></div>
          <div><dt>Schedule</dt><dd>${escapeHtml(scheduleLabel(routine))}</dd></div>
          <div><dt>Next run</dt><dd>${escapeHtml(routine.paused ? "Paused" : formatDate(routine.next_run_at))}</dd></div>
          <div><dt>Dangerous</dt><dd>${escapeHtml(routine.dangerous ? "On" : "Off")}</dd></div>
          <div><dt>Last run</dt><dd>${escapeHtml(latest ? `${statusText(latest.status)} · ${formatDate(latest.finished_at || latest.started_at || latest.scheduled_for)}` : "—")}</dd></div>
        </dl>
      </article>
      <section class="runs">
        <h2>Runs</h2>
        ${runs.length ? runs.map(renderRun).join("") : `<div class="muted-row">No runs yet</div>`}
      </section>
    </main>
  `;
}

function renderRun(run) {
  const when = formatDate(run.started_at || run.scheduled_for || run.finished_at);
  return `
    <details class="run" data-run-id="${escapeAttribute(run.id)}">
      <summary>
        <span class="${statusClass(run.status)}">${escapeHtml(statusText(run.status))}</span>
        <span>${escapeHtml(when)}</span>
        <span>${escapeHtml(run.id)}</span>
      </summary>
      <div class="run-body">
        <div class="output-grid">
          <div class="stream">
            <h3>stderr${run.stderr_truncated ? " - capped" : ""}</h3>
            <pre data-run-output="stderr">${escapeHtml(run.stderr_preview || "No stderr")}</pre>
          </div>
          <div class="stream">
            <h3>stdout${run.stdout_truncated ? " - capped" : ""}</h3>
            <pre data-run-output="stdout">${escapeHtml(run.stdout_preview || "No stdout")}</pre>
          </div>
        </div>
      </div>
    </details>
  `;
}

function renderEditorPage(state) {
  const routine = state.draft || selectedRoutine(state);
  return `
    <main class="mobile-shell editor-page" data-view="${escapeAttribute(state.mode)}">
      <header class="page-head">
        <button class="back-button" data-action="${state.mode === "new" ? "back-to-list" : "back-to-detail"}">Back</button>
        ${state.mode === "new" ? "<h1>New routine</h1>" : ""}
      </header>
      ${state.error ? `<div class="alert">${escapeHtml(state.error)}</div>` : ""}
      ${routine ? renderRoutineForm(routine, state) : `<div class="muted-row">No routine selected</div>`}
    </main>
  `;
}

function renderRoutineForm(routine, state) {
  const runner = runnerById(state, routine.runner_id) || state.snapshot?.runners?.[0] || null;
  const models = runner?.models || [];
  const efforts = runner?.efforts || [];
  const schedule = parseScheduleControls(routine.schedule || "0 7 * * *");
  const scriptLike = isScriptLikeRunner(runner);

  return `
    <form id="routine-form" class="routine-form">
      <input type="hidden" name="id" value="${escapeAttribute(routine.id || "")}" />
      <label class="form-title">Title<input name="title" value="${escapeAttribute(routine.title || "")}" required /></label>
      <label class="form-description">Description<textarea name="description">${escapeHtml(routine.description || "")}</textarea></label>
      <label class="form-prompt">${scriptLike ? "Command" : "Prompt"}<textarea class="prompt-input" name="prompt" required placeholder="${scriptLike ? "echo hello || /path/to/script.sh" : ""}">${escapeHtml(routine.prompt || "")}</textarea></label>
      <div class="form-grid">
        <label>Runner<select name="runner">${(state.snapshot?.runners || []).map((item) => optionHtml(item.id, item.label, routine.runner_id)).join("")}</select></label>
        ${
          scriptLike
            ? ""
            : `<label>Model<select name="model">${models.map((item) => optionHtml(item.value, item.label, routine.model)).join("")}</select></label>
        <label>Effort<select name="effort">${efforts.length ? efforts.map((item) => optionHtml(item.value, item.label, routine.effort)).join("") : `<option value="">None</option>`}</select></label>`
        }
      </div>
      <label class="form-cwd">Working directory<input name="cwd" value="${escapeAttribute(routine.cwd || "")}" required /></label>
      <div class="form-grid">
        ${renderScheduleDayControls(schedule)}
        ${
          schedule.customEnabled
            ? `<label class="custom-schedule">Cron<input name="schedule_custom" value="${escapeAttribute(schedule.custom)}" required /></label>`
            : `<label>Time<select name="schedule_time">${TIME_OPTIONS.map((item) => optionHtml(item.value, item.label, schedule.time)).join("")}</select></label>`
        }
        <label>Timezone<input name="timezone" value="${escapeAttribute(routine.timezone || state.snapshot?.timezone || "UTC")}" /></label>
        <label>Timeout seconds<input type="number" min="1" name="timeout_seconds" value="${escapeAttribute(routine.timeout_seconds || "")}" /></label>
      </div>
      <div class="check-row">
        <label><input type="checkbox" name="paused" ${routine.paused ? "checked" : ""} /> Paused</label>
        ${scriptLike ? "" : `<label><input type="checkbox" name="dangerous" ${routine.dangerous ? "checked" : ""} /> Dangerous</label>`}
      </div>
      ${scriptLike ? `<div class="inline-note">Runs as bash -lc in the working directory</div>` : ""}
      <div class="form-actions">
        <button type="button" data-action="${state.mode === "new" ? "back-to-list" : "back-to-detail"}">Cancel</button>
        <button class="primary" type="submit" ${state.busy ? "disabled" : ""}>Save routine</button>
      </div>
    </form>
  `;
}

function isScriptLikeRunner(runner) {
  if (!runner) return false;
  if (runner.kind === "script") return true;
  if (runner.uses_model === false) return true;
  return false;
}

function renderScheduleDayControls(schedule) {
  return `
    <fieldset class="schedule-days">
      <legend>Days</legend>
      ${
        schedule.customEnabled
          ? ""
          : `<div class="schedule-preset-row">${DAY_PRESETS.map((preset) => `<button type="button" data-schedule-preset="${escapeAttribute(preset.id)}">${escapeHtml(preset.label)}</button>`).join("")}</div>
             <div class="day-checkbox-grid">${DAY_VALUES.map((day) => renderDayCheckbox(day, schedule.days.includes(day))).join("")}</div>`
      }
      <label class="schedule-custom-toggle"><input type="checkbox" name="schedule_custom_enabled" ${schedule.customEnabled ? "checked" : ""} /> Custom cron</label>
    </fieldset>
  `;
}

function renderDayCheckbox(day, checked) {
  return `<label><input type="checkbox" name="schedule_days" value="${escapeAttribute(day)}" ${checked ? "checked" : ""} /> ${escapeHtml(day)}</label>`;
}

function selectedRoutine(state) {
  return state.snapshot?.routines?.find((routine) => routine.id === state.selectedId) || null;
}

function runnerById(state, id) {
  return state.snapshot?.runners?.find((runner) => runner.id === id) || null;
}

function statusClass(status) {
  return `status status-${String(status || "").replace("_", "-")}`;
}

function statusText(status) {
  return String(status || "unknown").replaceAll("_", " ");
}

function scheduleLabel(routine) {
  return `${routine.schedule_error || routine.schedule} · ${routine.timezone || "UTC"}`;
}

function parseScheduleControls(schedule) {
  const parsed = parseSimpleSchedule(schedule);
  if (parsed) return { ...parsed, custom: "", customEnabled: false };
  return { days: [...DAY_VALUES], time: "07:00", custom: schedule, customEnabled: true };
}

function buildSimpleSchedule(days, time) {
  const [hour = "7", minute = "0"] = String(time || "07:00").split(":");
  return `${Number(minute)} ${Number(hour)} * * ${buildDayField(days)}`;
}

function buildDayField(days) {
  const ordered = orderDays(days);
  if (sameDays(ordered, DAY_VALUES)) return "*";
  if (sameDays(ordered, WEEKDAY_VALUES)) return "Mon-Fri";
  return ordered.join(",");
}

function scheduleDaysFromForm(data, fallbackDays) {
  const selected = data.getAll("schedule_days").map(String);
  const days = orderDays(selected);
  return days.length ? days : fallbackDays;
}

function parseSimpleSchedule(schedule) {
  const fields = String(schedule || "").trim().split(/\s+/).filter(Boolean);
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

function parseDayField(value) {
  if (value === "*") return [...DAY_VALUES];
  const days = [];
  for (const part of String(value || "").split(",")) {
    const expanded = expandDayPart(part.trim());
    if (!expanded.length) return undefined;
    days.push(...expanded);
  }
  return orderDays(days);
}

function expandDayPart(value) {
  if (!value) return [];
  if (value.includes("-")) {
    const [startRaw, endRaw] = value.split("-", 2);
    const start = normalizeDayValue(startRaw);
    const end = normalizeDayValue(endRaw);
    if (!start || !end) return [];
    const startIndex = CRON_DAY_ORDER.indexOf(start);
    const endIndex = CRON_DAY_ORDER.indexOf(end);
    if (startIndex < 0 || endIndex < 0) return [];
    return startIndex <= endIndex
      ? CRON_DAY_ORDER.slice(startIndex, endIndex + 1)
      : [...CRON_DAY_ORDER.slice(startIndex), ...CRON_DAY_ORDER.slice(0, endIndex + 1)];
  }
  const day = normalizeDayValue(value);
  return day ? [day] : [];
}

function normalizeDayValue(value) {
  const lower = String(value || "").trim().toLowerCase();
  return DAY_VALUES.find((day) => day.toLowerCase() === lower);
}

function orderDays(days) {
  const selected = new Set(days.filter((day) => DAY_VALUES.includes(day)));
  return DAY_VALUES.filter((day) => selected.has(day));
}

function sameDays(left, right) {
  return left.length === right.length && left.every((day, index) => day === right[index]);
}

function buildTimeOptions() {
  const options = [];
  for (let hour = 0; hour < 24; hour += 1) {
    for (let minute = 0; minute < 60; minute += 5) {
      const value = `${String(hour).padStart(2, "0")}:${String(minute).padStart(2, "0")}`;
      options.push({ value, label: timeLabel(value) });
    }
  }
  return options;
}

function timeLabel(value) {
  const [hourRaw = "0", minute = "00"] = value.split(":");
  const hour = Number(hourRaw);
  const displayHour = hour % 12 || 12;
  const period = hour < 12 ? "AM" : "PM";
  return `${displayHour}:${minute.padStart(2, "0")} ${period}`;
}

function optionHtml(value, label, selected) {
  return `<option value="${escapeAttribute(value)}" ${value === selected ? "selected" : ""}>${escapeHtml(label)}</option>`;
}

function nullableString(value) {
  const text = String(value || "").trim();
  return text ? text : null;
}

function formatDate(value) {
  if (!value) return "—";
  const date = new Date(value);
  if (Number.isNaN(date.valueOf())) return value;
  return date.toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "numeric",
    minute: "2-digit",
  });
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

function escapeAttribute(value) {
  return escapeHtml(value);
}
