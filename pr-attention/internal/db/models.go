package db

import (
	"database/sql"
	"time"
)

type PR struct {
	PRID                   string
	URL                    string
	Title                  string
	Repo                   string
	Number                 int
	FirstSeen              time.Time
	LastSeen               time.Time
	LastUpdatedAt          time.Time       // GitHub's updatedAt
	LastNotifiedUpdatedAt  sql.NullTime    // updatedAt value when we last notified
	CurrentStatus          string
	IsActive               bool
}

type RunLog struct {
	ID                int64
	RunAt             time.Time
	PRsFound          int
	NotificationsSent int
	ErrorMessage      sql.NullString
	DurationMs        sql.NullInt64
}

type BackoffState struct {
	ConsecutiveFailures int
	LastFailureTime     sql.NullTime
}
