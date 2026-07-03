# Parallel runner capability probing

AI Scheduler will probe configured runner CLIs concurrently at startup and on manual refresh. Each runner probe resolves availability, model options, effort options, and dangerous-mode support independently, with a short timeout so one slow or missing CLI does not block the app. Cursor Agent model and effort choices should be parsed from `cursor-agent --list-models` where possible, while Codex and Claude Code can combine CLI help/configured defaults with manual model entry when their CLIs do not expose a complete model catalog.
