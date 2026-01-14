package cmd

import (
	"bufio"
	"fmt"
	"os"
	"strings"

	"wt/internal/git"
	"wt/internal/tmux"

	"github.com/spf13/cobra"
)

var (
	force        bool
	skipConfirm  bool
	deleteWindow bool
)

var deleteCmd = &cobra.Command{
	Use:     "delete <name>",
	Aliases: []string{"rm", "remove"},
	Short:   "Delete a worktree and its tmux session",
	Long: `Delete a git worktree and its associated tmux session.

By default, this will prompt for confirmation. Use -y to skip.
Use -f to force deletion even if worktree has uncommitted changes.

Examples:
  wt delete feature-login
  wt rm bugfix-123 -y
  wt delete experiment -f
  wt delete quick-fix --window    # Delete window instead of session`,
	Args: cobra.ExactArgs(1),
	RunE: runDelete,
}

func init() {
	deleteCmd.Flags().BoolVarP(&force, "force", "f", false, "Force deletion even with uncommitted changes")
	deleteCmd.Flags().BoolVarP(&skipConfirm, "yes", "y", false, "Skip confirmation prompt")
	deleteCmd.Flags().BoolVarP(&deleteWindow, "window", "w", false, "Delete window in current session instead of session")
	rootCmd.AddCommand(deleteCmd)
}

func runDelete(cmd *cobra.Command, args []string) error {
	name := args[0]

	basePath, err := getWorktreeBasePath()
	if err != nil {
		return err
	}

	if deleteWindow {
		return runDeleteWindow(name, basePath)
	}

	return runDeleteSession(name, basePath)
}

func runDeleteSession(name, basePath string) error {
	worktreePath, wtErr := git.GetWorktreePath(name, basePath)
	sessionExists := tmux.SessionExists(name)

	if wtErr != nil && !sessionExists {
		return fmt.Errorf("'%s' not found (no worktree or session with that name)", name)
	}

	if !skipConfirm {
		what := []string{}
		if wtErr == nil {
			what = append(what, fmt.Sprintf("worktree at %s", worktreePath))
		}
		if sessionExists {
			what = append(what, "tmux session")
		}

		fmt.Printf("This will delete: %s\n", strings.Join(what, " and "))
		fmt.Print("Are you sure? [y/N]: ")

		reader := bufio.NewReader(os.Stdin)
		response, _ := reader.ReadString('\n')
		response = strings.TrimSpace(strings.ToLower(response))

		if response != "y" && response != "yes" {
			fmt.Println("Cancelled.")
			return nil
		}
	}

	if wtErr == nil {
		fmt.Printf("Removing worktree '%s'...\n", name)
		if err := git.RemoveWorktree(worktreePath, force); err != nil {
			return fmt.Errorf("failed to remove worktree: %w", err)
		}
	}

	if sessionExists {
		fmt.Printf("Killing tmux session '%s'...\n", name)
		if err := tmux.KillSession(name); err != nil {
			return fmt.Errorf("failed to kill tmux session: %w", err)
		}
	}

	fmt.Printf("Deleted '%s'\n", name)
	return nil
}

func runDeleteWindow(name, basePath string) error {
	if !tmux.IsInsideTmux() {
		return fmt.Errorf("--window requires being inside a tmux session")
	}

	session, err := tmux.GetCurrentSession()
	if err != nil {
		return fmt.Errorf("failed to get current session: %w", err)
	}

	worktreePath, wtErr := git.GetWorktreePath(name, basePath)
	windowExists := tmux.WindowExists(session, name)

	if wtErr != nil && !windowExists {
		return fmt.Errorf("'%s' not found (no worktree or window with that name)", name)
	}

	if !skipConfirm {
		what := []string{}
		if wtErr == nil {
			what = append(what, fmt.Sprintf("worktree at %s", worktreePath))
		}
		if windowExists {
			what = append(what, fmt.Sprintf("tmux window '%s:%s'", session, name))
		}

		fmt.Printf("This will delete: %s\n", strings.Join(what, " and "))
		fmt.Print("Are you sure? [y/N]: ")

		reader := bufio.NewReader(os.Stdin)
		response, _ := reader.ReadString('\n')
		response = strings.TrimSpace(strings.ToLower(response))

		if response != "y" && response != "yes" {
			fmt.Println("Cancelled.")
			return nil
		}
	}

	if wtErr == nil {
		fmt.Printf("Removing worktree '%s'...\n", name)
		if err := git.RemoveWorktree(worktreePath, force); err != nil {
			return fmt.Errorf("failed to remove worktree: %w", err)
		}
	}

	if windowExists {
		fmt.Printf("Killing tmux window '%s:%s'...\n", session, name)
		if err := tmux.KillWindow(session, name); err != nil {
			return fmt.Errorf("failed to kill tmux window: %w", err)
		}
	}

	fmt.Printf("Deleted '%s'\n", name)
	return nil
}
