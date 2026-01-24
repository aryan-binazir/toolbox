package github

import (
	"testing"
	"time"
)

func TestBackoffState_GetDelay_ZeroFailures(t *testing.T) {
	state := &BackoffState{
		ConsecutiveFailures: 0,
	}

	delay := state.GetDelay()

	if delay != 0 {
		t.Errorf("expected 0 delay for zero failures, got %v", delay)
	}
}

func TestBackoffState_GetDelay_ExponentialGrowth(t *testing.T) {
	tests := []struct {
		name        string
		failures    int
		minExpected time.Duration
		maxExpected time.Duration
	}{
		{
			name:        "1 failure = ~60s base",
			failures:    1,
			minExpected: 54 * time.Second,  // 60 - 10% jitter
			maxExpected: 66 * time.Second,  // 60 + 10% jitter
		},
		{
			name:        "2 failures = ~120s",
			failures:    2,
			minExpected: 108 * time.Second, // 120 - 10%
			maxExpected: 132 * time.Second, // 120 + 10%
		},
		{
			name:        "3 failures = ~240s",
			failures:    3,
			minExpected: 216 * time.Second, // 240 - 10%
			maxExpected: 264 * time.Second, // 240 + 10%
		},
		{
			name:        "4 failures = ~480s",
			failures:    4,
			minExpected: 432 * time.Second, // 480 - 10%
			maxExpected: 528 * time.Second, // 480 + 10%
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			state := &BackoffState{
				ConsecutiveFailures: tt.failures,
			}

			// Run multiple times due to jitter randomness
			for i := 0; i < 10; i++ {
				delay := state.GetDelay()

				if delay < tt.minExpected || delay > tt.maxExpected {
					t.Errorf("delay %v outside expected range [%v, %v]",
						delay, tt.minExpected, tt.maxExpected)
				}
			}
		})
	}
}

func TestBackoffState_GetDelay_MaxCap(t *testing.T) {
	tests := []struct {
		name     string
		failures int
	}{
		// 6 failures = 60 * 2^5 = 1920s (not capped)
		// 7 failures = 60 * 2^6 = 3840s (exceeds 3600s cap)
		{"7 failures (would be 3840s without cap)", 7},
		{"10 failures (way over cap)", 10},
		{"20 failures (extreme)", 20},
	}

	maxDelay := 3600 * time.Second
	maxWithJitter := maxDelay + (maxDelay / 10) // 3960s

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			state := &BackoffState{
				ConsecutiveFailures: tt.failures,
			}

			for i := 0; i < 10; i++ {
				delay := state.GetDelay()

				if delay > maxWithJitter {
					t.Errorf("delay %v exceeds max cap %v (with jitter)", delay, maxWithJitter)
				}

				// Should be at least 90% of max (3240s)
				minExpected := maxDelay - (maxDelay / 10)
				if delay < minExpected {
					t.Errorf("delay %v below expected minimum %v for capped value", delay, minExpected)
				}
			}
		})
	}
}

func TestBackoffState_RecordSuccess_Resets(t *testing.T) {
	state := &BackoffState{
		ConsecutiveFailures: 5,
	}

	state.RecordSuccess()

	if state.ConsecutiveFailures != 0 {
		t.Errorf("expected ConsecutiveFailures to be 0 after success, got %d", state.ConsecutiveFailures)
	}

	if state.LastFailureTime.Valid {
		t.Error("expected LastFailureTime to be invalid after success")
	}

	// Verify delay is now 0
	delay := state.GetDelay()
	if delay != 0 {
		t.Errorf("expected 0 delay after success, got %v", delay)
	}
}

func TestBackoffState_RecordFailure_Increments(t *testing.T) {
	state := &BackoffState{
		ConsecutiveFailures: 0,
	}

	// First failure
	state.RecordFailure()

	if state.ConsecutiveFailures != 1 {
		t.Errorf("expected 1 failure after first RecordFailure, got %d", state.ConsecutiveFailures)
	}

	if !state.LastFailureTime.Valid {
		t.Error("expected LastFailureTime to be valid after failure")
	}

	firstFailureTime := state.LastFailureTime.Time

	// Small delay to ensure time difference
	time.Sleep(10 * time.Millisecond)

	// Second failure
	state.RecordFailure()

	if state.ConsecutiveFailures != 2 {
		t.Errorf("expected 2 failures after second RecordFailure, got %d", state.ConsecutiveFailures)
	}

	if !state.LastFailureTime.Time.After(firstFailureTime) {
		t.Error("expected LastFailureTime to be updated on second failure")
	}

	// Third failure
	state.RecordFailure()

	if state.ConsecutiveFailures != 3 {
		t.Errorf("expected 3 failures after third RecordFailure, got %d", state.ConsecutiveFailures)
	}
}
