package db

import (
	"database/sql"
	"path/filepath"
	"testing"
	"time"
)

func TestDatabase_PRLifecycle(t *testing.T) {
	dbPath := filepath.Join(t.TempDir(), "test.db")
	database, err := Open(dbPath)
	if err != nil {
		t.Fatalf("failed to open database: %v", err)
	}
	defer database.Close()

	// Create a PR
	now := time.Now().Truncate(time.Second)
	pr := &PR{
		PRID:          "PR_test123",
		URL:           "https://github.com/org/repo/pull/42",
		Title:         "Test PR",
		Repo:          "org/repo",
		Number:        42,
		FirstSeen:     now,
		LastSeen:      now,
		LastUpdatedAt: now,
		CurrentStatus: "review_requested",
		IsActive:      true,
	}

	// Insert
	if err := database.UpsertPR(pr); err != nil {
		t.Fatalf("failed to insert PR: %v", err)
	}

	// Get and verify
	got, err := database.GetPR("PR_test123")
	if err != nil {
		t.Fatalf("failed to get PR: %v", err)
	}
	if got == nil {
		t.Fatal("expected PR, got nil")
	}
	if got.PRID != "PR_test123" {
		t.Errorf("expected PRID %q, got %q", "PR_test123", got.PRID)
	}
	if got.Title != "Test PR" {
		t.Errorf("expected Title %q, got %q", "Test PR", got.Title)
	}
	if got.CurrentStatus != "review_requested" {
		t.Errorf("expected status %q, got %q", "review_requested", got.CurrentStatus)
	}
	if !got.IsActive {
		t.Error("expected IsActive to be true")
	}

	// Update
	pr.CurrentStatus = "both"
	pr.LastSeen = now.Add(time.Hour)
	if err := database.UpsertPR(pr); err != nil {
		t.Fatalf("failed to update PR: %v", err)
	}

	got, err = database.GetPR("PR_test123")
	if err != nil {
		t.Fatalf("failed to get updated PR: %v", err)
	}
	if got.CurrentStatus != "both" {
		t.Errorf("expected updated status %q, got %q", "both", got.CurrentStatus)
	}

	// Get active PRs
	activePRs, err := database.GetActivePRs()
	if err != nil {
		t.Fatalf("failed to get active PRs: %v", err)
	}
	if len(activePRs) != 1 {
		t.Fatalf("expected 1 active PR, got %d", len(activePRs))
	}
	if activePRs[0].PRID != "PR_test123" {
		t.Errorf("expected PRID %q, got %q", "PR_test123", activePRs[0].PRID)
	}

	// Mark inactive
	if err := database.MarkInactive("PR_test123"); err != nil {
		t.Fatalf("failed to mark inactive: %v", err)
	}

	activePRs, err = database.GetActivePRs()
	if err != nil {
		t.Fatalf("failed to get active PRs after marking inactive: %v", err)
	}
	if len(activePRs) != 0 {
		t.Errorf("expected 0 active PRs after marking inactive, got %d", len(activePRs))
	}

	// PR should still exist but be inactive
	got, err = database.GetPR("PR_test123")
	if err != nil {
		t.Fatalf("failed to get inactive PR: %v", err)
	}
	if got.IsActive {
		t.Error("expected IsActive to be false after marking inactive")
	}
}

func TestDatabase_SilencePR(t *testing.T) {
	dbPath := filepath.Join(t.TempDir(), "test.db")
	database, err := Open(dbPath)
	if err != nil {
		t.Fatalf("failed to open database: %v", err)
	}
	defer database.Close()

	now := time.Now().Truncate(time.Second)
	pr := &PR{
		PRID:          "PR_silence",
		URL:           "https://github.com/org/repo/pull/99",
		Title:         "Silence Test",
		Repo:          "org/repo",
		Number:        99,
		FirstSeen:     now,
		LastSeen:      now,
		LastUpdatedAt: now,
		CurrentStatus: "review_requested",
		IsActive:      true,
	}

	if err := database.UpsertPR(pr); err != nil {
		t.Fatalf("failed to insert PR: %v", err)
	}

	// Initially should have no notification record
	got, _ := database.GetPR("PR_silence")
	if got.LastNotifiedUpdatedAt.Valid {
		t.Error("expected LastNotifiedUpdatedAt to be invalid initially")
	}

	// Silence the PR (sets LastNotifiedUpdatedAt = LastUpdatedAt)
	if err := database.SilencePR("PR_silence"); err != nil {
		t.Fatalf("failed to silence PR: %v", err)
	}

	// Verify silence
	got, err = database.GetPR("PR_silence")
	if err != nil {
		t.Fatalf("failed to get silenced PR: %v", err)
	}
	if !got.LastNotifiedUpdatedAt.Valid {
		t.Fatal("expected LastNotifiedUpdatedAt to be valid after silence")
	}
	if !got.LastNotifiedUpdatedAt.Time.Equal(now) {
		t.Errorf("expected LastNotifiedUpdatedAt %v, got %v", now, got.LastNotifiedUpdatedAt.Time)
	}
}

