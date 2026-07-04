# AI Scheduler Spec

AI Scheduler is an Arch-first local desktop app for scheduling AI CLI routines while the app is open. The app uses Tauri v2 with a Rust backend and a vanilla TypeScript/CSS frontend.

## Product Model

- A routine is a saved scheduled automation.
- A run is one stored execution or non-execution record for a routine.
- Routines are split into Current and Paused sections.
- Routine search matches title, description, prompt, and working directory.
- Run history is viewed from a visible routine, not from a global run-history screen.

## Storage

- Config path: `$XDG_CONFIG_HOME/ai-scheduler/config.toml`, fallback `~/.config/ai-scheduler/config.toml`.
- Run history path: `$XDG_DATA_HOME/ai-scheduler/runs.db`, fallback `~/.local/share/ai-scheduler/runs.db`.
- Config stores settings, runners, and routines.
- Settings include a disabled-by-default mobile web server flag and port.
- SQLite stores runs, stdout, stderr, exit state, cancel reasons, missed runs, and pruning metadata.
- Default retention keeps the last 25 runs per routine and prunes runs older than 90 days.
- Orphaned runs from raw config removal are hidden from the UI but still pruned.

## Scheduling

- Routines only run while the app is open.
- The mobile web server only runs while the desktop app is open and
  `[settings].mobile_web_enabled` is true.
- The mobile web server binds to `127.0.0.1` on `[settings].mobile_web_port`,
  default `6882`.
- The mobile web surface supports routine viewing, creation, editing, deletion,
  pause/resume, manual runs, cancellation, run history, and runner status
  refresh.
- Missed scheduled occurrences are stored as missed runs and are never run late.
- Schedules use cron strings plus IANA timezones.
- Pausing a routine prevents future scheduled execution but does not cancel an active run.
- Paused routines can still be run manually.
- If a routine becomes due while an older run of the same routine is active, the app cancels the older run as superseded and starts the newer one.
- Closing the app cancels active runs with reason `app_closed`.

## Runners

- Built-in runners are Codex, Claude Code, and Cursor Agent.
- Runner commands are command names resolved through PATH, such as `codex`, `claude`, and `cursor-agent`.
- The app assumes runner CLIs are already installed and authenticated.
- The app does not manage credentials and does not store environment variables.
- Runner availability, version, model options, effort options, and dangerous-mode support are probed in parallel.
- Codex and Claude expose separate effort controls.
- Cursor uses a model dropdown; v1 defaults to `composer-2.5` and `composer-2.5-fast`.
- Dangerous mode is a simple per-routine toggle, default off.
- Prompts are delivered according to runner config, not forced through stdin.
- The process manager drains stdout and stderr concurrently.

## Runs

- Run statuses: `queued`, `running`, `succeeded`, `failed`, `cancelled`, `timed_out`, `missed`, `superseded`.
- A queued run row is created before spawning the process.
- stdout and stderr are stored separately in SQLite.
- Each stream is capped at 5 MB per run with truncation flags.
- Default timeout is 30 minutes.
- Timeout and cancel terminate the child process and spawned descendants.
- Cancellation first sends graceful termination, then hard kill if needed.
- No sleep inhibitor in v1.

## Editing

- Routine creation and editing have normal form controls.
- Raw TOML editing is available in an in-app panel.
- Raw TOML can be invalid while editing, but save requires parse and schema validation.
- New routines get random app-generated IDs.
- Form saves write canonical TOML and do not preserve comments or hand formatting.
- Removing a routine from raw TOML does not delete its run history.
- Deleting a routine through the UI deletes the routine and its run history after confirmation.

## Install and Update

- `make install` builds the app, installs the binary to `$PREFIX/bin/ai-scheduler`, and writes a desktop launcher.
- `make install` must not overwrite `config.toml`, routines, or `runs.db`.
- `make bootstrap-config` may create a config only when one does not already exist.
- `make update` pulls fast-forward changes and reruns install.

## Testing Approach

- Build with TDD using vertical slices.
- First implementation slice: backend config loading and validation through a public Rust interface.
- Tests should verify behavior through public interfaces, not internal implementation details.
