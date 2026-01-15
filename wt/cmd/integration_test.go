package cmd

import (
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"wt/internal/git"
	"wt/internal/tmux"
)

func hasGit() bool {
	_, err := exec.LookPath("git")
	return err == nil
}

func hasTmux() bool {
	_, err := exec.LookPath("tmux")
	return err == nil
}

func skipIfMissingDeps(t *testing.T) {
	t.Helper()
	if !hasGit() {
		t.Skip("git not installed")
	}
	if !hasTmux() {
		t.Skip("tmux not installed")
	}
}

// setupIntegrationTest creates a temp git repo and returns cleanup func
func setupIntegrationTest(t *testing.T) (repoDir string, cleanup func()) {
	t.Helper()

	tmpDir, err := os.MkdirTemp("", "wt-integration-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}

	repoDir = filepath.Join(tmpDir, "repo")
	if err := os.MkdirAll(repoDir, 0755); err != nil {
		os.RemoveAll(tmpDir)
		t.Fatalf("failed to create repo dir: %v", err)
	}

	// Initialize git repo
	cmds := [][]string{
		{"git", "init"},
		{"git", "config", "user.email", "test@test.com"},
		{"git", "config", "user.name", "Test"},
	}
	for _, args := range cmds {
		cmd := exec.Command(args[0], args[1:]...)
		cmd.Dir = repoDir
		if out, err := cmd.CombinedOutput(); err != nil {
			os.RemoveAll(tmpDir)
			t.Fatalf("git init failed: %v\n%s", err, out)
		}
	}

	// Create initial commit
	testFile := filepath.Join(repoDir, "test.txt")
	if err := os.WriteFile(testFile, []byte("test"), 0644); err != nil {
		os.RemoveAll(tmpDir)
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
			os.RemoveAll(tmpDir)
			t.Fatalf("git commit failed: %v\n%s", err, out)
		}
	}

	cleanup = func() {
		// Kill any test sessions we created
		sessions := []string{"wt-test-create", "wt-test-attach", "wt-test-list", "wt-test-partial", "wt-test-dup"}
		for _, s := range sessions {
			if tmux.SessionExists(s) {
				tmux.KillSession(s)
			}
		}
		os.RemoveAll(tmpDir)
	}

	return repoDir, cleanup
}

// resetFlags resets all global flags to their default values
func resetFlags() {
	noAttach = false
	asWindow = false
	branch = ""
	worktreeDir = ""
	skipConfirm = false
	force = false
	deleteWindow = false
}

func TestIntegration_CreateAndDelete(t *testing.T) {
	skipIfMissingDeps(t)
	repoDir, cleanup := setupIntegrationTest(t)
	defer cleanup()

	name := "wt-test-create"
	basePath := filepath.Dir(repoDir)

	// Save current dir, change to repo
	oldWd, _ := os.Getwd()
	os.Chdir(repoDir)
	defer os.Chdir(oldWd)

	// Reset and set flags for test
	resetFlags()
	noAttach = true

	err := runCreate(nil, []string{name})
	if err != nil {
		t.Fatalf("runCreate() error = %v", err)
	}

	// Verify worktree exists
	exists, err := git.WorktreeExists(name, basePath)
	if err != nil {
		t.Fatalf("WorktreeExists() error = %v", err)
	}
	if !exists {
		t.Error("worktree was not created")
	}

	// Verify session exists
	if !tmux.SessionExists(name) {
		t.Error("tmux session was not created")
	}

	// Reset and set flags for delete
	resetFlags()
	skipConfirm = true

	err = runDelete(nil, []string{name})
	if err != nil {
		t.Fatalf("runDelete() error = %v", err)
	}

	// Verify cleanup
	exists, _ = git.WorktreeExists(name, basePath)
	if exists {
		t.Error("worktree still exists after delete")
	}
	if tmux.SessionExists(name) {
		t.Error("tmux session still exists after delete")
	}
}

func TestIntegration_AttachCreatesSession(t *testing.T) {
	skipIfMissingDeps(t)
	repoDir, cleanup := setupIntegrationTest(t)
	defer cleanup()

	name := "wt-test-attach"
	basePath := filepath.Dir(repoDir)

	oldWd, _ := os.Getwd()
	os.Chdir(repoDir)
	defer os.Chdir(oldWd)

	// Reset flags
	resetFlags()

	// Create worktree without session by using git directly
	_, err := git.CreateWorktree(name, basePath, "")
	if err != nil {
		t.Fatalf("CreateWorktree() error = %v", err)
	}
	defer git.RemoveWorktree(filepath.Join(basePath, name), true)

	// Verify no session yet
	if tmux.SessionExists(name) {
		t.Fatal("session should not exist before attach")
	}

	// runAttach would block (it tries to attach), so we test the session creation logic directly
	// by calling the internal path that creates session if missing
	// The attach command would create session and attach - we can't test the attach part
	// but we can verify the worktree lookup works
	wtPath, err := git.GetWorktreePath(name, basePath)
	if err != nil {
		t.Fatalf("GetWorktreePath() error = %v", err)
	}

	// Create session manually to verify the pattern works
	err = tmux.CreateSession(name, wtPath)
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}
	defer tmux.KillSession(name)

	if !tmux.SessionExists(name) {
		t.Error("session was not created")
	}
}

