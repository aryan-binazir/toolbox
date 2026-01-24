package cmd

import (
	"fmt"
	"os"
	"strings"
	"text/tabwriter"
	"time"

	"github.com/spf13/cobra"

	"pr-attention/internal/config"
	"pr-attention/internal/db"
)

var statusCmd = &cobra.Command{
	Use:   "status",
	Short: "Show current attention queue and last run info",
	Long:  `Display the current list of PRs needing attention and information about the last poll run.`,
	RunE:  runStatus,
}

func init() {
	rootCmd.AddCommand(statusCmd)
}

func runStatus(cmd *cobra.Command, args []string) error {
	cfg, err := config.Load(GetConfigPath(), GetOrgs())
	if err != nil {
		return fmt.Errorf("loading config: %w", err)
	}

	database, err := db.Open(cfg.DBPath)
	if err != nil {
		return fmt.Errorf("opening database: %w", err)
	}
	defer database.Close()

	// Show last run info
	lastRun, err := database.GetLastRun()
	if err != nil {
		return fmt.Errorf("getting last run: %w", err)
	}

	fmt.Println("=== Last Run ===")
	if lastRun == nil {
		fmt.Println("No runs recorded yet.")
	} else {
		ago := time.Since(lastRun.RunAt).Round(time.Second)
		fmt.Printf("Time:          %s (%s ago)\n", lastRun.RunAt.Format(time.RFC3339), ago)
		fmt.Printf("PRs found:     %d\n", lastRun.PRsFound)
		fmt.Printf("Notifications: %d\n", lastRun.NotificationsSent)
		if lastRun.DurationMs.Valid {
			fmt.Printf("Duration:      %dms\n", lastRun.DurationMs.Int64)
		}
		if lastRun.ErrorMessage.Valid && lastRun.ErrorMessage.String != "" {
			fmt.Printf("Error:         %s\n", lastRun.ErrorMessage.String)
		}
	}

	fmt.Println()

	// Show attention queue
	activePRs, err := database.GetActivePRs()
	if err != nil {
		return fmt.Errorf("getting active PRs: %w", err)
	}

	fmt.Println("=== Attention Queue ===")
	if len(activePRs) == 0 {
		fmt.Println("No PRs in attention queue.")
		return nil
	}

	w := tabwriter.NewWriter(os.Stdout, 0, 0, 2, ' ', 0)
	fmt.Fprintln(w, "REPO\t#\tTITLE\tSTATUS\tSINCE")

	for _, pr := range activePRs {
		title := truncate(pr.Title, 40)
		since := formatDuration(time.Since(pr.FirstSeen))

		fmt.Fprintf(w, "%s\t%d\t%s\t%s\t%s\n",
			pr.Repo, pr.Number, title, pr.CurrentStatus, since)
	}

	w.Flush()

	return nil
}

func truncate(s string, maxLen int) string {
	if len(s) <= maxLen {
		return s
	}
	return s[:maxLen-3] + "..."
}

func formatDuration(d time.Duration) string {
	switch hours := int(d.Hours()); {
	case hours >= 24:
		return fmt.Sprintf("%dd", hours/24)
	case hours >= 1:
		return fmt.Sprintf("%dh", hours)
	default:
		return fmt.Sprintf("%dm", int(d.Minutes()))
	}
}

func parseRef(ref string) (repo string, number int, prID string, err error) {
	// Handle URL format: https://github.com/org/repo/pull/123
	if strings.Contains(ref, "github.com") || strings.Contains(ref, "/pull/") {
		parts := strings.Split(ref, "/")
		for i, part := range parts {
			if part == "pull" && i+1 < len(parts) && i >= 2 {
				repo = parts[i-2] + "/" + parts[i-1]
				fmt.Sscanf(parts[i+1], "%d", &number)
				return repo, number, "", nil
			}
		}
		return "", 0, "", fmt.Errorf("invalid URL format: %s", ref)
	}

	// Handle org/repo#N format
	if strings.Contains(ref, "#") {
		parts := strings.SplitN(ref, "#", 2)
		repo = parts[0]
		fmt.Sscanf(parts[1], "%d", &number)
		return repo, number, "", nil
	}

	// Assume it's a PR ID
	return "", 0, ref, nil
}
