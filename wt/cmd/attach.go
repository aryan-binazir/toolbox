package cmd

import (
	"fmt"

	"wt/internal/git"
	"wt/internal/tmux"

	"github.com/spf13/cobra"
)

var attachCmd = &cobra.Command{
	Use:     "attach <name>",
	Aliases: []string{"a"},
	Short:   "Attach to an existing tmux session",
	Long: `Attach to an existing tmux session for a worktree.
If the session doesn't exist but the worktree does, a new session will be created.

Examples:
  wt attach feature-login
  wt a bugfix-123`,
	Args: cobra.ExactArgs(1),
	RunE: runAttach,
}

func init() {
	rootCmd.AddCommand(attachCmd)
}

func runAttach(cmd *cobra.Command, args []string) error {
	name := args[0]

	if err := tmux.CheckTmux(); err != nil {
		return fmt.Errorf("tmux is required: %w", err)
	}

	if tmux.SessionExists(name) {
		return tmux.SwitchToSession(name)
	}

	basePath, err := getWorktreeBasePath()
	if err != nil {
		return err
	}

	worktreePath, err := git.GetWorktreePath(name, basePath)
	if err != nil {
		return fmt.Errorf("'%s' not found (no session or worktree with that name)", name)
	}

	fmt.Printf("Session not found. Creating new session for worktree '%s'...\n", name)
	if err := tmux.CreateSession(name, worktreePath); err != nil {
		return fmt.Errorf("failed to create session: %w", err)
	}

	return tmux.SwitchToSession(name)
}
