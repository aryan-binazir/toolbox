# AI Scheduler

Local desktop scheduler for AI CLI routines. The app targets Arch/Linux first and uses Tauri v2 with a Rust backend and vanilla TypeScript/CSS frontend.

## Commands

```sh
make test
make install
make install-local
make update
make bootstrap-config
```

`make install` and `make install-local` install the app binary and desktop launcher into the current user's local XDG paths. They do not overwrite `~/.config/ai-scheduler/config.toml` and do not touch `~/.local/share/ai-scheduler/runs.db`.

## Storage

- Config: `$XDG_CONFIG_HOME/ai-scheduler/config.toml`, fallback `~/.config/ai-scheduler/config.toml`
- Runs DB: `$XDG_DATA_HOME/ai-scheduler/runs.db`, fallback `~/.local/share/ai-scheduler/runs.db`

The app assumes runner CLIs such as `codex`, `claude`, and `cursor-agent` are already installed and authenticated on the machine.
