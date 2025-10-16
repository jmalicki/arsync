# /pr-checks

Watch PR checks for the current branch and keep streaming updates until completion.

Recommended approach (uses GitHub CLI):
- `gh pr checks --watch` streams status for the open PR tied to the current branch
- If no PR exists yet, use `/pr-ready` first

```bash
/pr-checks
```

Alternatives and tips:
- Specific run: `gh run list --branch $(git rev-parse --abbrev-ref HEAD) --limit 1 | awk '{print $7}' | xargs gh run watch`
- One‑shot summary (no stream): `gh pr checks`
- Full PR status: `gh pr view --json url,headRefName,statusCheckRollup`
- View workflow logs: `gh run view --log`

