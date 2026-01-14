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

## media-scripts

Python scripts for file/directory management (run with `uv run`):

- `consolidate_files` – flatten nested files into one directory
- `delete_empty_dirs` – remove empty directories
- `delete_moved_files` – delete files from source that already exist in destination (by hash comparison)
- `split_dir` – split large directories into smaller chunks
