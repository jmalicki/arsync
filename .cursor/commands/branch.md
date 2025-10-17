# /branch

Create a new local branch directly from a remote base branch (without checking out the base locally), set upstream, and optionally push.

## Parameters

- **name** (string, required): new branch name (e.g., `copy/fix-io-timeout`)
- **base** (string, optional): remote base branch (default: `main`)
- **remote** (string, optional): remote name (default: `origin`)
- **push** (boolean, optional): push and set upstream to `<remote>/<name>` (default: `true`)

## Branch Strategy

- Do not checkout `main` first; this command fetches the base and branches directly from the remote ref
- Single‑concern branches only; keep scope small to ease review and CI
- Recommended naming: `<area>/<verb-noun>` (e.g., `sync/feat-adaptive-io`, `metadata/fix-xattr-bug`)
- Default base is `main`; for hotfixes or release work, pass the specific base (e.g., `release/v1.0`)

## Behavior

1. Fetch just the base ref and related metadata
2. Create the branch from `<remote>/<base>` directly (no local checkout of `main`)
3. Track `<remote>/<base>` and (optionally) push `name` to remote with upstream
4. Abort if `name` already exists locally

## Example Usage

```bash
/branch "sync/feature-progress-reporting" main origin true
```

## Implementation

When you invoke this command, the AI will execute the following git workflow:

```bash
#!/bin/bash
set -euo pipefail

NAME="${1:?Branch name required}"
BASE="${2:-main}"
REMOTE="${3:-origin}"
PUSH="${4:-true}"

# Check if branch already exists locally
if git show-ref --verify --quiet "refs/heads/${NAME}"; then
    echo "Error: Branch '${NAME}' already exists locally" >&2
    exit 1
fi

# Fetch the base branch ref
echo "Fetching ${REMOTE}/${BASE}..."
git fetch "${REMOTE}" "${BASE}"

# Create new branch from the remote base and switch to it
echo "Creating branch '${NAME}' from ${REMOTE}/${BASE}..."
git switch -c "${NAME}" --track "${REMOTE}/${BASE}"

# Optionally push the new branch and set upstream
if [ "${PUSH}" = "true" ]; then
    echo "Pushing '${NAME}' to ${REMOTE} and setting upstream..."
    git push -u "${REMOTE}" "${NAME}"
fi

echo "✓ Branch '${NAME}' created successfully!"
echo "  Base: ${REMOTE}/${BASE}"
[ "${PUSH}" = "true" ] && echo "  Upstream: ${REMOTE}/${NAME}"
```

## Notes

- Use `/pr` or `/pr-ready` right after creating a branch if you plan to open a PR
- This flow avoids checking out `main` locally; it branches directly from the remote base
- Keeps your local `main` clean and avoids unnecessary checkouts
- Perfect for working on multiple features in parallel without switching to `main`
