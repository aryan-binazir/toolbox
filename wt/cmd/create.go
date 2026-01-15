package cmd

import (
	"fmt"
	"os"
	"path/filepath"
	"regexp"

	"wt/internal/git"
	"wt/internal/tmux"

	"github.com/spf13/cobra"
)

var (
	branch    string
	noAttach  bool
	asWindow  bool
	nameRegex = regexp.MustCompile(`^[a-zA-Z0-9][a-zA-Z0-9._-]*$`)
)

var createCmd = &cobra.Command{
	Use:   "create <name>",
	Short: "Create a new worktree and tmux session",
	Long: `Create a new git worktree and an associated tmux session.
The session will start in the worktree directory.

Examples:
  wt create feature-login
  wt create bugfix-123 -b fix/issue-123
  wt create experiment --no-attach
  wt create quick-fix --window      # Create as window in current session`,
	Args: cobra.ExactArgs(1),
	RunE: runCreate,
}

func init() {
	createCmd.Flags().StringVarP(&branch, "branch", "b", "", "Branch name (defaults to worktree name)")
	createCmd.Flags().BoolVar(&noAttach, "no-attach", false, "Don't attach to the session after creation")
	createCmd.Flags().BoolVarP(&asWindow, "window", "w", false, "Create as window in current tmux session instead of new session")
	rootCmd.AddCommand(createCmd)
}

func runCreate(cmd *cobra.Command, args []string) error {
	name := args[0]

	if err := validateName(name); err != nil {
		return err
	}

	if err := tmux.CheckTmux(); err != nil {
		return fmt.Errorf("tmux is required: %w", err)
	}

	basePath, err := getWorktreeBasePath()
	if err != nil {
		return err
	}

	exists, err := git.WorktreeExists(name, basePath)
	if err != nil {
		return fmt.Errorf("failed to check worktree: %w", err)
	}
	if exists {
		return fmt.Errorf("worktree '%s' already exists", name)
	}

	if asWindow {
		return createAsWindow(name, basePath)
	}

	return createAsSession(name, basePath)
}

func createAsSession(name, basePath string) error {
	if tmux.SessionExists(name) {
		return fmt.Errorf("tmux session '%s' already exists", name)
	}

	fmt.Printf("Creating worktree '%s'...\n", name)
	worktreePath, err := git.CreateWorktree(name, basePath, branch)
	if err != nil {
		return fmt.Errorf("failed to create worktree: %w", err)
	}

	fmt.Printf("Creating tmux session '%s'...\n", name)
	if err := tmux.CreateSession(name, worktreePath); err != nil {
		if cleanupErr := git.RemoveWorktree(worktreePath, true); cleanupErr != nil {
			fmt.Fprintf(os.Stderr, "Warning: failed to clean up worktree at %s: %v\n", worktreePath, cleanupErr)
		}
		return fmt.Errorf("failed to create tmux session: %w", err)
	}

	fmt.Printf("Created worktree at: %s\n", worktreePath)
	fmt.Printf("Created tmux session: %s\n", name)

	if !noAttach {
		fmt.Println("Attaching to session...")
		return tmux.SwitchToSession(name)
	}

	return nil
}

func createAsWindow(name, basePath string) error {
	if !tmux.IsInsideTmux() {
		return fmt.Errorf("--window requires being inside a tmux session")
	}

	session, err := tmux.GetCurrentSession()
	if err != nil {
		return fmt.Errorf("failed to get current session: %w", err)
	}

	if tmux.WindowExists(session, name) {
		return fmt.Errorf("window '%s' already exists in session '%s'", name, session)
	}

	fmt.Printf("Creating worktree '%s'...\n", name)
	worktreePath, err := git.CreateWorktree(name, basePath, branch)
	if err != nil {
		return fmt.Errorf("failed to create worktree: %w", err)
	}

	fmt.Printf("Creating tmux window '%s' in session '%s'...\n", name, session)
	if err := tmux.CreateWindow(session, name, worktreePath); err != nil {
		if cleanupErr := git.RemoveWorktree(worktreePath, true); cleanupErr != nil {
			fmt.Fprintf(os.Stderr, "Warning: failed to clean up worktree at %s: %v\n", worktreePath, cleanupErr)
		}
		return fmt.Errorf("failed to create tmux window: %w", err)
	}

	fmt.Printf("Created worktree at: %s\n", worktreePath)
	fmt.Printf("Created tmux window: %s:%s\n", session, name)

	if !noAttach {
		fmt.Println("Switching to window...")
		return tmux.SwitchToWindow(session, name)
	}

	return nil
}

func validateName(name string) error {
	if name == "" {
		return fmt.Errorf("name cannot be empty")
	}

	if len(name) > 100 {
		return fmt.Errorf("name too long (max 100 characters)")
	}

	if !nameRegex.MatchString(name) {
		return fmt.Errorf("invalid name '%s': must start with alphanumeric and contain only alphanumeric, dots, underscores, or hyphens", name)
	}

	reserved := []string{".", "..", "new", "list", "delete", "attach"}
	for _, r := range reserved {
		if name == r {
			return fmt.Errorf("'%s' is a reserved name", name)
		}
	}

	return nil
}

func getWorktreeBasePath() (string, error) {
	if worktreeDir != "" {
		absPath, err := filepath.Abs(worktreeDir)
		if err != nil {
			return "", fmt.Errorf("invalid directory path: %w", err)
		}
		return absPath, nil
	}

	repoRoot, err := git.GetWorktreeRoot()
	if err != nil {
		return "", fmt.Errorf("not in a git repository: %w", err)
	}

	parentDir := filepath.Dir(repoRoot)
	if _, err := os.Stat(parentDir); err != nil {
		return "", fmt.Errorf("parent directory does not exist: %w", err)
	}

	return parentDir, nil
}
