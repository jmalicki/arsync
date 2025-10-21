# /review-comments

Read and analyze ALL review comments on the current PR (from any reviewer: humans, CodeRabbit, bots) and provide AI assistant's opinion on each suggestion **without changing any code**.

```bash
/review-comments
```

Optionally filter by reviewer:
```bash
/review-comments coderabbitai
/review-comments @username
```

This command should:
1. **Fetch all PR review comments** using GitHub API (from all reviewers by default)
2. **Group comments by file and reviewer**
3. **Provide opinion on each comment**:
   - Agree / Disagree / Neutral
   - Rationale for opinion
   - Context about the design decision
   - Whether to accept, defer, or reject the suggestion
4. **Post replies directly on the PR** using `gh pr comment` or `gh api` for inline comments
5. **Summarize** the review and recommendations

## Output Format

```markdown
## PR Review Analysis

PR: #[number] - [title]
Total Comments: [N] from [M] reviewers
Reviewers: [list]

---

### [Filename]

#### Comment 1: [Summary]
**Reviewer:** [@username or bot-name]
**Comment:** [suggestion]
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

# Get ALL review comments (inline comments on code)
gh api "repos/OWNER/REPO/pulls/$PR_NUM/comments" \
  --jq '.[] | {
    id, path, line, body, 
    user: .user.login, 
    created_at
  }'

# Get review summaries (top-level review comments)
gh pr view --json reviews --jq '.reviews[] | {
  author: .author.login,
  state,
  body,
  submittedAt
}'

# Filter by specific reviewer (optional):
gh api "repos/OWNER/REPO/pulls/$PR_NUM/comments" \
  --jq '.[] | select(.user.login == "coderabbitai[bot]") | ...'
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
/review-comments

# Agent output:
## PR Review Analysis

PR: #72 - Cross-platform support for compio-fs-extended

Total Comments: 23 from 3 reviewers
Reviewers: coderabbitai[bot] (20), @alice (2), @bob (1)

---

### crates/compio-fs-extended/src/xattr.rs

#### Comment 1: Consider using const for XATTR_NOFOLLOW
**Reviewer:** coderabbitai[bot]
**Comment:** "Define XATTR_NOFOLLOW as a module-level const instead of inline"
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
**Reviewer:** coderabbitai[bot]
**Comment:** "CString::new() can panic on interior nulls, handle error"
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

## Posting Replies

After analysis, **post responses directly on the PR**:

```bash
# Reply to inline comment
gh api "repos/OWNER/REPO/pulls/comments/$COMMENT_ID/replies" \
  -X POST \
  -f body="Your analysis and response"

# Post general PR comment
gh pr comment $PR_NUM --body "Summary of review analysis"
```

**Response format**:
- Acknowledge the suggestion
- State agreement/disagreement with rationale
- If agreeing: Confirm will implement
- If disagreeing: Explain context/reasoning
- Professional and constructive tone

## Notes

- **Posts responses to PR** - engages with reviewers directly
- **No code changes** - analysis and communication only
- Provides AI-to-AI dialogue about code review
- Helps user make informed decisions about which suggestions to accept
- Documents rationale for rejecting suggestions
- Can be run multiple times as review comments are added

