package github

import (
	"encoding/json"
	"fmt"
	"os/exec"
	"strings"
)

type Executor interface {
	Run(name string, args ...string) ([]byte, error)
}

type RealExecutor struct{}

func (e *RealExecutor) Run(name string, args ...string) ([]byte, error) {
	cmd := exec.Command(name, args...)
	return cmd.Output()
}

type Client struct {
	executor Executor
	ghHost   string
}

func NewClient(executor Executor, ghHost string) *Client {
	return &Client{
		executor: executor,
		ghHost:   ghHost,
	}
}

func (c *Client) SearchReviewRequested(orgs []string) ([]GHSearchResult, error) {
	return c.searchPRs("--review-requested=@me", orgs)
}

func (c *Client) SearchAssigned(orgs []string) ([]GHSearchResult, error) {
	return c.searchPRs("--assignee=@me", orgs)
}

func (c *Client) searchPRs(filter string, orgs []string) ([]GHSearchResult, error) {
	args := []string{"search", "prs", filter, "--state=open", "--limit=100",
		"--json", "id,url,title,repository,updatedAt,isDraft,number,labels,author"}

	if len(orgs) > 0 {
		args = append(args, "--owner="+strings.Join(orgs, ","))
	}

	var output []byte
	var err error

	if c.ghHost != "" {
		// Use env to set GH_HOST for the command
		output, err = c.executor.Run("env", append([]string{"GH_HOST=" + c.ghHost, "gh"}, args...)...)
	} else {
		output, err = c.executor.Run("gh", args...)
	}

	if err != nil {
		return nil, fmt.Errorf("executing gh search: %w", err)
	}

	var results []GHSearchResult
	if err := json.Unmarshal(output, &results); err != nil {
		return nil, fmt.Errorf("parsing gh search output: %w", err)
	}

	return results, nil
}
