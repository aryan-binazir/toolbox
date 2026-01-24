package cmd

import (
	"fmt"
	"os"

	"github.com/spf13/cobra"
)

var (
	orgs       []string
	configPath string
	quiet      bool
)

var rootCmd = &cobra.Command{
	Use:   "pr-attention",
	Short: "Monitor PRs that need your attention",
	Long: `pr-attention monitors GitHub pull requests that require your attention
and sends desktop notifications when action is needed.

It tracks PRs where you are requested as a reviewer or where your
review has been addressed with new changes.`,
}

func Execute() {
	if err := rootCmd.Execute(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}

func init() {
	rootCmd.PersistentFlags().StringSliceVar(&orgs, "org", nil, "GitHub organization(s) to monitor (repeatable)")
	rootCmd.PersistentFlags().StringVar(&configPath, "config", "", "Config file path (default ~/.config/pr-attention/config.toml)")
	rootCmd.PersistentFlags().BoolVarP(&quiet, "quiet", "q", false, "Suppress non-error output")
}

func GetOrgs() []string {
	return orgs
}

func GetConfigPath() string {
	return configPath
}

func GetQuiet() bool {
	return quiet
}
