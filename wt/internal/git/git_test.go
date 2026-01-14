package git

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"
)

func setupTestRepo(t *testing.T) (string, func()) {
	t.Helper()

	tmpDir, err := os.MkdirTemp("", "wt-test-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}

	cleanup := func() {
		os.RemoveAll(tmpDir)
	}

	repoDir := filepath.Join(tmpDir, "repo")
	if err := os.MkdirAll(repoDir, 0755); err != nil {
		cleanup()
		t.Fatalf("failed to create repo dir: %v", err)
	}

	cmds := [][]string{
		{"git", "init"},
		{"git", "config", "user.email", "test@test.com"},
		{"git", "config", "user.name", "Test"},
	}

	for _, args := range cmds {
		cmd := exec.Command(args[0], args[1:]...)
		cmd.Dir = repoDir
		if out, err := cmd.CombinedOutput(); err != nil {
			cleanup()
			t.Fatalf("failed to run %v: %v\n%s", args, err, out)
		}
	}

	testFile := filepath.Join(repoDir, "test.txt")
	if err := os.WriteFile(testFile, []byte("test"), 0644); err != nil {
		cleanup()
		t.Fatalf("failed to write test file: %v", err)
	}

	commitCmds := [][]string{
		{"git", "add", "."},
		{"git", "commit", "-m", "initial"},
	}

	for _, args := range commitCmds {
		cmd := exec.Command(args[0], args[1:]...)
		cmd.Dir = repoDir
		if out, err := cmd.CombinedOutput(); err != nil {
			cleanup()
			t.Fatalf("failed to run %v: %v\n%s", args, err, out)
		}
	}

	oldWd, _ := os.Getwd()
	os.Chdir(repoDir)

	return tmpDir, func() {
		os.Chdir(oldWd)
		cleanup()
	}
}

func TestGetRepoRoot(t *testing.T) {
	tmpDir, cleanup := setupTestRepo(t)
	defer cleanup()

	root, err := GetRepoRoot()
	if err != nil {
		t.Fatalf("GetRepoRoot() error = %v", err)
	}

	expected := filepath.Join(tmpDir, "repo")
	if root != expected {
		t.Errorf("GetRepoRoot() = %v, want %v", root, expected)
	}
}

func TestGetRepoRoot_NotInRepo(t *testing.T) {
	tmpDir, err := os.MkdirTemp("", "wt-test-norepo-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tmpDir)

	oldWd, _ := os.Getwd()
	os.Chdir(tmpDir)
	defer os.Chdir(oldWd)

	_, err = GetRepoRoot()
	if err != ErrNotGitRepo {
		t.Errorf("GetRepoRoot() error = %v, want %v", err, ErrNotGitRepo)
	}
}

func TestListWorktrees(t *testing.T) {
	_, cleanup := setupTestRepo(t)
	defer cleanup()

	worktrees, err := ListWorktrees()
	if err != nil {
		t.Fatalf("ListWorktrees() error = %v", err)
	}

	if len(worktrees) != 1 {
		t.Errorf("ListWorktrees() got %d worktrees, want 1", len(worktrees))
	}
}

func TestCreateWorktree(t *testing.T) {
	tmpDir, cleanup := setupTestRepo(t)
	defer cleanup()

	wtPath, err := CreateWorktree("test-branch", tmpDir, "")
	if err != nil {
		t.Fatalf("CreateWorktree() error = %v", err)
	}

	expected := filepath.Join(tmpDir, "test-branch")
	if wtPath != expected {
		t.Errorf("CreateWorktree() path = %v, want %v", wtPath, expected)
	}

	if _, err := os.Stat(wtPath); os.IsNotExist(err) {
		t.Errorf("worktree directory was not created")
	}

	worktrees, _ := ListWorktrees()
	if len(worktrees) != 2 {
		t.Errorf("expected 2 worktrees after creation, got %d", len(worktrees))
	}
}

func TestCreateWorktree_InvalidName(t *testing.T) {
	_, cleanup := setupTestRepo(t)
	defer cleanup()

	_, err := CreateWorktree("", "/tmp", "")
	if err != ErrInvalidName {
		t.Errorf("CreateWorktree() error = %v, want %v", err, ErrInvalidName)
	}
}

func TestCreateWorktree_AlreadyExists(t *testing.T) {
	tmpDir, cleanup := setupTestRepo(t)
	defer cleanup()

	_, err := CreateWorktree("dup-branch", tmpDir, "")
	if err != nil {
		t.Fatalf("first CreateWorktree() error = %v", err)
	}

	_, err = CreateWorktree("dup-branch", tmpDir, "")
	if err == nil {
		t.Error("second CreateWorktree() expected error, got nil")
	}
}

func TestRemoveWorktree(t *testing.T) {
	tmpDir, cleanup := setupTestRepo(t)
	defer cleanup()

	wtPath, err := CreateWorktree("to-remove", tmpDir, "")
	if err != nil {
		t.Fatalf("CreateWorktree() error = %v", err)
	}

	err = RemoveWorktree(wtPath, false)
	if err != nil {
		t.Fatalf("RemoveWorktree() error = %v", err)
	}

	if _, err := os.Stat(wtPath); !os.IsNotExist(err) {
		t.Errorf("worktree directory still exists after removal")
	}
}

func TestRemoveWorktree_NotFound(t *testing.T) {
	_, cleanup := setupTestRepo(t)
	defer cleanup()

	err := RemoveWorktree("/nonexistent/path", false)
	if err == nil {
		t.Error("RemoveWorktree() expected error for nonexistent path, got nil")
	}
}

func TestWorktreeExists(t *testing.T) {
	tmpDir, cleanup := setupTestRepo(t)
	defer cleanup()

	if WorktreeExists("nonexistent", tmpDir) {
		t.Error("WorktreeExists() = true for nonexistent worktree")
	}

	CreateWorktree("exists-test", tmpDir, "")

	if !WorktreeExists("exists-test", tmpDir) {
		t.Error("WorktreeExists() = false for existing worktree")
	}
}

func TestGetWorktreePath(t *testing.T) {
	tmpDir, cleanup := setupTestRepo(t)
	defer cleanup()

	expected, _ := CreateWorktree("path-test", tmpDir, "")

	got, err := GetWorktreePath("path-test", tmpDir)
	if err != nil {
		t.Fatalf("GetWorktreePath() error = %v", err)
	}

	if got != expected {
		t.Errorf("GetWorktreePath() = %v, want %v", got, expected)
	}
}

func TestGetWorktreePath_NotFound(t *testing.T) {
	tmpDir, cleanup := setupTestRepo(t)
	defer cleanup()

	_, err := GetWorktreePath("nonexistent", tmpDir)
	if err != ErrWorktreeNotFound {
		t.Errorf("GetWorktreePath() error = %v, want %v", err, ErrWorktreeNotFound)
	}
}
