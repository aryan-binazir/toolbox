# wt - Worktree + Tmux Session Manager

A CLI tool that pairs git worktrees with tmux sessions for fast, isolated development environments.

## Why?

Switching between branches with `git checkout` or `git switch` is disruptive—you lose your terminal state, editor context, and running processes. Worktrees solve this by letting you have multiple branches checked out simultaneously, but managing them alongside tmux sessions is tedious.

`wt` automates this: one command creates both a worktree and a dedicated tmux session, so you can instantly jump between features, bug fixes, and experiments without losing context.

## Installation

### From source

```bash
git clone <repo-url>
cd wt
go build -o wt .

# Install to PATH (choose one)
sudo cp wt /usr/local/bin/
# or
cp wt ~/.local/bin/
# or
go install .
```

### Requirements

- Go 1.21+
- Git 2.15+ (worktree support)
- tmux 2.0+

## Quick Start

```bash
cd ~/your-project

# Create a new worktree + tmux session and attach to it
wt create feature-login

# You're now in a tmux session, in a new worktree, on branch 'feature-login'
# Do your work...

# Switch to another feature (from any terminal)
wt create billing-fix --no-attach
wt attach billing-fix

# See all your worktrees and their session status
wt list

# Clean up when done
wt delete feature-login
```

## Commands

### `wt create <name>`

Creates a git worktree and an associated tmux session.

```bash
# Basic usage - creates worktree and attaches to session
wt create feature-login

# Create with a different branch name
wt create hotfix -b fix/critical-issue-123

# Create without attaching (background work)
wt create experiment --no-attach

# Create in a custom directory
wt create spike -d ~/worktrees

# Create as a window in current tmux session (instead of new session)
wt create quick-fix --window
```

**Flags:**
| Flag | Description |
|------|-------------|
| `-b, --branch <name>` | Branch name (defaults to worktree name) |
| `--no-attach` | Don't attach to the session after creation |
| `-w, --window` | Create as window in current tmux session (requires being inside tmux) |
| `-d, --dir <path>` | Directory for worktrees (default: sibling to repo) |

### `wt list`

Lists all worktrees and shows which have active tmux sessions.

```bash
wt list
# or
wt ls
```

**Output:**
```
NAME            BRANCH        SESSION   PATH
----            ------        -------   ----
my-project      main          -         /home/user/my-project
feature-login   feature-login active    /home/user/feature-login
billing-fix     billing-fix   attached  /home/user/billing-fix
```

Session status:
- `-` = No tmux session
- `active` = Session exists, not attached
- `attached` = Session exists and currently attached

### `wt attach <name>`

Attaches to an existing tmux session. If the session doesn't exist but the worktree does, creates a new session.

```bash
wt attach feature-login
# or
wt a feature-login
```

**Behavior:**
- If inside tmux: switches to the target session
- If outside tmux: attaches to the target session

### `wt delete <name>`

Deletes a worktree and its associated tmux session.

```bash
# Interactive (prompts for confirmation)
wt delete feature-login

# Skip confirmation
wt delete feature-login -y
# or
wt rm feature-login -y

# Force delete (even with uncommitted changes)
wt delete experiment -f -y

# Delete a window instead of a session
wt delete quick-fix --window -y
```

**Flags:**
| Flag | Description |
|------|-------------|
| `-y, --yes` | Skip confirmation prompt |
| `-f, --force` | Force deletion even with uncommitted changes |
| `-w, --window` | Delete window in current session (requires being inside tmux) |

### `wt slot`

Creates one worktree slot from a fixed pool: `alpha`, `beta`, `gamma`, `delta`.

```bash
wt slot
```

Behavior:
- checks existing worktrees in order `alpha` -> `beta` -> `gamma` -> `delta`
- creates the first missing slot from `main` (or `--base`)
- symlinks `<slot>/context` to `<base-worktree>/context`
- stops at 4 total slots (errors when all 4 already exist)

```bash
wt slot --base main
wt slot --base develop
```

### `make run`

From the `wt/` directory:

```bash
make run ARGS="slot"
make run ARGS="slot --base main"
```

## How It Works

