package github

func MergeResults(review, assigned []GHSearchResult, excludeDrafts bool, ignoreLabels, ignoreAuthors []string) map[string]*MergedPR {
	ignoreLabelSet := make(map[string]bool)
	for _, l := range ignoreLabels {
		ignoreLabelSet[l] = true
	}

	ignoreAuthorSet := make(map[string]bool)
	for _, a := range ignoreAuthors {
		ignoreAuthorSet[a] = true
	}

	shouldInclude := func(pr GHSearchResult) bool {
		if excludeDrafts && pr.IsDraft {
			return false
		}

		if ignoreAuthorSet[pr.Author.Login] {
			return false
		}

		for _, label := range pr.Labels {
			if ignoreLabelSet[label.Name] {
				return false
			}
		}

		return true
	}

	result := make(map[string]*MergedPR)

	for _, pr := range review {
		if !shouldInclude(pr) {
			continue
		}
		result[pr.ID] = &MergedPR{
			GHSearchResult: pr,
			Status:         "review_requested",
		}
	}

	for _, pr := range assigned {
		if !shouldInclude(pr) {
			continue
		}
		if existing, ok := result[pr.ID]; ok {
			existing.Status = "both"
		} else {
			result[pr.ID] = &MergedPR{
				GHSearchResult: pr,
				Status:         "assigned",
			}
		}
	}

	return result
}
