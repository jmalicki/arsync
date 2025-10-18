# /coderabbit-review

Read CodeRabbit AI review comments on the current PR and provide AI assistant's opinion on each suggestion **without changing any code**.

```bash
/coderabbit-review
```

This command should:
1. **Fetch PR review comments** from CodeRabbit using GitHub API
2. **Group comments by file** and organize by severity/category
3. **Provide opinion on each comment**:
   - Agree / Disagree / Neutral
   - Rationale for opinion
   - Context about the design decision
   - Whether to accept, defer, or reject the suggestion
4. **Summarize** the review and recommendations

## Output Format

```markdown
## CodeRabbit Review Analysis

PR: #[number] - [title]
Total Comments: [N]
Categories: [list categories]

---

### [Filename]

#### Comment 1: [Summary]
**CodeRabbit says:** [suggestion]
**Location:** Lines [X-Y]

**My Opinion:** [Agree/Disagree/Neutral]

**Rationale:**
- [Point 1]
- [Point 2]

**Recommendation:** [Accept/Defer/Reject]

---

#### Comment 2: ...

---

## Summary

**Agree with:** [N] comments
- [List specific valuable suggestions]

**Disagree with:** [N] comments  
- [List with rationale]

**Neutral/Context-Dependent:** [N] comments
- [List with considerations]

## Recommended Actions

1. **Accept immediately:** [list]
2. **Consider for next PR:** [list]
3. **Won't implement because:** [list with reasons]
```

## Implementation

Use GitHub API to fetch PR review comments:

```bash
# Get current PR number
PR_NUM=$(gh pr view --json number -q .number)

# Get review comments from CodeRabbit
gh api repos/jmalicki/arsync/pulls/$PR_NUM/comments \
  --jq '.[] | select(.user.login == "coderabbitai") | {
    path, line, body, created_at
  }'

# Get review comments on specific commits
gh pr view --json reviews --jq '.reviews[] | 
  select(.author.login == "coderabbitai") | 
  {state, body, submittedAt}'
```

## Analysis Guidelines

When providing opinion:

### Agree When:
- Suggestion improves safety (error handling, bounds checks)
- Enhances readability without changing behavior
- Catches actual bugs or edge cases
- Improves performance meaningfully
- Better aligns with Rust idioms

### Disagree When:
- Suggestion doesn't understand context/design
- Would make code more complex for negligible benefit
- Conflicts with project architecture decisions
- Style preference without objective benefit
- Would break existing functionality

### Neutral When:
- Valid point but low priority
- Design trade-off (no clear winner)
- Stylistic preference
- Would require significant refactoring for minor gain

## Example Usage

```bash
# User runs:
/coderabbit-review

# Agent output:
## CodeRabbit Review Analysis

PR: #72 - Cross-platform support for compio-fs-extended

Total Comments: 15
Categories: Performance (3), Safety (5), Style (4), Documentation (3)

---

### crates/compio-fs-extended/src/xattr.rs

#### Comment 1: Consider using const for XATTR_NOFOLLOW
**CodeRabbit says:** "Define XATTR_NOFOLLOW as a module-level const instead of inline"
**Location:** Lines 494-501

**My Opinion:** Agree

**Rationale:**
- Improves readability (const name is self-documenting)
- DRY principle (used in multiple functions)
- No performance impact (compiler inlines anyway)
- Makes future updates easier

**Recommendation:** Accept - low risk, clear improvement

---

#### Comment 2: Potential panic in CString::new()
**CodeRabbit says:** "CString::new() can panic on interior nulls, handle error"
**Location:** Line 388

**My Opinion:** Disagree

**Rationale:**
- Already using map_err() to handle the error
- Returns Result<>, not panicking
- CodeRabbit may have misread the error handling
- Current code is correct

**Recommendation:** Reject - false positive

---

## Summary

**Agree with:** 8 comments
- XATTR_NOFOLLOW as const (xattr.rs)
- Add safety doc for unsafe blocks (device.rs) 
- Better error messages (metadata.rs)
- ...

**Disagree with:** 4 comments
- False positives on error handling
- Style preferences that hurt readability
- ...

**Neutral:** 3 comments
- Minor optimizations (defer to next PR)
- Documentation improvements (low priority)

## Recommended Actions

1. **Accept immediately:**
   - Define XATTR_NOFOLLOW as const
   - Add safety documentation
   - Improve error messages

2. **Consider for next PR:**
   - Performance micro-optimizations
   - Additional documentation

3. **Won't implement:**
   - Suggestions based on misunderstanding the code
   - Style changes that reduce clarity
```

## Notes

- **No code changes** - this command is read-only
- Provides AI-to-AI dialogue about code review
- Helps user make informed decisions about which suggestions to accept
- Documents rationale for rejecting suggestions
- Can be run multiple times as review comments are added

