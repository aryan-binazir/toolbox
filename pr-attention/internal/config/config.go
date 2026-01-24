package config

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/BurntSushi/toml"
)

type Config struct {
	Orgs          []string `toml:"orgs"`
	GHHost        string   `toml:"gh_host"`
	ExcludeDrafts bool     `toml:"exclude_drafts"`
	IgnoreLabels  []string `toml:"ignore_labels"`
	IgnoreAuthors []string `toml:"ignore_authors"`
	SoundEnabled  bool     `toml:"sound_enabled"`
	DBPath        string   `toml:"db_path"`
	ConfigPath    string   `toml:"-"`
}

func defaultConfigPath() string {
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	return filepath.Join(home, ".config", "pr-attention", "config.toml")
}

func defaultDBPath() string {
	home, err := os.UserHomeDir()
	if err != nil {
		return ""
	}
	return filepath.Join(home, ".local", "share", "pr-attention", "state.db")
}

func Load(configPath string, cliOrgs []string) (*Config, error) {
	cfg := &Config{
		ExcludeDrafts: true,
		SoundEnabled:  true,
		DBPath:        defaultDBPath(),
	}

	if configPath == "" {
		configPath = defaultConfigPath()
	}
	cfg.ConfigPath = configPath

	if _, err := os.Stat(configPath); err == nil {
		if _, err := toml.DecodeFile(configPath, cfg); err != nil {
			return nil, fmt.Errorf("parsing config file: %w", err)
		}
	}

	if envOrgs := os.Getenv("PR_ATTENTION_ORGS"); envOrgs != "" {
		cfg.Orgs = splitAndTrim(envOrgs, ",")
	}

	if envHost := os.Getenv("GH_HOST"); envHost != "" {
		cfg.GHHost = envHost
	}

	if len(cliOrgs) > 0 {
		cfg.Orgs = cliOrgs
	}

	if cfg.DBPath == "" {
		cfg.DBPath = defaultDBPath()
	}

	return cfg, nil
}

func splitAndTrim(s, sep string) []string {
	parts := strings.Split(s, sep)
	result := make([]string, 0, len(parts))
	for _, p := range parts {
		p = strings.TrimSpace(p)
		if p != "" {
			result = append(result, p)
		}
	}
	return result
}
