package poller

import (
	"database/sql"
	"fmt"
	"strings"
	"time"

	"pr-attention/internal/config"
	"pr-attention/internal/db"
	"pr-attention/internal/github"
	"pr-attention/internal/notify"
)

type NotificationReason int

const (
	ReasonNew NotificationReason = iota
	ReasonUpdated
)

func (r NotificationReason) String() string {
	switch r {
	case ReasonNew:
		return "new"
	case ReasonUpdated:
		return "updated"
	default:
		return "unknown"
	}
}

type PRChange struct {
	PR     *db.PR
	Reason NotificationReason
}

type PollResult struct {
	TotalPRs          int
	NotificationsSent int
	Changes           []PRChange
	Skipped           bool
	SkipReason        string
}

type PollOptions struct {
	IncludeDrafts bool
}

func Poll(database *db.Database, cfg *config.Config, ghClient *github.Client, opts *PollOptions) (*PollResult, error) {
	startTime := time.Now()

	// Check backoff state
	backoff, err := github.LoadBackoffState(database)
	if err != nil {
		return nil, fmt.Errorf("loading backoff state: %w", err)
	}

	if backoff.ConsecutiveFailures > 0 && backoff.LastFailureTime.Valid {
		delay := backoff.GetDelay()
		elapsed := time.Since(backoff.LastFailureTime.Time)
		if elapsed < delay {
			remaining := delay - elapsed
			return &PollResult{
				Skipped:    true,
				SkipReason: fmt.Sprintf("in backoff, retry in %s", remaining.Round(time.Second)),
			}, nil
		}
	}

	result, pollErr := doPoll(database, cfg, ghClient, opts)

	duration := time.Since(startTime).Milliseconds()

	// Update backoff state and log run
	if pollErr != nil {
		backoff.RecordFailure()
		_ = github.SaveBackoffState(database, backoff)
		_ = database.LogRun(0, 0, pollErr.Error(), duration)
		return nil, pollErr
	}

	backoff.RecordSuccess()
	_ = github.SaveBackoffState(database, backoff)
	_ = database.LogRun(result.TotalPRs, result.NotificationsSent, "", duration)

	return result, nil
}

func doPoll(database *db.Database, cfg *config.Config, ghClient *github.Client, opts *PollOptions) (*PollResult, error) {
	// Fetch PRs from GitHub
	reviewPRs, err := ghClient.SearchReviewRequested(cfg.Orgs)
	if err != nil {
		return nil, fmt.Errorf("searching review-requested PRs: %w", err)
	}

	assignedPRs, err := ghClient.SearchAssigned(cfg.Orgs)
	if err != nil {
		return nil, fmt.Errorf("searching assigned PRs: %w", err)
	}

	// Determine excludeDrafts setting
	excludeDrafts := cfg.ExcludeDrafts
	if opts != nil && opts.IncludeDrafts {
		excludeDrafts = false
	}

	// Merge and filter results
	merged := github.MergeResults(reviewPRs, assignedPRs, excludeDrafts, cfg.IgnoreLabels, cfg.IgnoreAuthors)

	// Track which PR IDs we saw in this poll
	seenIDs := make(map[string]bool)
	for id := range merged {
		seenIDs[id] = true
	}

	// Get current active PRs from database
	activePRs, err := database.GetActivePRs()
	if err != nil {
		return nil, fmt.Errorf("getting active PRs: %w", err)
	}

	var changes []PRChange
	now := time.Now()

	// Process each PR from GitHub
	for prID, ghPR := range merged {
		updatedAt, _ := time.Parse(time.RFC3339, ghPR.UpdatedAt)

		existingPR, err := database.GetPR(prID)
		if err != nil {
			return nil, fmt.Errorf("getting PR %s: %w", prID, err)
		}

		reason := determineNotificationReason(existingPR, updatedAt)

		// Build the PR record
		dbPR := &db.PR{
			PRID:          prID,
			URL:           ghPR.URL,
			Title:         ghPR.Title,
			Repo:          ghPR.Repository.NameWithOwner,
			Number:        ghPR.Number,
			LastSeen:      now,
			LastUpdatedAt: updatedAt,
			CurrentStatus: ghPR.Status,
			IsActive:      true,
		}

		if existingPR != nil {
			dbPR.FirstSeen = existingPR.FirstSeen
			dbPR.LastNotifiedUpdatedAt = existingPR.LastNotifiedUpdatedAt
		} else {
			dbPR.FirstSeen = now
		}

		if reason != nil {
			dbPR.LastNotifiedUpdatedAt = sql.NullTime{Time: updatedAt, Valid: true}
			changes = append(changes, PRChange{PR: dbPR, Reason: *reason})
		}

		if err := database.UpsertPR(dbPR); err != nil {
			return nil, fmt.Errorf("upserting PR %s: %w", prID, err)
		}
	}

	// Mark PRs that are no longer in the poll as inactive
	for _, pr := range activePRs {
		if !seenIDs[pr.PRID] {
			if err := database.MarkInactive(pr.PRID); err != nil {
				return nil, fmt.Errorf("marking PR %s inactive: %w", pr.PRID, err)
			}
		}
	}

	if len(changes) > 0 {
		_ = notify.Send("PRs Need Attention", buildNotificationBody(changes))
		if cfg.SoundEnabled {
			notify.PlaySound()
		}
	}

	return &PollResult{
		TotalPRs:          len(merged),
		NotificationsSent: len(changes),
		Changes:           changes,
	}, nil
}

func determineNotificationReason(existingPR *db.PR, updatedAt time.Time) *NotificationReason {
	if existingPR == nil || !existingPR.LastNotifiedUpdatedAt.Valid {
		reason := ReasonNew
		return &reason
	}
	if updatedAt.After(existingPR.LastNotifiedUpdatedAt.Time) {
		reason := ReasonUpdated
		return &reason
	}
	return nil
}

func buildNotificationBody(changes []PRChange) string {
	newCount := 0
	updatedCount := 0

	for _, c := range changes {
		switch c.Reason {
		case ReasonNew:
			newCount++
		case ReasonUpdated:
			updatedCount++
		}
	}

	var parts []string
	if newCount > 0 {
		parts = append(parts, fmt.Sprintf("%d new", newCount))
	}
	if updatedCount > 0 {
		parts = append(parts, fmt.Sprintf("%d updated", updatedCount))
	}

	summary := strings.Join(parts, ", ")

	if len(changes) <= 3 {
		var titles []string
		for _, c := range changes {
			titles = append(titles, fmt.Sprintf("• %s", c.PR.Title))
		}
		return summary + "\n" + strings.Join(titles, "\n")
	}

	return summary
}
