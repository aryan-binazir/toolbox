package cmd

import (
	"fmt"
	"os"
	"text/tabwriter"

	"wt/internal/git"
	"wt/internal/tmux"

	"github.com/spf13/cobra"
)

var listCmd = &cobra.Command{
	Use:     "list",
	Aliases: []string{"ls"},
	Short:   "List worktrees and their tmux sessions",
	Long: `List all git worktrees and show which ones have associated tmux sessions.

Examples:
  wt list
  wt ls`,
	RunE: runList,
}

func init() {
	rootCmd.AddCommand(listCmd)
}

type worktreeStatus struct {
	Name       string
	Path       string
	Branch     string
	HasSession bool
	Attached   bool
}

func runList(cmd *cobra.Command, args []string) error {
	worktrees, err := git.ListWorktrees()
	if err != nil {
		return fmt.Errorf("failed to list worktrees: %w", err)
	}

	sessions, err := tmux.ListSessions()
	if err != nil {
		return fmt.Errorf("failed to list tmux sessions: %w", err)
	}

	sessionMap := make(map[string]tmux.Session)
	for _, s := range sessions {
		sessionMap[s.Name] = s
	}

	if len(worktrees) == 0 {
		fmt.Println("No worktrees found.")
		return nil
	}

	w := tabwriter.NewWriter(os.Stdout, 0, 0, 2, ' ', 0)
	fmt.Fprintln(w, "NAME\tBRANCH\tSESSION\tPATH")
	fmt.Fprintln(w, "----\t------\t-------\t----")

	for _, wt := range worktrees {
		sessionStatus := "-"
		if session, exists := sessionMap[wt.Name]; exists {
			if session.Attached {
				sessionStatus = "attached"
			} else {
				sessionStatus = "active"
			}
		}

		branchDisplay := wt.Branch
		if branchDisplay == "" {
			branchDisplay = "(detached)"
		}

		fmt.Fprintf(w, "%s\t%s\t%s\t%s\n", wt.Name, branchDisplay, sessionStatus, wt.Path)
	}

	w.Flush()
	return nil
}
