package tmux

import (
	"os"
	"os/exec"
	"testing"
)

func hasTmux() bool {
	_, err := exec.LookPath("tmux")
	return err == nil
}

func TestCheckTmux(t *testing.T) {
	err := CheckTmux()
	if hasTmux() {
		if err != nil {
			t.Errorf("CheckTmux() error = %v, want nil (tmux is installed)", err)
		}
	} else {
		if err != ErrTmuxNotFound {
			t.Errorf("CheckTmux() error = %v, want %v", err, ErrTmuxNotFound)
		}
	}
}

func TestListSessions(t *testing.T) {
	if !hasTmux() {
		t.Skip("tmux not installed")
	}

	sessions, err := ListSessions()
	if err != nil {
		t.Fatalf("ListSessions() error = %v", err)
	}

	if sessions == nil {
		t.Error("ListSessions() returned nil, want empty slice")
	}
}

func TestSessionExists_NotExists(t *testing.T) {
	if !hasTmux() {
		t.Skip("tmux not installed")
	}

	exists := SessionExists("wt-test-nonexistent-session-12345")
	if exists {
		t.Error("SessionExists() = true for nonexistent session")
	}
}

func TestCreateSession_InvalidName(t *testing.T) {
	if !hasTmux() {
		t.Skip("tmux not installed")
	}

	err := CreateSession("", "/tmp")
	if err != ErrInvalidName {
		t.Errorf("CreateSession() error = %v, want %v", err, ErrInvalidName)
	}
}

func TestCreateAndKillSession(t *testing.T) {
	if !hasTmux() {
		t.Skip("tmux not installed")
	}

	sessionName := "wt-test-session-crud"

	if SessionExists(sessionName) {
		KillSession(sessionName)
	}

	tmpDir, err := os.MkdirTemp("", "wt-tmux-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tmpDir)

	err = CreateSession(sessionName, tmpDir)
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}

	if !SessionExists(sessionName) {
		t.Error("session was not created")
	}

	sessions, _ := ListSessions()
	found := false
	for _, s := range sessions {
		if s.Name == sessionName {
			found = true
			break
		}
	}
	if !found {
		t.Error("created session not found in ListSessions()")
	}

	err = KillSession(sessionName)
	if err != nil {
		t.Fatalf("KillSession() error = %v", err)
	}

	if SessionExists(sessionName) {
		t.Error("session still exists after kill")
	}
}

func TestCreateSession_AlreadyExists(t *testing.T) {
	if !hasTmux() {
		t.Skip("tmux not installed")
	}

	sessionName := "wt-test-duplicate"

	if SessionExists(sessionName) {
		KillSession(sessionName)
	}

	tmpDir, err := os.MkdirTemp("", "wt-tmux-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tmpDir)

	err = CreateSession(sessionName, tmpDir)
	if err != nil {
		t.Fatalf("first CreateSession() error = %v", err)
	}
	defer KillSession(sessionName)

	err = CreateSession(sessionName, tmpDir)
	if err != ErrSessionExists {
		t.Errorf("CreateSession() error = %v, want %v", err, ErrSessionExists)
	}
}

func TestKillSession_NotExists(t *testing.T) {
	if !hasTmux() {
		t.Skip("tmux not installed")
	}

	err := KillSession("wt-test-nonexistent-kill-12345")
	if err != ErrSessionNotFound {
		t.Errorf("KillSession() error = %v, want %v", err, ErrSessionNotFound)
	}
}

func TestIsInsideTmux(t *testing.T) {
	result := IsInsideTmux()
	hasTmuxEnv := os.Getenv("TMUX") != ""
	if result != hasTmuxEnv {
		t.Errorf("IsInsideTmux() = %v, want %v", result, hasTmuxEnv)
	}
}

func TestWindowExists_NotExists(t *testing.T) {
	if !hasTmux() {
		t.Skip("tmux not installed")
	}

	exists := WindowExists("nonexistent-session", "nonexistent-window")
	if exists {
		t.Error("WindowExists() = true for nonexistent window")
	}
}

func TestCreateWindow_InvalidName(t *testing.T) {
	if !hasTmux() {
		t.Skip("tmux not installed")
	}

	err := CreateWindow("test-session", "", "/tmp")
	if err != ErrInvalidName {
		t.Errorf("CreateWindow() error = %v, want %v", err, ErrInvalidName)
	}
}

func TestCreateAndKillWindow(t *testing.T) {
	if !hasTmux() {
		t.Skip("tmux not installed")
	}

	sessionName := "wt-test-window-session"
	windowName := "wt-test-window"

	if SessionExists(sessionName) {
		KillSession(sessionName)
	}

	tmpDir, err := os.MkdirTemp("", "wt-tmux-window-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tmpDir)

	err = CreateSession(sessionName, tmpDir)
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}
	defer KillSession(sessionName)

	err = CreateWindow(sessionName, windowName, tmpDir)
	if err != nil {
		t.Fatalf("CreateWindow() error = %v", err)
	}

	if !WindowExists(sessionName, windowName) {
		t.Error("window was not created")
	}

	err = KillWindow(sessionName, windowName)
	if err != nil {
		t.Fatalf("KillWindow() error = %v", err)
	}

	if WindowExists(sessionName, windowName) {
		t.Error("window still exists after kill")
	}
}

func TestCreateWindow_AlreadyExists(t *testing.T) {
	if !hasTmux() {
		t.Skip("tmux not installed")
	}

	sessionName := "wt-test-dup-window-session"
	windowName := "wt-test-dup-window"

	if SessionExists(sessionName) {
		KillSession(sessionName)
	}

	tmpDir, err := os.MkdirTemp("", "wt-tmux-dup-window-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tmpDir)

	err = CreateSession(sessionName, tmpDir)
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}
	defer KillSession(sessionName)

	err = CreateWindow(sessionName, windowName, tmpDir)
	if err != nil {
		t.Fatalf("first CreateWindow() error = %v", err)
	}

	err = CreateWindow(sessionName, windowName, tmpDir)
	if err != ErrWindowExists {
		t.Errorf("CreateWindow() error = %v, want %v", err, ErrWindowExists)
	}
}

func TestKillWindow_NotExists(t *testing.T) {
	if !hasTmux() {
		t.Skip("tmux not installed")
	}

	sessionName := "wt-test-kill-window-session"

	if SessionExists(sessionName) {
		KillSession(sessionName)
	}

	tmpDir, err := os.MkdirTemp("", "wt-tmux-kill-window-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tmpDir)

	err = CreateSession(sessionName, tmpDir)
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}
	defer KillSession(sessionName)

	err = KillWindow(sessionName, "nonexistent-window")
	if err != ErrWindowNotFound {
		t.Errorf("KillWindow() error = %v, want %v", err, ErrWindowNotFound)
	}
}
