# toolbox

Personal collection of scripts and utilities. Various languages, zero polish, works-for-me quality.

## wt

Go CLI that pairs git worktrees with tmux sessions. One command creates both a worktree and a dedicated tmux session for fast context switching between branches.

```bash
cd ~/your-project
wt create feature-login   # creates worktree + tmux session, attaches
wt create bugfix --no-attach
wt list                    # shows worktrees and session status
wt attach bugfix
wt delete feature-login -y
```

See [wt/README.md](wt/README.md) for full documentation.

## pr-attention

Go CLI that polls GitHub for PRs where you're a reviewer or assignee, sends desktop notifications, and tracks state to avoid repeated alerts.

```bash
pr-attention run                    # poll and notify
pr-attention status                 # show attention queue
pr-attention ack org/repo#42        # silence until updated
pr-attention clear                  # reset state
```

Supports scheduling via launchd (macOS), systemd timer (Linux), or cron. See [pr-attention/README.md](pr-attention/README.md) for full documentation.

## media-scripts

Python scripts for file/directory management (run with `uv run`):

- `consolidate_files` – flatten nested files into one directory
- `delete_empty_dirs` – remove empty directories
- `delete_moved_files` – delete files from source that already exist in destination (by hash comparison)
- `split_dir` – split large directories into smaller chunks
