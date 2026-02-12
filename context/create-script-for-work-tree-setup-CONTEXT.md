# Branch Context: create-script-for-work-tree-setup

## Current Goal
Validate Claude's review in `result.md` and apply only legitimate fixes.

## Decisions
- Accepted and fixed:
  - Slot occupancy now only considers worktrees at `<basePath>/<slot>` (prevents unrelated basename collisions).
  - Slot rollback now surfaces cleanup failure instead of discarding it.
  - Git worktree creation now distinguishes `ErrBranchExists` vs `ErrWorktreeExists` with explicit pre-checks and fallback classification.
  - `ensureContextSymlink` now uses create-first handling (`os.Symlink` then inspect on `EEXIST`) and verifies existing matching symlinks are not broken.
  - Tests now restore mutable package globals via `t.Cleanup` to prevent state leakage.

- Reviewed but not treated as required in this pass:
  - Broad refactor to de-duplicate `CreateWorktree` and `CreateWorktreeFromBase`.
  - Additional validation on `--base` format.
  - Concurrency/race hardening for concurrent `wt slot` invocations.

## Files Changed
- `wt/cmd/slot.go`
- `wt/cmd/slot_test.go`
- `wt/internal/git/git.go`
- `wt/internal/git/git_test.go`

## Verification
- Ran: `go test ./...` in `wt/`
- Result: all tests passing.
