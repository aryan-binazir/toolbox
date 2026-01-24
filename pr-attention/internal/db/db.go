package db

import (
	"database/sql"
	"fmt"
	"os"
	"path/filepath"
	"time"

	_ "github.com/mattn/go-sqlite3"
)

type Database struct {
	conn *sql.DB
	path string
}

func Open(path string) (*Database, error) {
	dir := filepath.Dir(path)
	if err := os.MkdirAll(dir, 0755); err != nil {
		return nil, fmt.Errorf("creating database directory: %w", err)
	}

	conn, err := sql.Open("sqlite3", path)
	if err != nil {
		return nil, fmt.Errorf("opening database: %w", err)
	}

	db := &Database{
		conn: conn,
		path: path,
	}

	if err := InitSchema(db); err != nil {
		conn.Close()
		return nil, fmt.Errorf("initializing schema: %w", err)
	}

	return db, nil
}

func (db *Database) Close() error {
	return db.conn.Close()
}

func (db *Database) GetPR(prID string) (*PR, error) {
	row := db.conn.QueryRow(`
		SELECT pr_id, url, title, repo, number, first_seen, last_seen,
		       last_updated_at, last_notified_updated_at, current_status, is_active
		FROM prs WHERE pr_id = ?`, prID)

	pr, err := scanPR(row.Scan)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("scanning PR: %w", err)
	}
	return pr, nil
}

type scanFunc func(dest ...any) error

func scanPR(scan scanFunc) (*PR, error) {
	var pr PR
	var firstSeen, lastSeen, lastUpdatedAt string
	var lastNotifiedUpdatedAt sql.NullString
	var isActive int

	err := scan(
		&pr.PRID, &pr.URL, &pr.Title, &pr.Repo, &pr.Number,
		&firstSeen, &lastSeen, &lastUpdatedAt,
		&lastNotifiedUpdatedAt, &pr.CurrentStatus, &isActive,
	)
	if err != nil {
		return nil, err
	}

	pr.FirstSeen, _ = time.Parse(time.RFC3339, firstSeen)
	pr.LastSeen, _ = time.Parse(time.RFC3339, lastSeen)
	pr.LastUpdatedAt, _ = time.Parse(time.RFC3339, lastUpdatedAt)
	pr.IsActive = isActive == 1

	if lastNotifiedUpdatedAt.Valid {
		t, _ := time.Parse(time.RFC3339, lastNotifiedUpdatedAt.String)
		pr.LastNotifiedUpdatedAt = sql.NullTime{Time: t, Valid: true}
	}

	return &pr, nil
}

func (db *Database) UpsertPR(pr *PR) error {
	var lastNotifiedUpdatedAt sql.NullString
	if pr.LastNotifiedUpdatedAt.Valid {
		lastNotifiedUpdatedAt = sql.NullString{String: pr.LastNotifiedUpdatedAt.Time.Format(time.RFC3339), Valid: true}
	}

	_, err := db.conn.Exec(`
		INSERT INTO prs (pr_id, url, title, repo, number, first_seen, last_seen,
		                 last_updated_at, last_notified_updated_at, current_status, is_active)
		VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
		ON CONFLICT(pr_id) DO UPDATE SET
		    url = excluded.url,
		    title = excluded.title,
		    last_seen = excluded.last_seen,
		    last_updated_at = excluded.last_updated_at,
		    last_notified_updated_at = excluded.last_notified_updated_at,
		    current_status = excluded.current_status,
		    is_active = excluded.is_active`,
		pr.PRID, pr.URL, pr.Title, pr.Repo, pr.Number,
		pr.FirstSeen.Format(time.RFC3339), pr.LastSeen.Format(time.RFC3339),
		pr.LastUpdatedAt.Format(time.RFC3339), lastNotifiedUpdatedAt,
		pr.CurrentStatus, boolToInt(pr.IsActive),
	)
	if err != nil {
		return fmt.Errorf("upserting PR: %w", err)
	}
	return nil
}

