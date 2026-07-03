# Tauri with a Rust backend

AI Scheduler will be built as a Tauri v2 desktop app with a Rust backend and a vanilla TypeScript/CSS frontend. Rust/Tauri is viable for the Arch Linux target and gives the scheduler, process manager, config validation, and SQLite store strong local ownership in one native binary. This supersedes the earlier Wails/Go direction while preserving the decision to avoid React and avoid an embedded HTTP server for HTMX.
