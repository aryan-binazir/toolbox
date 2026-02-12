package cmd

import (
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"wt/internal/git"
)

func TestFirstMissingSlot(t *testing.T) {
	tests := []struct {
		name     string
		existing map[string]bool
		want     string
	}{
		{name: "none exist", existing: map[string]bool{}, want: "alpha"},
		{name: "alpha exists", existing: map[string]bool{"alpha": true}, want: "beta"},
		{name: "beta exists but alpha missing", existing: map[string]bool{"beta": true}, want: "alpha"},
		{name: "alpha beta exist", existing: map[string]bool{"alpha": true, "beta": true}, want: "gamma"},
		{name: "alpha gamma exist", existing: map[string]bool{"alpha": true, "gamma": true}, want: "beta"},
		{name: "all exist", existing: map[string]bool{"alpha": true, "beta": true, "gamma": true, "delta": true}, want: ""},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := firstMissingSlot(tt.existing)
			if got != tt.want {
				t.Fatalf("firstMissingSlot() = %q, want %q", got, tt.want)
			}
		})
	}
}

func TestExistingSlots_UsesConfiguredBasePath(t *testing.T) {
	basePath := "/tmp/worktrees"
	worktrees := []git.Worktree{
		{Path: "/tmp/repo", Branch: "main"},
		{Path: "/tmp/unrelated/alpha", Branch: "feature-a"},
		{Path: filepath.Join(basePath, "beta"), Branch: "beta"},
	}

	got := existingSlots(worktrees, basePath)
	if got["alpha"] {
		t.Fatalf("expected unrelated alpha worktree to be ignored")
	}
	if !got["beta"] {
		t.Fatalf("expected beta slot to be marked as existing")
	}
}

func TestFindBranchWorktreePath(t *testing.T) {
	worktrees := []git.Worktree{
		{Path: "/tmp/repo", Branch: "main"},
		{Path: "/tmp/alpha", Branch: "alpha"},
	}

	got, err := findBranchWorktreePath(worktrees, "main")
	if err != nil {
		t.Fatalf("expected success, got error: %v", err)
	}
	if got != "/tmp/repo" {
		t.Fatalf("expected /tmp/repo, got %s", got)
	}

	_, err = findBranchWorktreePath(worktrees, "missing")
	if err == nil || !strings.Contains(err.Error(), "could not find a checked-out worktree") {
		t.Fatalf("expected missing branch error, got: %v", err)
	}
}

