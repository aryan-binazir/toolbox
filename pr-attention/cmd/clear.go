package cmd

import (
	"bufio"
	"fmt"
	"os"
	"strings"

	"github.com/spf13/cobra"

	"pr-attention/internal/config"
)

var forceFlag bool

var clearCmd = &cobra.Command{
	Use:   "clear",
	Short: "Delete the database and start fresh",
	Long:  `Delete the pr-attention database file to reset all state.`,
	RunE:  runClear,
}

func init() {
	clearCmd.Flags().BoolVarP(&forceFlag, "force", "f", false, "Skip confirmation prompt")
	rootCmd.AddCommand(clearCmd)
}

func runClear(cmd *cobra.Command, args []string) error {
	cfg, err := config.Load(GetConfigPath(), GetOrgs())
	if err != nil {
		return fmt.Errorf("loading config: %w", err)
	}

	dbPath := cfg.DBPath

	// Check if database exists
	if _, err := os.Stat(dbPath); os.IsNotExist(err) {
		fmt.Println("Database does not exist. Nothing to clear.")
		return nil
	}

	// Confirm unless --force
	if !forceFlag {
		fmt.Printf("This will delete: %s\n", dbPath)
		fmt.Print("Are you sure? [y/N] ")

		reader := bufio.NewReader(os.Stdin)
		response, err := reader.ReadString('\n')
		if err != nil {
			return fmt.Errorf("reading response: %w", err)
		}

		response = strings.TrimSpace(strings.ToLower(response))
		if response != "y" && response != "yes" {
			fmt.Println("Aborted.")
			return nil
		}
	}

	if err := os.Remove(dbPath); err != nil {
		return fmt.Errorf("deleting database: %w", err)
	}

	fmt.Printf("Deleted: %s\n", dbPath)
	return nil
}
