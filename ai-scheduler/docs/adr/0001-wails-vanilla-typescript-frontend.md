# Wails with a vanilla TypeScript frontend

Status: Superseded by [0005-tauri-rust-desktop-app.md](./0005-tauri-rust-desktop-app.md)

AI Scheduler is a local desktop app whose backend owns config, scheduling, process execution, and run storage. We will use Wails with vanilla TypeScript and CSS for the frontend, calling Go through the Wails bridge, rather than adding React or an embedded HTTP server for HTMX. This keeps the app light and avoids an unnecessary localhost routing surface while still giving the UI enough structure for routine search, editing, and run viewing.