func TestEnsureContextSymlink(t *testing.T) {
	tmpDir, err := os.MkdirTemp("", "wt-context-link-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}
	defer os.RemoveAll(tmpDir)

	source := filepath.Join(tmpDir, "source")
	target := filepath.Join(tmpDir, "target")
	if err := os.MkdirAll(source, 0755); err != nil {
		t.Fatalf("failed to make source dir: %v", err)
	}

	if err := ensureContextSymlink(source, target); err != nil {
		t.Fatalf("expected symlink creation success, got: %v", err)
	}

	if fi, err := os.Lstat(target); err != nil || fi.Mode()&os.ModeSymlink == 0 {
		t.Fatalf("expected target to be symlink, err=%v", err)
	}

	if err := ensureContextSymlink(source, target); err != nil {
		t.Fatalf("expected existing matching symlink to be accepted, got: %v", err)
	}

	otherSource := filepath.Join(tmpDir, "other")
	if err := os.MkdirAll(otherSource, 0755); err != nil {
		t.Fatalf("failed to make other source dir: %v", err)
	}
	otherTarget := filepath.Join(tmpDir, "other-target")
	if err := os.Symlink(otherSource, otherTarget); err != nil {
		t.Fatalf("failed to create preexisting symlink: %v", err)
	}

	err = ensureContextSymlink(source, otherTarget)
	if err == nil || !strings.Contains(err.Error(), "different target") {
		t.Fatalf("expected different target error, got: %v", err)
	}

	fileTarget := filepath.Join(tmpDir, "file-target")
	if err := os.WriteFile(fileTarget, []byte("data"), 0644); err != nil {
		t.Fatalf("failed to create regular file target: %v", err)
	}

	err = ensureContextSymlink(source, fileTarget)
	if err == nil || !strings.Contains(err.Error(), "not a symlink") {
		t.Fatalf("expected not a symlink error, got: %v", err)
	}

	brokenTarget := filepath.Join(tmpDir, "broken-target")
	brokenSource := filepath.Join(tmpDir, "does-not-exist")
	if err := os.Symlink(brokenSource, brokenTarget); err != nil {
		t.Fatalf("failed to create broken symlink: %v", err)
	}
	err = ensureContextSymlink(brokenSource, brokenTarget)
	if err == nil || !strings.Contains(err.Error(), "points to missing source") {
		t.Fatalf("expected missing source error, got: %v", err)
	}
}

func TestResolveBaseRef(t *testing.T) {
	repoDir, cleanup := setupSlotRepo(t, true)
	defer cleanup()

	oldWd, _ := os.Getwd()
	if err := os.Chdir(repoDir); err != nil {
		t.Fatalf("failed to chdir: %v", err)
	}
	defer os.Chdir(oldWd)

	got, err := resolveBaseRef("main")
	if err != nil {
		t.Fatalf("expected local main ref to resolve, got error: %v", err)
	}
	if got != "main" {
		t.Fatalf("expected main, got %s", got)
	}

	_, err = resolveBaseRef("does-not-exist")
	if err == nil || !strings.Contains(err.Error(), "base branch not found") {
		t.Fatalf("expected missing base branch error, got: %v", err)
	}
}

func TestRunSlotCreate_FailureCases(t *testing.T) {
	t.Run("empty base branch", func(t *testing.T) {
		orig := slotBaseBranch
		defer func() { slotBaseBranch = orig }()
		slotBaseBranch = ""
		err := runSlotCreate(nil, nil)
		if err == nil || !strings.Contains(err.Error(), "base branch cannot be empty") {
			t.Fatalf("expected base branch validation error, got: %v", err)
		}
	})

	t.Run("missing context in base worktree", func(t *testing.T) {
		repoDir, cleanup := setupSlotRepo(t, false)
		defer cleanup()

		oldWd, _ := os.Getwd()
		if err := os.Chdir(repoDir); err != nil {
			t.Fatalf("failed to chdir: %v", err)
		}
		defer os.Chdir(oldWd)

		origWorktreeDir := worktreeDir
		origSlotBaseBranch := slotBaseBranch
		t.Cleanup(func() {
			worktreeDir = origWorktreeDir
			slotBaseBranch = origSlotBaseBranch
		})

		worktreeDir = ""
		slotBaseBranch = "main"
		err := runSlotCreate(nil, nil)
		if err == nil || !strings.Contains(err.Error(), "context directory not found") {
			t.Fatalf("expected missing context error, got: %v", err)
		}
	})
}

func TestRunSlotCreate_CreatesAllSlotsThenErrors(t *testing.T) {
	repoDir, cleanup := setupSlotRepo(t, true)
	defer cleanup()

	oldWd, _ := os.Getwd()
	if err := os.Chdir(repoDir); err != nil {
		t.Fatalf("failed to chdir: %v", err)
	}
	defer os.Chdir(oldWd)

	origWorktreeDir := worktreeDir
	origSlotBaseBranch := slotBaseBranch
	t.Cleanup(func() {
		worktreeDir = origWorktreeDir
		slotBaseBranch = origSlotBaseBranch
	})

	worktreeDir = ""
	slotBaseBranch = "main"

	slots := []string{"alpha", "beta", "gamma", "delta"}
	for _, s := range slots {
		if err := runSlotCreate(nil, nil); err != nil {
			t.Fatalf("runSlotCreate failed for slot %s: %v", s, err)
		}

		slotPath := filepath.Join(filepath.Dir(repoDir), s)
		if _, err := os.Stat(slotPath); err != nil {
			t.Fatalf("expected slot worktree %s to exist: %v", s, err)
		}

		contextPath := filepath.Join(slotPath, "context")
		fi, err := os.Lstat(contextPath)
		if err != nil {
			t.Fatalf("expected context symlink for %s: %v", s, err)
		}
		if fi.Mode()&os.ModeSymlink == 0 {
			t.Fatalf("expected %s/context to be symlink", s)
		}
	}

	err := runSlotCreate(nil, nil)
	if err == nil || !strings.Contains(err.Error(), "all slots already exist") {
		t.Fatalf("expected all slots exist error, got: %v", err)
	}
}

func setupSlotRepo(t *testing.T, withContext bool) (string, func()) {
	t.Helper()
	if !hasGit() {
		t.Skip("git not installed")
	}

	tmpDir, err := os.MkdirTemp("", "wt-slot-*")
	if err != nil {
		t.Fatalf("failed to create temp dir: %v", err)
	}

	repoDir := filepath.Join(tmpDir, "repo")
	if err := os.MkdirAll(repoDir, 0755); err != nil {
		os.RemoveAll(tmpDir)
		t.Fatalf("failed to create repo dir: %v", err)
	}

	cmds := [][]string{
		{"git", "init"},
		{"git", "config", "user.email", "test@test.com"},
		{"git", "config", "user.name", "Test"},
	}
	for _, args := range cmds {
		c := exec.Command(args[0], args[1:]...)
		c.Dir = repoDir
		if out, err := c.CombinedOutput(); err != nil {
			os.RemoveAll(tmpDir)
			t.Fatalf("command failed: %v\n%s", err, out)
		}
	}

	if err := os.WriteFile(filepath.Join(repoDir, "README.md"), []byte("init\n"), 0644); err != nil {
		os.RemoveAll(tmpDir)
		t.Fatalf("failed to write README: %v", err)
	}
	if withContext {
		if err := os.Mkdir(filepath.Join(repoDir, "context"), 0755); err != nil {
			os.RemoveAll(tmpDir)
			t.Fatalf("failed to create context dir: %v", err)
		}
	}

	commitCmds := [][]string{
		{"git", "add", "README.md"},
		{"git", "commit", "-m", "initial"},
		{"git", "branch", "-m", "main"},
	}
	for _, args := range commitCmds {
		c := exec.Command(args[0], args[1:]...)
		c.Dir = repoDir
		if out, err := c.CombinedOutput(); err != nil {
			os.RemoveAll(tmpDir)
			t.Fatalf("command failed: %v\n%s", err, out)
		}
	}

	cleanup := func() {
		os.RemoveAll(tmpDir)
	}
	return repoDir, cleanup
}
