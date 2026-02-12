package git

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
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
	if err != ErrWorktreeExists {
		t.Fatalf("expected ErrWorktreeExists, got %v", err)
	}
}

func TestCreateWorktree_BranchAlreadyExists(t *testing.T) {
	tmpDir, cleanup := setupTestRepo(t)
	defer cleanup()

	cmd := exec.Command("git", "branch", "existing-branch")
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("failed to create existing branch: %v\n%s", err, out)
	}

	_, err := CreateWorktree("new-worktree", tmpDir, "existing-branch")
	if err != ErrBranchExists {
		t.Fatalf("expected ErrBranchExists, got %v", err)
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

	exists, err := WorktreeExists("nonexistent", tmpDir)
	if err != nil {
		t.Fatalf("WorktreeExists() error = %v", err)
	}
	if exists {
		t.Error("WorktreeExists() = true for nonexistent worktree")
	}

	CreateWorktree("exists-test", tmpDir, "")

	exists, err = WorktreeExists("exists-test", tmpDir)
	if err != nil {
		t.Fatalf("WorktreeExists() error = %v", err)
	}
	if !exists {
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

func TestCreateWorktreeFromBase(t *testing.T) {
	tmpDir, cleanup := setupTestRepo(t)
	defer cleanup()

	wtPath, err := CreateWorktreeFromBase("from-base", tmpDir, "from-base", "HEAD")
	if err != nil {
		t.Fatalf("CreateWorktreeFromBase() error = %v", err)
	}

	expected := filepath.Join(tmpDir, "from-base")
	if wtPath != expected {
		t.Fatalf("CreateWorktreeFromBase() = %s, want %s", wtPath, expected)
	}

	if _, err := os.Stat(wtPath); os.IsNotExist(err) {
		t.Fatalf("expected worktree path to exist: %s", wtPath)
	}
}

func TestCreateWorktreeFromBase_InvalidInputs(t *testing.T) {
	tmpDir, cleanup := setupTestRepo(t)
	defer cleanup()

	if _, err := CreateWorktreeFromBase("", tmpDir, "b", "HEAD"); err != ErrInvalidName {
		t.Fatalf("expected ErrInvalidName for empty name, got %v", err)
	}
	if _, err := CreateWorktreeFromBase("x", tmpDir, "", "HEAD"); err != ErrInvalidName {
		t.Fatalf("expected ErrInvalidName for empty branch, got %v", err)
	}
	if _, err := CreateWorktreeFromBase("x", tmpDir, "x", ""); err == nil || !strings.Contains(err.Error(), "base ref cannot be empty") {
		t.Fatalf("expected base ref error, got %v", err)
	}
}

func TestCreateWorktreeFromBase_AlreadyExists(t *testing.T) {
	tmpDir, cleanup := setupTestRepo(t)
	defer cleanup()

	_, err := CreateWorktreeFromBase("dup-base", tmpDir, "dup-base", "HEAD")
	if err != nil {
		t.Fatalf("first CreateWorktreeFromBase() error = %v", err)
	}

	_, err = CreateWorktreeFromBase("dup-base", tmpDir, "dup-base-2", "HEAD")
	if err != ErrWorktreeExists {
		t.Fatalf("expected ErrWorktreeExists for duplicate worktree path, got %v", err)
	}
}

func TestCreateWorktreeFromBase_BranchAlreadyExists(t *testing.T) {
	tmpDir, cleanup := setupTestRepo(t)
	defer cleanup()

	cmd := exec.Command("git", "branch", "existing-branch")
	if out, err := cmd.CombinedOutput(); err != nil {
		t.Fatalf("failed to create existing branch: %v\n%s", err, out)
	}

	_, err := CreateWorktreeFromBase("from-base-new", tmpDir, "existing-branch", "HEAD")
	if err != ErrBranchExists {
		t.Fatalf("expected ErrBranchExists, got %v", err)
	}
}

func TestRefExists(t *testing.T) {
	_, cleanup := setupTestRepo(t)
	defer cleanup()

	if !RefExists("refs/heads/master") && !RefExists("refs/heads/main") {
		t.Fatalf("expected either refs/heads/master or refs/heads/main to exist")
	}

	if RefExists("refs/heads/definitely-does-not-exist") {
		t.Fatalf("expected missing ref to return false")
	}

	if RefExists("") {
		t.Fatalf("expected empty ref to return false")
	}
}
