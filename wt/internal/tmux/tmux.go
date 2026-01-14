package tmux

import (
	"bytes"
	"errors"
	"fmt"
	"os"
	"os/exec"
	"strings"
)

var (
	ErrSessionExists   = errors.New("tmux session already exists")
	ErrSessionNotFound = errors.New("tmux session not found")
	ErrWindowExists    = errors.New("tmux window already exists")
	ErrWindowNotFound  = errors.New("tmux window not found")
	ErrTmuxNotFound    = errors.New("tmux not installed or not in PATH")
	ErrNotInTmux       = errors.New("not inside a tmux session")
	ErrInvalidName     = errors.New("invalid session name")
)

type Session struct {
	Name      string
	Attached  bool
	Windows   int
	CreatedAt string
}

func CheckTmux() error {
	_, err := exec.LookPath("tmux")
	if err != nil {
		return ErrTmuxNotFound
	}
	return nil
}

func ListSessions() ([]Session, error) {
	if err := CheckTmux(); err != nil {
		return nil, err
	}

	cmd := exec.Command("tmux", "list-sessions", "-F", "#{session_name}\t#{session_attached}\t#{session_windows}")
	var stderr bytes.Buffer
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	if err != nil {
		stderrStr := stderr.String()
		// Handle expected "no sessions" conditions
		if strings.Contains(stderrStr, "no server running") ||
			strings.Contains(stderrStr, "no sessions") ||
			strings.Contains(stderrStr, "error connecting to") {
			return []Session{}, nil
		}
		if stderrStr != "" {
			return nil, fmt.Errorf("failed to list sessions: %s", strings.TrimSpace(stderrStr))
		}
		return nil, fmt.Errorf("failed to list sessions: %w", err)
	}

	var sessions []Session
	lines := strings.Split(strings.TrimSpace(string(out)), "\n")
	for _, line := range lines {
		if line == "" {
			continue
		}
		parts := strings.Split(line, "\t")
		if len(parts) >= 3 {
			sessions = append(sessions, Session{
				Name:     parts[0],
				Attached: parts[1] == "1",
			})
		}
	}

	return sessions, nil
}

func SessionExists(name string) bool {
	cmd := exec.Command("tmux", "has-session", "-t", name)
	return cmd.Run() == nil
}

func CreateSession(name, workDir string) error {
	if name == "" {
		return ErrInvalidName
	}

	if err := CheckTmux(); err != nil {
		return err
	}

	if SessionExists(name) {
		return ErrSessionExists
	}

	cmd := exec.Command("tmux", "new-session", "-d", "-s", name, "-c", workDir)
	if out, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("failed to create session: %s", strings.TrimSpace(string(out)))
	}

	return nil
}

func KillSession(name string) error {
	if err := CheckTmux(); err != nil {
		return err
	}

	if !SessionExists(name) {
		return ErrSessionNotFound
	}

	cmd := exec.Command("tmux", "kill-session", "-t", name)
	if out, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("failed to kill session: %s", strings.TrimSpace(string(out)))
	}

	return nil
}

func AttachSession(name string) error {
	if err := CheckTmux(); err != nil {
		return err
	}

	if !SessionExists(name) {
		return ErrSessionNotFound
	}

	cmd := exec.Command("tmux", "attach-session", "-t", name)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr

	return cmd.Run()
}

func SwitchToSession(name string) error {
	if err := CheckTmux(); err != nil {
		return err
	}

	if !SessionExists(name) {
		return ErrSessionNotFound
	}

	if os.Getenv("TMUX") != "" {
		cmd := exec.Command("tmux", "switch-client", "-t", name)
		return cmd.Run()
	}

	return AttachSession(name)
}

func IsInsideTmux() bool {
	return os.Getenv("TMUX") != ""
}

func GetCurrentSession() (string, error) {
	if !IsInsideTmux() {
		return "", ErrNotInTmux
	}

	cmd := exec.Command("tmux", "display-message", "-p", "#{session_name}")
	out, err := cmd.Output()
	if err != nil {
		return "", fmt.Errorf("failed to get current session: %w", err)
	}

	return strings.TrimSpace(string(out)), nil
}

func WindowExists(session, window string) bool {
	target := fmt.Sprintf("%s:%s", session, window)
	cmd := exec.Command("tmux", "has-session", "-t", target)
	return cmd.Run() == nil
}

func CreateWindow(session, name, workDir string) error {
	if name == "" {
		return ErrInvalidName
	}

	if err := CheckTmux(); err != nil {
		return err
	}

	if WindowExists(session, name) {
		return ErrWindowExists
	}

	cmd := exec.Command("tmux", "new-window", "-t", session, "-n", name, "-c", workDir)
	if out, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("failed to create window: %s", strings.TrimSpace(string(out)))
	}

	return nil
}

func KillWindow(session, name string) error {
	if err := CheckTmux(); err != nil {
		return err
	}

	target := fmt.Sprintf("%s:%s", session, name)
	if !WindowExists(session, name) {
		return ErrWindowNotFound
	}

	cmd := exec.Command("tmux", "kill-window", "-t", target)
	if out, err := cmd.CombinedOutput(); err != nil {
		return fmt.Errorf("failed to kill window: %s", strings.TrimSpace(string(out)))
	}

	return nil
}

func SwitchToWindow(session, name string) error {
	if err := CheckTmux(); err != nil {
		return err
	}

	target := fmt.Sprintf("%s:%s", session, name)

	if IsInsideTmux() {
		cmd := exec.Command("tmux", "select-window", "-t", target)
		return cmd.Run()
	}

	cmd := exec.Command("tmux", "attach-session", "-t", target)
	cmd.Stdin = os.Stdin
	cmd.Stdout = os.Stdout
	cmd.Stderr = os.Stderr
	return cmd.Run()
}
