# /review

Summarize current diff and highlight risks, test gaps, and CI considerations.

```bash
/review
```

This command should:
- Show `git diff main...HEAD` summary
- Identify modified modules and their risk level
- Check for test coverage of changed code
- Flag unsafe code blocks
- Check for performance-sensitive changes
- Verify documentation updates
- Check for breaking API changes

