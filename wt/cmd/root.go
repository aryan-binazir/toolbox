package cmd

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
)

var (
	worktreeDir string
)

var rootCmd = &cobra.Command{
	Use:   "wt",
	Short: "Manage tmux sessions with git worktrees",
	Long: `wt is a CLI tool that creates and manages tmux sessions
paired with git worktrees. It allows you to quickly spin up
isolated development environments for different branches.`,
}

func Execute() {
	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func init() {
	rootCmd.PersistentFlags().StringVarP(&worktreeDir, "dir", "d", "", "Directory for worktrees (default: sibling to current repo)")
}
