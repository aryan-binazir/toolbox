# Linux XDG storage paths

AI Scheduler v1 targets Arch Linux desktops and will use Linux XDG paths for app files. Configuration lives under `$XDG_CONFIG_HOME/ai-scheduler/config.toml` with `~/.config/ai-scheduler/config.toml` as the fallback, run history lives under `$XDG_DATA_HOME/ai-scheduler/runs.db` with `~/.local/share/ai-scheduler/runs.db` as the fallback, and runtime state or exported logs can use `$XDG_STATE_HOME/ai-scheduler/` with `~/.local/state/ai-scheduler/` as the fallback.