func TestIntegration_DeletePartialState(t *testing.T) {
	skipIfMissingDeps(t)
	repoDir, cleanup := setupIntegrationTest(t)
	defer cleanup()

	name := "wt-test-partial"

	oldWd, _ := os.Getwd()
	os.Chdir(repoDir)
	defer os.Chdir(oldWd)

	// Create only a session (no worktree) - simulates orphaned state
	err := tmux.CreateSession(name, repoDir)
	if err != nil {
		t.Fatalf("CreateSession() error = %v", err)
	}

	// Reset and set flags for delete
	resetFlags()
	skipConfirm = true

	err = runDelete(nil, []string{name})
	if err != nil {
		t.Fatalf("runDelete() error = %v", err)
	}

	if tmux.SessionExists(name) {
		t.Error("session still exists after delete")
	}
}

func TestIntegration_CreateDuplicate(t *testing.T) {
	skipIfMissingDeps(t)
	repoDir, cleanup := setupIntegrationTest(t)
	defer cleanup()

	name := "wt-test-dup"

	oldWd, _ := os.Getwd()
	os.Chdir(repoDir)
	defer os.Chdir(oldWd)

	// Reset and set flags for first create
	resetFlags()
	noAttach = true

	err := runCreate(nil, []string{name})
	if err != nil {
		t.Fatalf("first runCreate() error = %v", err)
	}
	defer func() {
		resetFlags()
		skipConfirm = true
		force = true
		runDelete(nil, []string{name})
	}()

	// Reset flags for second create attempt
	resetFlags()
	noAttach = true

	// Second create should fail
	err = runCreate(nil, []string{name})
	if err == nil {
		t.Error("second runCreate() should have failed for duplicate")
	}
}

func TestIntegration_CreateWithCustomBranch(t *testing.T) {
	skipIfMissingDeps(t)
	repoDir, cleanup := setupIntegrationTest(t)
	defer cleanup()

	name := "wt-test-branch"
	customBranch := "feature/custom-branch"

	oldWd, _ := os.Getwd()
	os.Chdir(repoDir)
	defer os.Chdir(oldWd)

	// Reset and set flags
	resetFlags()
	noAttach = true
	branch = customBranch

	err := runCreate(nil, []string{name})
	if err != nil {
		t.Fatalf("runCreate() with custom branch error = %v", err)
	}
	defer func() {
		resetFlags()
		skipConfirm = true
		force = true
		runDelete(nil, []string{name})
	}()

	// Verify worktree was created
	basePath := filepath.Dir(repoDir)
	exists, err := git.WorktreeExists(name, basePath)
	if err != nil {
		t.Fatalf("WorktreeExists() error = %v", err)
	}
	if !exists {
		t.Error("worktree was not created")
	}

	// Verify session exists
	if !tmux.SessionExists(name) {
		t.Error("tmux session was not created")
	}
}

func TestIntegration_DeleteWithForce(t *testing.T) {
	skipIfMissingDeps(t)
	repoDir, cleanup := setupIntegrationTest(t)
	defer cleanup()

	name := "wt-test-force"
	basePath := filepath.Dir(repoDir)

	oldWd, _ := os.Getwd()
	os.Chdir(repoDir)
	defer os.Chdir(oldWd)

	// Create worktree and session
	resetFlags()
	noAttach = true

	err := runCreate(nil, []string{name})
	if err != nil {
		t.Fatalf("runCreate() error = %v", err)
	}

	// Add uncommitted changes to the worktree
	wtPath := filepath.Join(basePath, name)
	newFile := filepath.Join(wtPath, "uncommitted.txt")
	if err := os.WriteFile(newFile, []byte("uncommitted"), 0644); err != nil {
		t.Fatalf("failed to write uncommitted file: %v", err)
	}

	// Delete with force should succeed
	resetFlags()
	skipConfirm = true
	force = true

	err = runDelete(nil, []string{name})
	if err != nil {
		t.Fatalf("runDelete() with force error = %v", err)
	}

	// Verify cleanup
	exists, _ := git.WorktreeExists(name, basePath)
	if exists {
		t.Error("worktree still exists after force delete")
	}
	if tmux.SessionExists(name) {
		t.Error("tmux session still exists after delete")
	}
}

func TestIntegration_DeleteNonExistent(t *testing.T) {
	skipIfMissingDeps(t)
	repoDir, cleanup := setupIntegrationTest(t)
	defer cleanup()

	oldWd, _ := os.Getwd()
	os.Chdir(repoDir)
	defer os.Chdir(oldWd)

	resetFlags()
	skipConfirm = true

	// Attempting to delete non-existent should return error
	err := runDelete(nil, []string{"nonexistent-worktree-12345"})
	if err == nil {
		t.Error("runDelete() should have failed for non-existent target")
	}
}