func TestDatabase_RunLog(t *testing.T) {
	dbPath := filepath.Join(t.TempDir(), "test.db")
	database, err := Open(dbPath)
	if err != nil {
		t.Fatalf("failed to open database: %v", err)
	}
	defer database.Close()

	// Initially no runs
	lastRun, err := database.GetLastRun()
	if err != nil {
		t.Fatalf("failed to get last run: %v", err)
	}
	if lastRun != nil {
		t.Error("expected nil for last run with no runs")
	}

	// Log a successful run
	if err := database.LogRun(5, 2, "", 150); err != nil {
		t.Fatalf("failed to log run: %v", err)
	}

	lastRun, err = database.GetLastRun()
	if err != nil {
		t.Fatalf("failed to get last run after logging: %v", err)
	}
	if lastRun == nil {
		t.Fatal("expected run log, got nil")
	}
	if lastRun.PRsFound != 5 {
		t.Errorf("expected PRsFound 5, got %d", lastRun.PRsFound)
	}
	if lastRun.NotificationsSent != 2 {
		t.Errorf("expected NotificationsSent 2, got %d", lastRun.NotificationsSent)
	}
	if lastRun.ErrorMessage.Valid {
		t.Error("expected no error message for successful run")
	}
	if !lastRun.DurationMs.Valid || lastRun.DurationMs.Int64 != 150 {
		t.Errorf("expected DurationMs 150, got %v", lastRun.DurationMs)
	}

	// Log a run with error
	if err := database.LogRun(0, 0, "API rate limited", 50); err != nil {
		t.Fatalf("failed to log error run: %v", err)
	}

	lastRun, err = database.GetLastRun()
	if err != nil {
		t.Fatalf("failed to get last run after error: %v", err)
	}
	if !lastRun.ErrorMessage.Valid {
		t.Fatal("expected error message for failed run")
	}
	if lastRun.ErrorMessage.String != "API rate limited" {
		t.Errorf("expected error message %q, got %q", "API rate limited", lastRun.ErrorMessage.String)
	}
}

func TestDatabase_BackoffState(t *testing.T) {
	dbPath := filepath.Join(t.TempDir(), "test.db")
	database, err := Open(dbPath)
	if err != nil {
		t.Fatalf("failed to open database: %v", err)
	}
	defer database.Close()

	// Initial state should have zero failures
	state, err := database.GetBackoffState()
	if err != nil {
		t.Fatalf("failed to get backoff state: %v", err)
	}
	if state.ConsecutiveFailures != 0 {
		t.Errorf("expected 0 failures initially, got %d", state.ConsecutiveFailures)
	}

	// Save state with failures
	now := time.Now().Truncate(time.Second)
	state = &BackoffState{
		ConsecutiveFailures: 3,
		LastFailureTime:     sql.NullTime{Time: now, Valid: true},
	}
	if err := database.SaveBackoffState(state); err != nil {
		t.Fatalf("failed to save backoff state: %v", err)
	}

	// Retrieve and verify
	got, err := database.GetBackoffState()
	if err != nil {
		t.Fatalf("failed to get backoff state: %v", err)
	}
	if got.ConsecutiveFailures != 3 {
		t.Errorf("expected 3 failures, got %d", got.ConsecutiveFailures)
	}
	if !got.LastFailureTime.Valid {
		t.Fatal("expected LastFailureTime to be valid")
	}

	// Reset state
	state = &BackoffState{
		ConsecutiveFailures: 0,
		LastFailureTime:     sql.NullTime{},
	}
	if err := database.SaveBackoffState(state); err != nil {
		t.Fatalf("failed to save reset state: %v", err)
	}

	got, err = database.GetBackoffState()
	if err != nil {
		t.Fatalf("failed to get reset state: %v", err)
	}
	if got.ConsecutiveFailures != 0 {
		t.Errorf("expected 0 failures after reset, got %d", got.ConsecutiveFailures)
	}
}

func TestDatabase_GetPR_NotFound(t *testing.T) {
	dbPath := filepath.Join(t.TempDir(), "test.db")
	database, err := Open(dbPath)
	if err != nil {
		t.Fatalf("failed to open database: %v", err)
	}
	defer database.Close()

	pr, err := database.GetPR("nonexistent")
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if pr != nil {
		t.Error("expected nil for nonexistent PR")
	}
}

func TestDatabase_MultiplePRs(t *testing.T) {
	dbPath := filepath.Join(t.TempDir(), "test.db")
	database, err := Open(dbPath)
	if err != nil {
		t.Fatalf("failed to open database: %v", err)
	}
	defer database.Close()

	now := time.Now().Truncate(time.Second)

	// Insert multiple PRs
	prs := []*PR{
		{PRID: "PR_1", URL: "https://github.com/a/b/pull/1", Title: "PR 1", Repo: "a/b", Number: 1, FirstSeen: now, LastSeen: now, LastUpdatedAt: now, CurrentStatus: "review_requested", IsActive: true},
		{PRID: "PR_2", URL: "https://github.com/a/b/pull/2", Title: "PR 2", Repo: "a/b", Number: 2, FirstSeen: now, LastSeen: now, LastUpdatedAt: now, CurrentStatus: "assigned", IsActive: true},
		{PRID: "PR_3", URL: "https://github.com/c/d/pull/3", Title: "PR 3", Repo: "c/d", Number: 3, FirstSeen: now, LastSeen: now, LastUpdatedAt: now, CurrentStatus: "both", IsActive: false},
	}

	for _, pr := range prs {
		if err := database.UpsertPR(pr); err != nil {
			t.Fatalf("failed to insert PR %s: %v", pr.PRID, err)
		}
	}

	// Get active PRs (should be 2)
	activePRs, err := database.GetActivePRs()
	if err != nil {
		t.Fatalf("failed to get active PRs: %v", err)
	}
	if len(activePRs) != 2 {
		t.Errorf("expected 2 active PRs, got %d", len(activePRs))
	}
}
