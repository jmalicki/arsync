# /branch

Create a new local branch directly from a remote base branch (without checking out the base locally), set upstream, and optionally push.

Branch strategy (guidance):
- Do not checkout `main` first; this command fetches the base and branches directly from the remote ref
- Single‑concern branches only; keep scope small to ease review and CI
- Recommended naming: `<area>/<verb-noun>` (e.g., `sync/feat-adaptive-io`, `metadata/fix-xattr-bug`) 
- Default base is `main`; for hotfixes or release work, pass the specific base (e.g., `release/v1.0`)

- name (string, required): new branch name (e.g., `copy/fix-io-timeout`)
- base (string, optional): remote base branch (default: `main`)
- remote (string, optional): remote name (default: `origin`)
- push (boolean, optional): push and set upstream to `<remote>/<name>` (default: `true`)

Behavior:
- Fetch just the base ref and related metadata
- Create the branch from `<remote>/<base>` directly (no local checkout of `main`)
- Track `<remote>/<base>` and (optionally) push `name` to remote with upstream
- Abort if `name` already exists locally

Example:
```bash
/branch "sync/feature-progress-reporting" main origin true
```

Implementation outline (what this command does under the hood):
```bash
# 1) Fetch the base branch ref
git fetch "${REMOTE}" "${BASE}"

# 2) Create new branch from the remote base and switch to it
git switch -c "${NAME}" --track "${REMOTE}/${BASE}"

# 3) Optionally push the new branch and set upstream
[ "${PUSH:-true}" = "true" ] && git push -u "${REMOTE}" "${NAME}"
```

Notes:
- Use `/pr` or `/pr-ready` right after creating a branch if you plan to open a PR
- This flow avoids checking out `main` locally; it branches directly from the remote base