### Worktree Location

By default, worktrees are created as siblings to your main repository:

```
~/projects/
├── my-app/              # Main repo
├── feature-login/       # Worktree created by: wt create feature-login
├── billing-fix/         # Worktree created by: wt create billing-fix
└── experiment/          # Worktree created by: wt create experiment
```

Use `-d` to specify a different location:

```bash
wt create feature -d ~/worktrees
# Creates ~/worktrees/feature
```

### Session Naming

Tmux sessions are named after the worktree. This makes it easy to:

```bash
# From anywhere, attach directly with tmux
tmux attach -t feature-login

# Or use wt
wt attach feature-login
```

### State Management

`wt` doesn't maintain its own state file. It derives everything from:
- `git worktree list` - Lists existing worktrees
- `tmux list-sessions` - Lists existing sessions

This means you can freely use `git worktree` and `tmux` commands directly—`wt` will stay in sync.

## Workflow Examples

### Feature Development

```bash
# Start new feature
cd ~/my-project
wt create user-dashboard

# Work on it, then switch to something urgent
wt create hotfix-auth --no-attach
wt attach hotfix-auth

# Fix done, back to feature
wt attach user-dashboard

# List everything
wt list

# Clean up merged work
wt delete hotfix-auth -y
```

### Code Review

```bash
# Review a colleague's PR without disrupting your work
wt create review-pr-456 -b origin/feature/their-branch --no-attach
wt attach review-pr-456

# Review done
wt delete review-pr-456 -y
```

### Experimentation

```bash
# Try something risky without affecting main work
wt create spike-new-api --no-attach

# Didn't work out? Clean delete
wt delete spike-new-api -f -y
```

### Quick Tasks (Window Mode)

When you want to keep related work in the same tmux session but different windows:

```bash
# Inside an existing tmux session...
wt create quick-bugfix --window

# Work on the fix, then clean up
wt delete quick-bugfix --window -y
```

**When to use `--window` vs new session:**
- **New session (default):** Long-running work, separate projects, need to detach/reattach independently
- **Window (`--window`):** Quick fixes, related tasks, want everything in one session

## Input Validation

Names must:
- Start with an alphanumeric character
- Contain only alphanumeric characters, dots (`.`), underscores (`_`), or hyphens (`-`)
- Be 100 characters or fewer
- Not be reserved words: `list`, `delete`, `attach`, `new`, `.`, `..`

```bash
# Valid
wt create feature-login
wt create v1.0.0-hotfix
wt create issue_123

# Invalid
wt create -bad-name      # Can't start with hyphen
wt create "has spaces"   # No spaces allowed
wt create list           # Reserved word
```

## Troubleshooting

### "not inside a git repository"

Run `wt` from within a git repository, or navigate to one first.

### "worktree already exists"

A worktree with that name already exists. Use `wt list` to see existing worktrees, or choose a different name.

### "tmux session already exists"

A tmux session with that name exists (possibly from manual creation). Either:
- Use `wt attach <name>` to attach to it
- Kill it manually with `tmux kill-session -t <name>` and retry

### "failed to remove worktree: ... has changes"

The worktree has uncommitted changes. Either:
- Commit or stash the changes
- Use `-f` to force deletion: `wt delete <name> -f`

### "--window requires being inside a tmux session"

The `--window` flag can only be used from inside an existing tmux session. Either:
- Attach to a tmux session first: `tmux attach` or `wt attach <name>`
- Use the default behavior (create a new session) by omitting `--window`

## Development

### Running Tests

```bash
cd wt
go test ./... -v
```

### Project Structure

```
wt/
├── main.go              # Entry point
├── cmd/
│   ├── root.go          # Root command and global flags
│   ├── create.go        # Create command
│   ├── list.go          # List command
│   ├── delete.go        # Delete command
│   ├── attach.go        # Attach command
│   └── create_test.go   # Validation tests
└── internal/
    ├── git/
    │   ├── git.go       # Git/worktree operations
    │   └── git_test.go
    └── tmux/
        ├── tmux.go      # Tmux session operations
        └── tmux_test.go
```

## License

MIT
