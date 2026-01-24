package github

import (
	"testing"
)

func TestMergeResults_CombinesStatus(t *testing.T) {
	tests := []struct {
		name     string
		review   []GHSearchResult
		assigned []GHSearchResult
		wantID   string
		wantStat string
	}{
		{
			name: "same PR in both lists gets status both",
			review: []GHSearchResult{
				{ID: "PR_abc123", Title: "Add feature", Number: 42, Author: GHAuthor{Login: "dev1"}},
			},
			assigned: []GHSearchResult{
				{ID: "PR_abc123", Title: "Add feature", Number: 42, Author: GHAuthor{Login: "dev1"}},
			},
			wantID:   "PR_abc123",
			wantStat: "both",
		},
		{
			name: "review only gets review_requested status",
			review: []GHSearchResult{
				{ID: "PR_review", Title: "Review me", Number: 10, Author: GHAuthor{Login: "dev1"}},
			},
			assigned: []GHSearchResult{},
			wantID:   "PR_review",
			wantStat: "review_requested",
		},
		{
			name:   "assigned only gets assigned status",
			review: []GHSearchResult{},
			assigned: []GHSearchResult{
				{ID: "PR_assigned", Title: "Assigned", Number: 11, Author: GHAuthor{Login: "dev2"}},
			},
			wantID:   "PR_assigned",
			wantStat: "assigned",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := MergeResults(tt.review, tt.assigned, false, nil, nil)

			if len(result) != 1 {
				t.Fatalf("expected 1 result, got %d", len(result))
			}

			pr, ok := result[tt.wantID]
			if !ok {
				t.Fatalf("expected PR with ID %s", tt.wantID)
			}

			if pr.Status != tt.wantStat {
				t.Errorf("expected status %q, got %q", tt.wantStat, pr.Status)
			}
		})
	}
}

func TestMergeResults_ExcludesDrafts(t *testing.T) {
	tests := []struct {
		name          string
		review        []GHSearchResult
		assigned      []GHSearchResult
		excludeDrafts bool
		wantCount     int
	}{
		{
			name: "excludes drafts when flag is true",
			review: []GHSearchResult{
				{ID: "PR_draft", Title: "WIP", IsDraft: true, Author: GHAuthor{Login: "dev1"}},
				{ID: "PR_ready", Title: "Ready", IsDraft: false, Author: GHAuthor{Login: "dev1"}},
			},
			assigned:      []GHSearchResult{},
			excludeDrafts: true,
			wantCount:     1,
		},
		{
			name: "includes drafts when flag is false",
			review: []GHSearchResult{
				{ID: "PR_draft", Title: "WIP", IsDraft: true, Author: GHAuthor{Login: "dev1"}},
				{ID: "PR_ready", Title: "Ready", IsDraft: false, Author: GHAuthor{Login: "dev1"}},
			},
			assigned:      []GHSearchResult{},
			excludeDrafts: false,
			wantCount:     2,
		},
		{
			name: "excludes draft from assigned list too",
			review: []GHSearchResult{},
			assigned: []GHSearchResult{
				{ID: "PR_draft", Title: "WIP", IsDraft: true, Author: GHAuthor{Login: "dev1"}},
			},
			excludeDrafts: true,
			wantCount:     0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := MergeResults(tt.review, tt.assigned, tt.excludeDrafts, nil, nil)

			if len(result) != tt.wantCount {
				t.Errorf("expected %d results, got %d", tt.wantCount, len(result))
			}
		})
	}
}

func TestMergeResults_IgnoresLabels(t *testing.T) {
	tests := []struct {
		name         string
		review       []GHSearchResult
		ignoreLabels []string
		wantCount    int
	}{
		{
			name: "excludes PR with ignored label",
			review: []GHSearchResult{
				{ID: "PR_wip", Title: "WIP", Labels: []GHLabel{{Name: "WIP"}}, Author: GHAuthor{Login: "dev1"}},
				{ID: "PR_ready", Title: "Ready", Labels: []GHLabel{}, Author: GHAuthor{Login: "dev1"}},
			},
			ignoreLabels: []string{"WIP"},
			wantCount:    1,
		},
		{
			name: "case sensitive label matching",
			review: []GHSearchResult{
				{ID: "PR_wip", Title: "WIP", Labels: []GHLabel{{Name: "wip"}}, Author: GHAuthor{Login: "dev1"}},
			},
			ignoreLabels: []string{"WIP"},
			wantCount:    1, // "wip" != "WIP"
		},
		{
			name: "excludes PR with any matching label",
			review: []GHSearchResult{
				{ID: "PR_multi", Title: "Multi", Labels: []GHLabel{{Name: "ready"}, {Name: "do-not-review"}}, Author: GHAuthor{Login: "dev1"}},
			},
			ignoreLabels: []string{"do-not-review"},
			wantCount:    0,
		},
		{
			name: "empty ignore list includes all",
			review: []GHSearchResult{
				{ID: "PR_labeled", Title: "Labeled", Labels: []GHLabel{{Name: "WIP"}}, Author: GHAuthor{Login: "dev1"}},
			},
			ignoreLabels: []string{},
			wantCount:    1,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := MergeResults(tt.review, nil, false, tt.ignoreLabels, nil)

			if len(result) != tt.wantCount {
				t.Errorf("expected %d results, got %d", tt.wantCount, len(result))
			}
		})
	}
}

func TestMergeResults_IgnoresAuthors(t *testing.T) {
	tests := []struct {
		name          string
		review        []GHSearchResult
		ignoreAuthors []string
		wantCount     int
	}{
		{
			name: "excludes PR from ignored author",
			review: []GHSearchResult{
				{ID: "PR_bot", Title: "Bump deps", Author: GHAuthor{Login: "dependabot[bot]"}},
				{ID: "PR_human", Title: "Feature", Author: GHAuthor{Login: "developer"}},
			},
			ignoreAuthors: []string{"dependabot[bot]"},
			wantCount:     1,
		},
		{
			name: "excludes multiple ignored authors",
			review: []GHSearchResult{
				{ID: "PR_bot1", Title: "Bump deps", Author: GHAuthor{Login: "dependabot[bot]"}},
				{ID: "PR_bot2", Title: "Renovate", Author: GHAuthor{Login: "renovate[bot]"}},
				{ID: "PR_human", Title: "Feature", Author: GHAuthor{Login: "developer"}},
			},
			ignoreAuthors: []string{"dependabot[bot]", "renovate[bot]"},
			wantCount:     1,
		},
		{
			name: "empty ignore list includes all",
			review: []GHSearchResult{
				{ID: "PR_bot", Title: "Bump deps", Author: GHAuthor{Login: "dependabot[bot]"}},
			},
			ignoreAuthors: []string{},
			wantCount:     1,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := MergeResults(tt.review, nil, false, nil, tt.ignoreAuthors)

			if len(result) != tt.wantCount {
				t.Errorf("expected %d results, got %d", tt.wantCount, len(result))
			}
		})
	}
}
