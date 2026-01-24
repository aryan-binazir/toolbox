package cmd

import (
	"fmt"

	"github.com/spf13/cobra"

	"pr-attention/internal/config"
	"pr-attention/internal/db"
)

var ackCmd = &cobra.Command{
	Use:   "ack <pr-reference>",
	Short: "Acknowledge a PR (suppress notifications until it's updated)",
	Long: `Acknowledge a PR to suppress notifications until the PR is updated.

PR reference can be:
  - Full URL: https://github.com/org/repo/pull/123
  - Short form: org/repo#123
  - PR ID from the database`,
	Args: cobra.ExactArgs(1),
	RunE: runAck,
}

func init() {
	rootCmd.AddCommand(ackCmd)
}

func runAck(cmd *cobra.Command, args []string) error {
	ref := args[0]

	cfg, err := config.Load(GetConfigPath(), GetOrgs())
	if err != nil {
		return fmt.Errorf("loading config: %w", err)
	}

	database, err := db.Open(cfg.DBPath)
	if err != nil {
		return fmt.Errorf("opening database: %w", err)
	}
	defer database.Close()

	// Parse the reference and find the PR
	repo, number, prID, err := parseRef(ref)
	if err != nil {
		return err
	}

	var pr *db.PR

	if prID != "" {
		// Direct PR ID lookup
		pr, err = database.GetPR(prID)
		if err != nil {
			return fmt.Errorf("getting PR: %w", err)
		}
	} else {
		// Find by repo and number
		pr, err = findPRByRepoAndNumber(database, repo, number)
		if err != nil {
			return err
		}
	}

	if pr == nil {
		return fmt.Errorf("PR not found: %s", ref)
	}

	if !pr.IsActive {
		return fmt.Errorf("PR is not in attention queue: %s", ref)
	}

	// Silence until next update
	if err := database.SilencePR(pr.PRID); err != nil {
		return fmt.Errorf("silencing PR: %w", err)
	}

	fmt.Printf("Silenced: %s#%d - %s\n", pr.Repo, pr.Number, pr.Title)
	fmt.Println("No more notifications until PR is updated.")

	return nil
}

func findPRByRepoAndNumber(database *db.Database, repo string, number int) (*db.PR, error) {
	activePRs, err := database.GetActivePRs()
	if err != nil {
		return nil, fmt.Errorf("getting active PRs: %w", err)
	}

	for _, pr := range activePRs {
		if pr.Repo == repo && pr.Number == number {
			return pr, nil
		}
	}

	return nil, nil
}
