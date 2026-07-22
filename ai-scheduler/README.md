# AI Scheduler

Local desktop scheduler for AI CLI routines. The app targets Arch/Linux first and uses Tauri v2 with a Rust backend and vanilla TypeScript/CSS frontend.

## What It Does

AI Scheduler runs saved local AI CLI routines while the desktop app is open. It does not install or configure the AI tools themselves; it assumes runner CLIs such as `codex`, `claude`, and `cursor-agent` are already installed, authenticated, and available on `PATH`.

Routines can be paused, run manually, searched by title/description/prompt/working directory, and reviewed through their stored run history. Scheduled runs that were missed while the app was closed are recorded as missed and are not run late.

## Install

```sh
make install
make install-local
make update
make bootstrap-config
```

`make install` and `make install-local` install the app binary and desktop launcher into the current user's local XDG paths. They do not overwrite `~/.config/ai-scheduler/config.toml` and do not touch `~/.local/share/ai-scheduler/runs.db`.

## Storage

- Config: `$XDG_CONFIG_HOME/ai-scheduler/config.toml`, fallback `~/.config/ai-scheduler/config.toml`
- Mobile passcode: `$XDG_CONFIG_HOME/ai-scheduler/mobile-passcode`, fallback `~/.config/ai-scheduler/mobile-passcode`
- Runs DB: `$XDG_DATA_HOME/ai-scheduler/runs.db`, fallback `~/.local/share/ai-scheduler/runs.db`
- Trusted mobile browsers: `$XDG_STATE_HOME/ai-scheduler/mobile-trusted-browsers`, fallback `~/.local/state/ai-scheduler/mobile-trusted-browsers`

Run history is stored in SQLite and pruned by config. Defaults keep the last 25 runs per routine and remove terminal runs older than 90 days. Active `queued` and `running` rows are not pruned.
Config saves are serialized and atomic, with at most 20 timestamped backups kept beside the config file.

## Config Model

The TOML config contains:

- `[settings]` for timezone, run retention, default timeout, output cap, and
  the disabled-by-default mobile web server
- `[[runners]]` for CLI command templates
- `[[routines]]` for scheduled work

Built-in runner defaults cover Codex, Claude Code, and Cursor Agent. Each routine chooses a runner, model, optional effort value, working directory, cron schedule, timezone, timeout, and dangerous-mode toggle.

Schedules use cron strings. Five-field cron strings are accepted and normalized to six-field cron with a leading seconds field.

## Mobile Web Spike

The embedded mobile web server is off by default. To enable it, set:

```toml
[settings]
mobile_web_enabled = true
mobile_web_port = 6882
```

When enabled, the desktop app binds a mobile web UI/API to
`http://127.0.0.1:6882` while the app is open. When disabled, no HTTP listener
is started. HTTP access requires the 4-12 digit passcode stored in the mobile
passcode file listed above. A successful unlock trusts that browser for one
year using a random, HTTP-only cookie. Only a SHA-256 token hash and expiry are
stored in the private trusted-browser file. Trust can be revoked for the current
browser or every browser from the mobile UI.

On the first upgraded launch, legacy repository-root `.mobile-passcode` and
`.mobile-trusted-browsers` files are copied to their XDG locations when the new
files do not exist. Legacy raw trust tokens are converted to hashed, expiring
records. Passcode-file changes apply to the next unlock without rebuilding or
restarting the app. Keep both files private with mode `600`. Incorrect unlock
attempts are progressively throttled. The server adds restrictive browser
security headers and rejects state-changing API calls without the expected
same-origin mutation header. The mobile surface can view, create, edit, pause,
resume, delete, run, and cancel routines, and can refresh runner status checks.

External config-file edits are applied on the next app launch. In-app raw config
saves reconcile the mobile server immediately.

## Runtime Behavior

- Routines run only while the app is open.
- On startup, at most the latest 100 missed occurrences per routine are recorded; older gaps are skipped to keep reconciliation bounded.
- A failed scheduled dispatch or missed-run write is retried because the scheduler checkpoint advances only after successful handling.
- Paused routines do not run on schedule but can still be run manually.
- If a scheduled run overlaps an older active run for the same routine, the older run is cancelled as superseded and the newer run starts.
- Closing the app cancels active runs.
- Timeouts and cancels target the child process group so spawned descendants are also terminated.
- stdout and stderr are drained concurrently, stored separately, and capped per stream.

## Development

```sh
make test
make dev
```

`make test` runs Rust tests and the frontend production build. `make dev` starts the Tauri dev app.
