# AI Scheduler

AI Scheduler is a local desktop app for defining and running scheduled AI automations while the app is open.

## Language

**Routine**:
A saved scheduled automation that can be searched, paused, run manually, edited, or deleted.
_Avoid_: Task, job, automation

**Routine ID**:
A stable app-generated identifier that links a routine definition to its stored runs.
_Avoid_: User slug, title key

**Pause**:
The routine-level state that prevents scheduled execution while keeping the routine searchable, editable, and runnable manually.
_Avoid_: Disable, archive

**Run**:
One stored execution of a routine, including the command result and enough metadata to view, cancel, diagnose, or prune it.
_Avoid_: Task, job

**Superseded Run**:
A run that was cancelled because the same routine became due again and the newer run replaced it.
_Avoid_: Overlap, skipped run

**Missed Run**:
A stored non-execution record for a scheduled occurrence that passed while the app was closed.
_Avoid_: Catch-up run, delayed run

**Retention Policy**:
The rule that decides which stored runs are kept and which old results are pruned.
_Avoid_: Cleanup, history limit

**Schedule**:
The cron expression and timezone that determine when an unpaused routine is eligible to run while the app is open.
_Avoid_: Cadence, repeat

**Working Directory**:
The filesystem directory where a routine's runner process starts.
_Avoid_: Project, repository

**Runner**:
A configured CLI tool that can execute a routine headlessly.
_Avoid_: App, provider

**Runner Capability**:
The discovered or configured options a runner exposes to the UI, such as available models, effort levels, and dangerous-mode support.
_Avoid_: Static dropdown, hardcoded options

**Dangerous Mode**:
A per-routine setting that lets the selected runner bypass normal permission prompts or sandboxing for that routine's manual and scheduled runs.
_Avoid_: Global YOLO, unsafe default