func (db *Database) GetActivePRs() ([]*PR, error) {
	rows, err := db.conn.Query(`
		SELECT pr_id, url, title, repo, number, first_seen, last_seen,
		       last_updated_at, last_notified_updated_at, current_status, is_active
		FROM prs WHERE is_active = 1`)
	if err != nil {
		return nil, fmt.Errorf("querying active PRs: %w", err)
	}
	defer rows.Close()

	var prs []*PR
	for rows.Next() {
		pr, err := scanPR(rows.Scan)
		if err != nil {
			return nil, fmt.Errorf("scanning PR row: %w", err)
		}
		prs = append(prs, pr)
	}

	if err := rows.Err(); err != nil {
		return nil, fmt.Errorf("iterating PR rows: %w", err)
	}

	return prs, nil
}

func (db *Database) MarkInactive(prID string) error {
	_, err := db.conn.Exec(`UPDATE prs SET is_active = 0 WHERE pr_id = ?`, prID)
	if err != nil {
		return fmt.Errorf("marking PR inactive: %w", err)
	}
	return nil
}

// SilencePR sets LastNotifiedUpdatedAt to the PR's current LastUpdatedAt,
// suppressing notifications until the PR is actually updated again.
func (db *Database) SilencePR(prID string) error {
	_, err := db.conn.Exec(`
		UPDATE prs SET last_notified_updated_at = last_updated_at WHERE pr_id = ?`, prID)
	if err != nil {
		return fmt.Errorf("silencing PR: %w", err)
	}
	return nil
}

func (db *Database) LogRun(prsFound, notificationsSent int, errMsg string, durationMs int64) error {
	var errMsgVal sql.NullString
	if errMsg != "" {
		errMsgVal = sql.NullString{String: errMsg, Valid: true}
	}

	_, err := db.conn.Exec(`
		INSERT INTO run_log (run_at, prs_found, notifications_sent, error_message, duration_ms)
		VALUES (?, ?, ?, ?, ?)`,
		time.Now().Format(time.RFC3339), prsFound, notificationsSent, errMsgVal, durationMs)
	if err != nil {
		return fmt.Errorf("logging run: %w", err)
	}
	return nil
}

func (db *Database) GetLastRun() (*RunLog, error) {
	row := db.conn.QueryRow(`
		SELECT id, run_at, prs_found, notifications_sent, error_message, duration_ms
		FROM run_log ORDER BY id DESC LIMIT 1`)

	var log RunLog
	var runAt string

	err := row.Scan(
		&log.ID, &runAt, &log.PRsFound, &log.NotificationsSent,
		&log.ErrorMessage, &log.DurationMs,
	)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("scanning run log: %w", err)
	}

	log.RunAt, _ = time.Parse(time.RFC3339, runAt)
	return &log, nil
}

func (db *Database) GetBackoffState() (*BackoffState, error) {
	row := db.conn.QueryRow(`
		SELECT consecutive_failures, last_failure_time
		FROM backoff_state WHERE id = 1`)

	var state BackoffState
	var lastFailureTime sql.NullString

	err := row.Scan(&state.ConsecutiveFailures, &lastFailureTime)
	if err == sql.ErrNoRows {
		return &BackoffState{}, nil
	}
	if err != nil {
		return nil, fmt.Errorf("loading backoff state: %w", err)
	}

	if lastFailureTime.Valid {
		t, _ := time.Parse(time.RFC3339, lastFailureTime.String)
		state.LastFailureTime = sql.NullTime{Time: t, Valid: true}
	}

	return &state, nil
}

func (db *Database) SaveBackoffState(state *BackoffState) error {
	var lastFailureTime sql.NullString
	if state.LastFailureTime.Valid {
		lastFailureTime = sql.NullString{
			String: state.LastFailureTime.Time.Format(time.RFC3339),
			Valid:  true,
		}
	}

	_, err := db.conn.Exec(`
		INSERT INTO backoff_state (id, consecutive_failures, last_failure_time)
		VALUES (1, ?, ?)
		ON CONFLICT(id) DO UPDATE SET
		    consecutive_failures = excluded.consecutive_failures,
		    last_failure_time = excluded.last_failure_time`,
		state.ConsecutiveFailures, lastFailureTime)
	if err != nil {
		return fmt.Errorf("saving backoff state: %w", err)
	}

	return nil
}

func boolToInt(b bool) int {
	if b {
		return 1
	}
	return 0
}
