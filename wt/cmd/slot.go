package cmd

import (
	"fmt"
	"os"
	"path/filepath"

	"wt/internal/git"

	"github.com/spf13/cobra"
)

var (
	slotBaseBranch string
	slotNames      = []string{"alpha", "beta", "gamma", "delta"}
)

var slotCmd = &cobra.Command{
	Use:   "slot",
	Short: "Create the first missing named worktree slot (alpha..delta)",
	Long: `Create one worktree slot from main-branch context.

The command checks slots in order: alpha, beta, gamma, delta.
It creates the first missing slot (max 4 total), branching from the base branch
and symlinking <slot>/context to <base-worktree>/context.`,
	RunE: runSlotCreate,
}

func init() {
	slotCmd.Flags().StringVar(&slotBaseBranch, "base", "main", "Base branch used to create slot branches")
	rootCmd.AddCommand(slotCmd)
}

func runSlotCreate(cmd *cobra.Command, args []string) error {
	if slotBaseBranch == "" {
		return fmt.Errorf("base branch cannot be empty")
	}

	basePath, err := getWorktreeBasePath()
	if err != nil {
		return err
	}

	worktrees, err := git.ListWorktrees()
	if err != nil {
		return fmt.Errorf("failed to list worktrees: %w", err)
	}

	existing := map[string]bool{}
	for _, wt := range worktrees {
		existing[filepath.Base(wt.Path)] = true
	}

	slot := firstMissingSlot(existing)
	if slot == "" {
		return fmt.Errorf("all slots already exist: %v", slotNames)
	}

	baseRef, err := resolveBaseRef(slotBaseBranch)
	if err != nil {
		return err
	}

	baseWorktreePath, err := findBranchWorktreePath(worktrees, slotBaseBranch)
	if err != nil {
		return err
	}

	contextSource := filepath.Join(baseWorktreePath, "context")
	if st, statErr := os.Stat(contextSource); statErr != nil || !st.IsDir() {
		return fmt.Errorf("context directory not found at base worktree: %s", contextSource)
	}

	fmt.Printf("Creating slot '%s' from '%s'...\n", slot, baseRef)
	worktreePath, err := git.CreateWorktreeFromBase(slot, basePath, slot, baseRef)
	if err != nil {
		return fmt.Errorf("failed to create slot '%s': %w", slot, err)
	}

	contextTarget := filepath.Join(worktreePath, "context")
	if err := ensureContextSymlink(contextSource, contextTarget); err != nil {
		_ = git.RemoveWorktree(worktreePath, true)
		return err
	}

	fmt.Printf("Created slot: %s\n", slot)
	fmt.Printf("Worktree: %s\n", worktreePath)
	fmt.Printf("Context: %s -> %s\n", contextTarget, contextSource)
	return nil
}

func firstMissingSlot(existing map[string]bool) string {
	for _, slot := range slotNames {
		if !existing[slot] {
			return slot
		}
	}
	return ""
}

func resolveBaseRef(baseBranch string) (string, error) {
	if git.RefExists("refs/heads/" + baseBranch) {
		return baseBranch, nil
	}
	remote := "origin/" + baseBranch
	if git.RefExists("refs/remotes/" + remote) {
		return remote, nil
	}
	return "", fmt.Errorf("base branch not found locally or on origin: %s", baseBranch)
}

func findBranchWorktreePath(worktrees []git.Worktree, branch string) (string, error) {
	for _, wt := range worktrees {
		if wt.Branch == branch {
			return wt.Path, nil
		}
	}
	return "", fmt.Errorf("could not find a checked-out worktree for branch '%s'", branch)
}

func ensureContextSymlink(contextSource, contextTarget string) error {
	if fi, err := os.Lstat(contextTarget); err == nil {
		if fi.Mode()&os.ModeSymlink != 0 {
			target, readErr := os.Readlink(contextTarget)
			if readErr != nil {
				return fmt.Errorf("failed to read existing context symlink: %w", readErr)
			}
			if target == contextSource {
				return nil
			}
			return fmt.Errorf("context symlink already exists with different target: %s", target)
		}
		return fmt.Errorf("context path already exists and is not a symlink: %s", contextTarget)
	} else if !os.IsNotExist(err) {
		return fmt.Errorf("failed to inspect context path '%s': %w", contextTarget, err)
	}

	if err := os.Symlink(contextSource, contextTarget); err != nil {
		return fmt.Errorf("failed to create context symlink: %w", err)
	}
	return nil
}
