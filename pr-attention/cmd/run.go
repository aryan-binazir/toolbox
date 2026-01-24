package cmd

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"

	"pr-attention/internal/config"
	"pr-attention/internal/db"
	"pr-attention/internal/github"
	"pr-attention/internal/poller"
)

var includeDrafts bool

var runCmd = &cobra.Command{
	Use:   "run",
	Short: "Poll GitHub for PRs needing attention",
	Long: `Poll GitHub for pull requests where you are requested as a reviewer
or are assigned, and send notifications for PRs that need your attention.`,
	RunE: runRun,
}

func init() {
	runCmd.Flags().BoolVar(&includeDrafts, "include-drafts", false, "Include draft PRs in results")
	rootCmd.AddCommand(runCmd)
}

func runRun(cmd *cobra.Command, args []string) error {
	cfg, err := config.Load(GetConfigPath(), GetOrgs())
	if err != nil {
		return fmt.Errorf("loading config: %w", err)
	}

	database, err := db.Open(cfg.DBPath)
	if err != nil {
		return fmt.Errorf("opening database: %w", err)
	}
	defer database.Close()

	ghClient := github.NewClient(&github.RealExecutor{}, cfg.GHHost)

	opts := &poller.PollOptions{
		IncludeDrafts: includeDrafts,
	}

	result, err := poller.Poll(database, cfg, ghClient, opts)
	if err != nil {
		return fmt.Errorf("polling: %w", err)
	}

	if result.Skipped {
		if !GetQuiet() {
			fmt.Fprintf(os.Stderr, "Skipped: %s\n", result.SkipReason)
		}
		return nil
	}

	if !GetQuiet() {
		fmt.Printf("Found %d PRs (%d notifications sent)\n", result.TotalPRs, result.NotificationsSent)
	}

	return nil
}
