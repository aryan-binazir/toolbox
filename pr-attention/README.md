# pr-attention

Desktop notification tool for GitHub pull requests that need your attention.

Monitors PRs where you are requested as a reviewer or assigned, and sends notifications when action is needed. Tracks PR state to avoid spamming you with repeated notifications.

## Installation

```bash
# From source
go install github.com/ar/toolbox/pr-attention@latest

# Or build locally
git clone <repo>
cd pr-attention
go build -o pr-attention .
sudo mv pr-attention /usr/local/bin/
```

Requires the `gh` CLI to be installed and authenticated.

## Configuration

Config file: `~/.config/pr-attention/config.toml`

```toml
# GitHub organizations to monitor (required)
orgs = ["myorg", "another-org"]

# GitHub Enterprise host (optional, defaults to github.com)
gh_host = "github.example.com"

# Exclude draft PRs (default: true)
exclude_drafts = true

# Ignore PRs with these labels
ignore_labels = ["WIP", "do-not-review", "on-hold"]

# Ignore PRs from these authors (useful for bots)
ignore_authors = ["dependabot[bot]", "renovate[bot]"]

# Play sound with notifications (default: true)
sound_enabled = true

# Database path (default: ~/.local/share/pr-attention/state.db)
db_path = "/custom/path/state.db"
```

### Environment Variables

- `PR_ATTENTION_ORGS` - Comma-separated list of organizations (overrides config file)
- `GH_HOST` - GitHub host for Enterprise (overrides config file)

### CLI Flags

Flags take highest precedence:

```
--org       Organization to monitor (repeatable)
--config    Config file path
--quiet     Suppress non-error output
```

## CLI Usage

### Run a poll

```bash
# Basic usage
pr-attention run

# Include draft PRs
pr-attention run --include-drafts

# Specify orgs via CLI
pr-attention run --org myorg --org another-org

# Quiet mode (for cron/scheduled tasks)
pr-attention run --quiet
```

### Check status

```bash
pr-attention status
```

Shows the last run info and current attention queue:

```
=== Last Run ===
Time:          2024-01-15T10:30:00Z (5m ago)
PRs found:     3
Notifications: 1
Duration:      245ms

=== Attention Queue ===
REPO          #    TITLE                     STATUS            SINCE  ACKED
org/repo      42   Add new feature           review_requested  2d     no
org/repo      43   Fix critical bug          both              1h     yes
other/lib     17   Update dependencies       assigned          30m    no
```

### Acknowledge a PR

Suppress notifications for a PR until it's updated:

```bash
# By URL
pr-attention ack https://github.com/org/repo/pull/42

# By short reference
pr-attention ack org/repo#42
```

### Clear database

Reset all state:

```bash
# With confirmation prompt
pr-attention clear

# Force without prompt
pr-attention clear --force
```

## Scheduler Setup

### macOS (launchd)

```bash
# Copy plist
cp schedulers/com.ar.pr-attention.plist ~/Library/LaunchAgents/

# Load (starts immediately and on login)
launchctl load ~/Library/LaunchAgents/com.ar.pr-attention.plist

# Unload
launchctl unload ~/Library/LaunchAgents/com.ar.pr-attention.plist

# Check status
launchctl list | grep pr-attention
```

### Linux (systemd user timer)

```bash
# Copy service and timer
mkdir -p ~/.config/systemd/user
cp schedulers/pr-attention.service ~/.config/systemd/user/
cp schedulers/pr-attention.timer ~/.config/systemd/user/

# Enable and start timer
systemctl --user daemon-reload
systemctl --user enable pr-attention.timer
systemctl --user start pr-attention.timer

# Check status
systemctl --user status pr-attention.timer
systemctl --user list-timers

# View logs
journalctl --user -u pr-attention.service
```

### Cron

```bash
# Edit crontab
crontab -e

# Add line (runs every 10 minutes)
*/10 * * * * /usr/local/bin/pr-attention run --quiet >> /tmp/pr-attention.log 2>> /tmp/pr-attention.err
```

## How It Works

### Polling Logic

1. Queries GitHub for PRs where you are:
   - Requested as a reviewer (`review-requested:@me`)
   - Assigned (`assignee:@me`)
2. Filters out drafts (unless `--include-drafts`), ignored labels, and ignored authors
3. Merges results, marking PRs appearing in both queries as status "both"
4. Compares with stored state to detect new PRs or updates

### Notification Rules

A notification is sent when:

- A PR is newly added to your attention queue
- A previously-seen PR has been updated since you last saw it
- The PR is not currently acknowledged

A notification is NOT sent when:

- The PR was already notified and hasn't changed
- The PR is acknowledged (acked until next update)
- The PR is a draft (unless `--include-drafts`)
- The PR has an ignored label or author

### Backoff

If GitHub API calls fail, the tool uses exponential backoff:

- Base delay: 60 seconds
- Doubles with each failure (60s, 120s, 240s, ...)
- Capped at 1 hour maximum
- Includes +/-10% jitter to avoid thundering herd
- Resets on successful poll

### State Storage

SQLite database at `~/.local/share/pr-attention/state.db` stores:

- PR tracking (first seen, last seen, last updated, ack state)
- Run history (timing, counts, errors)
- Backoff state

## Troubleshooting

**No notifications appearing:**
- Verify `gh auth status` shows you're authenticated
- Check that orgs are configured (config file or `--org` flag)
- Run `pr-attention status` to see if PRs are being found
- On Linux, ensure `notify-send` is installed

**Rate limiting:**
- The tool respects GitHub's rate limits via `gh` CLI
- If hitting limits, increase the poll interval in your scheduler

**Wrong GitHub host:**
- Set `gh_host` in config or `GH_HOST` environment variable
- Verify with `gh api user` against your host
