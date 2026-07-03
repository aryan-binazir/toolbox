# SQLite run history

AI Scheduler will keep human-editable configuration in `~/.config/ai-scheduler/config.toml` and store routine execution history in `~/.local/share/ai-scheduler/runs.db`. SQLite gives the app a simple local store for run output, timestamps, exit status, cancellation, missed schedules, and retention pruning without making the TOML config large or slow.
