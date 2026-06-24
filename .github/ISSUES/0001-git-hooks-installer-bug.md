Title: Hook installer removes .git/hooks and uses symlink — breaks on Windows; prefer git core.hooksPath

Summary
-------
`git-hooks/install-hooks.sh` unconditionally removes `../.git/hooks` and attempts to create a symlink to `../git-hooks/hooks`. On Windows (and some restricted environments) creating symlinks requires elevated privileges or is unsupported, and the script can unintentionally remove existing hooks. This causes the installer to fail or damage developer setups.

Steps to reproduce
------------------
1. On a Windows machine without symlink privileges, run from repo root:

```bash
bash git-hooks/install-hooks.sh
```

2. Observe that `ln -s` fails (or that `.git/hooks` was removed), and that hooks are not installed.

Expected behavior
-----------------
- Installer should set up hooks in a cross-platform way without requiring symlink privileges and without deleting existing hooks unless explicitly requested.

Proposed fix
------------
- Use `git config core.hooksPath git-hooks/hooks` by default (works cross-platform and avoids symlinks).
- Only attempt a symlink as a fallback when `git` is not available, and show a clear warning.
- Only run `rustup component add rustfmt` if `rustup` is installed.

Acceptance criteria
-------------------
- Running `bash git-hooks/install-hooks.sh` configures hooks for normal Git clients on Linux/macOS/Windows without requiring symlink privileges.
- Existing `.git/hooks` is not accidentally deleted without fallback/config instructions.
- The repository documentation includes a short note about installing hooks (optional follow-up).

Suggested labels
----------------
- bug
- platform-windows
- good first issue

Suggested assignee
------------------
- Leave unassigned or assign to core-maintainers

Patch / PR
----------
I have a proposed patch that updates `git-hooks/install-hooks.sh` to prefer `git config core.hooksPath` and to guard `rustup` usage. The patch is ready in a branch `fix/git-hooks-installer`.
