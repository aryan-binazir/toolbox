package github

type GHSearchResult struct {
	ID         string       `json:"id"`
	URL        string       `json:"url"`
	Title      string       `json:"title"`
	Number     int          `json:"number"`
	Repository GHRepository `json:"repository"`
	UpdatedAt  string       `json:"updatedAt"`
	IsDraft    bool         `json:"isDraft"`
	Labels     []GHLabel    `json:"labels"`
	Author     GHAuthor     `json:"author"`
}

type GHRepository struct {
	Name      string `json:"name"`
	NameWithOwner string `json:"nameWithOwner"`
}

type GHLabel struct {
	Name string `json:"name"`
}

type GHAuthor struct {
	Login string `json:"login"`
}

type MergedPR struct {
	GHSearchResult
	Status string // "review_requested", "assigned", or "both"
}
