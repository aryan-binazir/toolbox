package github

import (
	"database/sql"
	"math"
	"math/rand"
	"time"

	"pr-attention/internal/db"
)

const (
	baseDelaySeconds = 60
	maxDelaySeconds  = 3600
)

type BackoffState struct {
	ConsecutiveFailures int
	LastFailureTime     sql.NullTime
}

func (b *BackoffState) GetDelay() time.Duration {
	if b.ConsecutiveFailures == 0 {
		return 0
	}

	delay := float64(baseDelaySeconds) * math.Pow(2, float64(b.ConsecutiveFailures-1))
	if delay > maxDelaySeconds {
		delay = maxDelaySeconds
	}

	// Add +/-10% jitter
	jitter := delay * 0.1 * (2*rand.Float64() - 1)
	delay += jitter

	return time.Duration(delay) * time.Second
}

func (b *BackoffState) RecordSuccess() {
	b.ConsecutiveFailures = 0
	b.LastFailureTime = sql.NullTime{}
}

func (b *BackoffState) RecordFailure() {
	b.ConsecutiveFailures++
	b.LastFailureTime = sql.NullTime{Time: time.Now(), Valid: true}
}

func LoadBackoffState(database *db.Database) (*BackoffState, error) {
	dbState, err := database.GetBackoffState()
	if err != nil {
		return nil, err
	}
	return &BackoffState{
		ConsecutiveFailures: dbState.ConsecutiveFailures,
		LastFailureTime:     dbState.LastFailureTime,
	}, nil
}

func SaveBackoffState(database *db.Database, state *BackoffState) error {
	dbState := &db.BackoffState{
		ConsecutiveFailures: state.ConsecutiveFailures,
		LastFailureTime:     state.LastFailureTime,
	}
	return database.SaveBackoffState(dbState)
}
