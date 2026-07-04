import assert from "node:assert/strict";
import test from "node:test";

import { renderMobileApp } from "./mobile-view.js";

const snapshot = {
  timezone: "America/New_York",
  scheduler_last_checked: "2026-07-04T10:00:00Z",
  runners: [
    {
      id: "codex",
      label: "Codex",
      available: true,
      models: [],
      efforts: [],
    },
  ],
  routines: [
    {
      id: "rtn_current",
      title: "Current routine",
      description: "A current routine",
      prompt: "Do the thing.",
      project_label: "ai-scheduler",
      runner_id: "codex",
      runner_label: "Codex",
      runner_available: true,
      cwd: "/home/ar/repos/toolbox/ai-scheduler",
      schedule: "0 7 * * Mon-Fri",
      timezone: "America/New_York",
      paused: false,
      dangerous: false,
      next_run_at: "2026-07-06T11:00:00Z",
      schedule_error: null,
      active_run: null,
      latest_run: null,
    },
    {
      id: "rtn_paused",
      title: "Paused routine",
      description: "A paused routine",
      prompt: "Wait.",
      project_label: "toolbox",
      runner_id: "codex",
      runner_label: "Codex",
      runner_available: true,
      cwd: "/home/ar/repos/toolbox",
      schedule: "0 9 * * *",
      timezone: "America/New_York",
      paused: true,
      dangerous: false,
      next_run_at: null,
      schedule_error: null,
      active_run: null,
      latest_run: null,
    },
  ],
};

test("mobile list page is the desktop sidebar promoted to the whole screen", () => {
  const html = renderMobileApp({
    snapshot,
    mode: "list",
    selectedId: null,
    query: "",
    runs: [],
    runsFor: null,
    draft: null,
    busy: false,
    error: "",
  });

  assert.match(html, /data-view="list"/);
  assert.match(html, /name="search"/);
  assert.match(html, /data-action="new-routine"/);
  assert.match(html, />Current</);
  assert.match(html, />Paused</);
  assert.match(html, />Runners</);
  assert.match(html, /Current routine/);
  assert.match(html, /ai-scheduler - Codex/);
  assert.match(html, /Next ·/);
  assert.match(html, /Paused routine/);
  assert.doesNotMatch(html, /class="layout"/);
  assert.doesNotMatch(html, /side-panel/);
});

test("mobile routine detail is a full-screen page with back navigation and inline runs", () => {
  const html = renderMobileApp({
    snapshot,
    mode: "detail",
    selectedId: "rtn_current",
    query: "",
    runs: [
      {
        id: "run_1",
        status: "succeeded",
        scheduled_for: "2026-07-04T09:00:00Z",
        started_at: "2026-07-04T09:00:01Z",
        finished_at: "2026-07-04T09:00:30Z",
        stdout_preview: "done",
        stderr_preview: "",
        stdout_truncated: false,
        stderr_truncated: false,
      },
    ],
    runsFor: "rtn_current",
    draft: null,
    busy: false,
    error: "",
  });

  assert.match(html, /data-view="detail"/);
  assert.match(html, /data-action="back-to-list"/);
  assert.match(html, /Current routine/);
  assert.match(html, />Run now</);
  assert.match(html, />Pause</);
  assert.match(html, />Edit</);
  assert.match(html, />Runs</);
  assert.match(html, /run_1/);
  assert.match(html, /succeeded/);
  assert.doesNotMatch(html, /class="layout"/);
  assert.doesNotMatch(html, /side-panel/);
});

test("mobile edit page is full-screen and keeps the full routine form", () => {
  const html = renderMobileApp({
    snapshot,
    mode: "edit",
    selectedId: "rtn_current",
    query: "",
    runs: [],
    runsFor: null,
    draft: { ...snapshot.routines[0] },
    busy: false,
    error: "",
  });

  assert.match(html, /data-view="edit"/);
  assert.match(html, /data-action="back-to-detail"/);
  assert.match(html, /<form id="routine-form"/);
  assert.match(html, /name="title"/);
  assert.match(html, /name="prompt"/);
  assert.match(html, /name="runner"/);
  assert.match(html, /name="schedule_days"/);
  assert.match(html, /Save routine/);
  assert.doesNotMatch(html, /side-panel/);
});
