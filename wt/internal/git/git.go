package git

import (
	"errors"
	"fmt"
	"os/exec"
	"path/filepath"
	"strings"
)

var (
	ErrNotGitRepo       = errors.New("not inside a git repository")
	ErrWorktreeExists   = errors.New("worktree already exists")
	ErrWorktreeNotFound = errors.New("worktree not found")
	ErrBranchExists     = errors.New("branch already exists")
	ErrInvalidName      = errors.New("invalid worktree name")
)

type Worktree struct {
	Name   string
	Path   string
	Branch string
}

func GetRepoRoot() (string, error) {
	cmd := exec.Command("git", "rev-parse", "--show-toplevel")
	out, err := cmd.Output()
	if err != nil {
		return "", ErrNotGitRepo
	}
	return strings.TrimSpace(string(out)), nil
}

func GetWorktreeRoot() (string, error) {
	cmd := exec.Command("git", "rev-parse", "--path-format=absolute", "--git-common-dir")
	out, err := cmd.Output()
	if err != nil {
		return "", ErrNotGitRepo
	}
	gitDir := strings.TrimSpace(string(out))
	return filepath.Dir(gitDir), nil
}

func ListWorktrees() ([]Worktree, error) {
	cmd := exec.Command("git", "worktree", "list", "--porcelain")
	out, err := cmd.Output()
	if err != nil {
		return nil, fmt.Errorf("failed to list worktrees: %w", err)
	}

	var worktrees []Worktree
	var current Worktree
	lines := strings.Split(string(out), "\n")

	for _, line := range lines {
		line = strings.TrimSpace(line)
		if line == "" {
			if current.Path != "" {
				current.Name = filepath.Base(current.Path)
				worktrees = append(worktrees, current)
				current = Worktree{}
			}
			continue
		}

		if strings.HasPrefix(line, "worktree ") {
			current.Path = strings.TrimPrefix(line, "worktree ")
		} else if strings.HasPrefix(line, "branch ") {
			branch := strings.TrimPrefix(line, "branch ")
			current.Branch = strings.TrimPrefix(branch, "refs/heads/")
		}
	}

	return worktrees, nil
}

func CreateWorktree(name, basePath, branch string) (string, error) {
	if name == "" {
		return "", ErrInvalidName
	}

	worktreePath := filepath.Join(basePath, name)
	exists, err := worktreePathExists(worktreePath)
	if err != nil {
		return "", fmt.Errorf("failed to check existing worktrees: %w", err)
	}
	if exists {
		return "", ErrWorktreeExists
	}

	branchName := branch
	if branchName == "" {
		branchName = name
	}
	if RefExists("refs/heads/" + branchName) {
		return "", ErrBranchExists
	}

	args := []string{"worktree", "add", "-b", branchName, worktreePath}

	cmd := exec.Command("git", args...)
	if out, cmdErr := cmd.CombinedOutput(); cmdErr != nil {
		if strings.Contains(string(out), "already exists") {
			exists, err := worktreePathExists(worktreePath)
			if err == nil && exists {
				return "", ErrWorktreeExists
			}
			if RefExists("refs/heads/" + branchName) {
				return "", ErrBranchExists
			}
			return "", ErrWorktreeExists
		}
		return "", fmt.Errorf("failed to create worktree: %s", strings.TrimSpace(string(out)))
	}

	return worktreePath, nil
}

func CreateWorktreeFromBase(name, basePath, branch, baseRef string) (string, error) {
	if name == "" {
		return "", ErrInvalidName
	}
	if branch == "" {
		return "", ErrInvalidName
	}
	if baseRef == "" {
		return "", fmt.Errorf("base ref cannot be empty")
	}
	worktreePath := filepath.Join(basePath, name)
	exists, err := worktreePathExists(worktreePath)
	if err != nil {
		return "", fmt.Errorf("failed to check existing worktrees: %w", err)
	}
	if exists {
		return "", ErrWorktreeExists
	}
	if RefExists("refs/heads/" + branch) {
		return "", ErrBranchExists
	}

	args := []string{"worktree", "add", "-b", branch, worktreePath, baseRef}

	cmd := exec.Command("git", args...)
	if out, cmdErr := cmd.CombinedOutput(); cmdErr != nil {
		if strings.Contains(string(out), "already exists") {
			exists, err := worktreePathExists(worktreePath)
			if err == nil && exists {
				return "", ErrWorktreeExists
			}
			if RefExists("refs/heads/" + branch) {
				return "", ErrBranchExists
			}
			return "", ErrWorktreeExists
		}
		return "", fmt.Errorf("failed to create worktree: %s", strings.TrimSpace(string(out)))
	}

	return worktreePath, nil
}

func worktreePathExists(worktreePath string) (bool, error) {
	worktrees, err := ListWorktrees()
	if err != nil {
		return false, err
	}
	for _, wt := range worktrees {
		if wt.Path == worktreePath {
			return true, nil
		}
	}
	return false, nil
}

func RefExists(ref string) bool {
	if ref == "" {
		return false
	}
	cmd := exec.Command("git", "show-ref", "--verify", "--quiet", ref)
	return cmd.Run() == nil
}

func RemoveWorktree(path string, force bool) error {
	args := []string{"worktree", "remove"}
	if force {
		args = append(args, "--force")
	}
	args = append(args, path)

	cmd := exec.Command("git", args...)
	if out, err := cmd.CombinedOutput(); err != nil {
		if strings.Contains(string(out), "not a working tree") {
			return ErrWorktreeNotFound
		}
		return fmt.Errorf("failed to remove worktree: %s", strings.TrimSpace(string(out)))
	}

	return nil
}

func WorktreeExists(name, basePath string) (bool, error) {
	worktrees, err := ListWorktrees()
	if err != nil {
		return false, err
	}

	targetPath := filepath.Join(basePath, name)
	for _, wt := range worktrees {
		if wt.Path == targetPath {
			return true, nil
		}
	}
	return false, nil
}

func GetWorktreePath(name, basePath string) (string, error) {
	worktrees, err := ListWorktrees()
	if err != nil {
		return "", err
	}

	targetPath := filepath.Join(basePath, name)
	for _, wt := range worktrees {
		if wt.Path == targetPath {
			return wt.Path, nil
		}
	}
	return "", ErrWorktreeNotFound
}
