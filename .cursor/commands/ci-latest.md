# /ci-latest

Show latest GitHub Actions runs for the current branch and their statuses.

```bash
/ci-latest
```

Implementation:
```bash
gh run list --branch $(git rev-parse --abbrev-ref HEAD) --limit 5
```

To view details of a specific run:
```bash
gh run view <run-id>
```

To view logs:
```bash
gh run view <run-id> --log
```

