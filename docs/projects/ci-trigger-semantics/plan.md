# Implementation Plan: GitHub Actions CI Trigger Semantics

**Status**: In Progress  
**Complexity**: Simple  
**Estimated Duration**: 1-2 hours  
**Created On Branch**: `design/ci-trigger-semantics`  
**Implementation Branch**: `design/ci-trigger-semantics` (combined with design for simplicity)  
**Related Design**: [Design Document](design.md)  
**Progress**: Phase 1 complete (1/3)

## Context

This plan was created from the design document in the same project folder. The design addresses confusion around GitHub Actions trigger semantics for stacked PR workflows.

**Key Context:**
- Current configuration causes duplicate CI runs (push + pull_request)
- Stacked PRs (feature-B → feature-A) don't get CI
- Simple fix: Change trigger configuration in `.github/workflows/ci.yml`

## Overview

Update the GitHub Actions CI workflow trigger configuration to eliminate duplicate runs and support stacked pull requests. The change is minimal (7 lines in one file) but requires careful testing to verify correct behavior across multiple scenarios.

## Design References

- **Design Document**: [design.md](design.md)
- **Key Design Decision**: Use `push: [main]` + `pull_request:` (no branch filter)
- **Architecture**: Leverage semantic difference between push and pull_request events
- **Acceptance Criteria**: No duplicate runs, stacked PRs work, main branch always gets CI

## Prerequisites

- [x] Design document created and reviewed
- [x] Current CI configuration understood (`.github/workflows/ci.yml` lines 3-9)
- [x] Test repository/branches available for verification
- [x] Understanding of GitHub Actions event semantics

---

## Phase 1: Update CI Configuration

**Objective**: Modify the workflow trigger section to implement the new semantics

### Steps

- [x] Read current CI configuration in `.github/workflows/ci.yml`
- [x] Locate the `on:` section (lines 3-9)
- [x] Replace the trigger configuration with:
  ```yaml
  on:
    push:
      branches:
        - main
    pull_request:
      # No branches specified = all PRs
    schedule:
      - cron: '0 0 * * 1'
  ```
  **Note**: Implemented on `design/ci-trigger-semantics` branch (combining design and implementation for this simple change).
- [x] Verify the `concurrency` section remains unchanged (lines 11-13)
- [x] Save the file

### Quality Checks

- [x] `/fmt false true` - Format code (N/A for YAML)
- [x] Verify YAML syntax is valid (no tabs, proper indentation)
- [x] Review the change carefully

### Files to Modify

- `.github/workflows/ci.yml` (lines 3-9)
  - **Before**:
    ```yaml
    on:
      push:
        branches: ['**']  # Run on all branches
      pull_request:
        branches: [ main, develop ]
    ```
  - **After**:
    ```yaml
    on:
      push:
        branches:
          - main
      pull_request:
        # No branches specified = all PRs
    ```

### Next Phase Prerequisites

- [x] Configuration updated and saved
- [x] Change reviewed for correctness

---

## Phase 2: Testing and Verification

**Objective**: Verify the new configuration works correctly across all scenarios

### Test Plan

The design document outlines 5 critical test scenarios. We'll verify each:

#### Test Case 1: Push to Main
- [ ] Create a test branch from main
- [ ] Make a trivial change (e.g., update README)
- [ ] Create PR targeting main
- [ ] Merge the PR (or push directly to main)
- [ ] **Expected**: ONE CI run via `push` event
- [ ] **Verify**: Check Actions tab shows `push` event type

#### Test Case 2: PR Targeting Main
- [x] Create a feature branch
- [x] Make changes and push
- [x] Create PR targeting main (PR #75)
- [x] **Expected**: ONE CI run via `pull_request` event
- [x] **Verify**: PR shows CI check, Actions shows `pull_request` event type
  **Note**: ✅ VERIFIED! PR #75 triggered `pull_request` event (run ID: 18604374053).
  No duplicate `push` event. Previous `push` event was from old config before PR creation.

#### Test Case 3: Stacked PR (Critical Test)
- [ ] Create feature-A branch and push
- [ ] Create PR: feature-A → main
- [ ] Create feature-B branch based on feature-A
- [ ] Make changes and push feature-B
- [ ] Create PR: feature-B → feature-A (not main!)
- [ ] **Expected**: ONE CI run via `pull_request` event
- [ ] **Verify**: CI runs and tests merge into feature-A
- [ ] **This is the key test**: Confirms no branch filter on pull_request works

#### Test Case 4: Push to PR Branch
- [ ] Use an existing open PR
- [ ] Push a new commit to the PR branch
- [ ] **Expected**: ONE CI run via `pull_request` (synchronized)
- [ ] **Verify**: No duplicate `push` event
- [ ] Check Actions tab shows only `pull_request: synchronize`

#### Test Case 5: Feature Branch Without PR
- [ ] Create a feature branch
- [ ] Push commits without creating a PR
- [ ] **Expected**: NO CI run
- [ ] **Verify**: Actions tab shows no new runs
- [ ] This is intentional behavior

### Quality Checks

- [ ] All 5 test cases pass
- [ ] No duplicate CI runs observed in any scenario
- [ ] CI runtime/costs are reduced (monitor Actions tab)

### Testing Notes

Record results of each test case:

```markdown
Test Case 1 (Push to Main):
- Date tested: 
- Result: 
- Actions run ID: 
- Notes: 

Test Case 2 (PR to Main):
- Date tested: 
- Result: 
- Actions run ID: 
- Notes: 

Test Case 3 (Stacked PR):
- Date tested: 
- PR numbers: feature-A→main #___, feature-B→feature-A #___
- Result: 
- Actions run ID: 
- Notes: 

Test Case 4 (Push to PR Branch):
- Date tested: 
- Result: 
- Actions run ID: 
- Notes: 

Test Case 5 (Feature Branch Without PR):
- Date tested: 
- Result: 
- Notes: No CI run expected
```

### Files to Review

- `.github/workflows/ci.yml` - Updated configuration
- GitHub Actions tab - Verify event types and run counts

### Next Phase Prerequisites

- [ ] All test cases completed successfully
- [ ] No unexpected behavior observed
- [ ] Duplicate run issue confirmed resolved

---

## Phase 3: Documentation and PR

**Objective**: Document the change and create a pull request

### Documentation Updates

- [ ] Review `docs/DEVELOPER.md` for any CI-related documentation
- [ ] Add note about CI trigger behavior:
  - Feature branches require a PR to get CI
  - Stacked PRs are fully supported
  - Main always gets CI on push
- [ ] Update if needed, or note that no changes are required

### Commit and PR Creation

- [ ] Review all changes one final time
- [ ] Commit the CI configuration change:
  ```bash
  /commit "ci: fix trigger semantics for stacked PRs"
  ```
- [ ] Create PR with descriptive title and body
- [ ] PR body should include:
  - Problem statement (duplicate runs, missing stacked PR support)
  - Solution explanation (push: [main], pull_request: all)
  - Test results from Phase 2
  - Reference to design document

### PR Checklist

- [ ] `/review` - Review all changes
- [ ] Create PR targeting main
- [ ] PR title: `ci: fix trigger semantics for stacked PRs`
- [ ] PR body includes:
  - Summary of change (1-3 sentences)
  - Motivation: Duplicate CI runs, missing stacked PR support
  - Solution: Updated trigger configuration
  - Test results: All 5 test cases passed
  - Link to design document
  - Expected impact: 50% reduction in CI runs for PR branches
- [ ] `/pr-ready "ci: fix trigger semantics for stacked PRs"`
- [ ] `/pr-checks` - Monitor CI on the PR itself
- [ ] **Meta-test**: Verify this PR itself gets CI via `pull_request` event

### Final Quality Gates

- [ ] CI passes on the PR (using new configuration)
- [ ] No linter errors
- [ ] Design document committed
- [ ] Implementation plan committed
- [ ] All test cases documented in PR body

---

## Success Criteria

From the design document, the implementation is successful when:

- [x] Design document created and reviewed
- [ ] `.github/workflows/ci.yml` updated with new trigger configuration
- [ ] CI runs exactly once on PR branches (via `pull_request` event)
- [ ] CI runs on pushes to main (via `push` event)
- [ ] Stacked PRs (e.g., feature-B → feature-A) trigger CI
- [ ] No duplicate CI runs observed
- [ ] Developer documentation updated (if needed)
- [ ] Manual testing completed for all 5 scenarios
- [ ] PR merged to main

## Notes and Considerations

### Important Points from Design

1. **`pull_request` tests the merge commit**: The CI runs against `refs/pull/:number/merge`, which is a virtual merge commit between the PR branch and base. This is exactly what you want to test.

2. **Why no branch filter on `pull_request`**: Omitting the `branches` filter means ALL pull requests trigger CI, regardless of the base branch. This enables stacked PRs.

3. **No CI on feature branches without PRs**: This is intentional. Create a PR to get CI feedback.

4. **Scheduled runs**: The `schedule` trigger runs on the default branch (main), so no changes needed there.

### Rollback Plan

If issues are discovered after merge:

```yaml
# Revert to (a safer intermediate):
on:
  push:
    branches:
      - main
  pull_request:
    branches:
      - main
      - develop
  schedule:
    - cron: '0 0 * * 1'
```

This provides some benefit (reduced duplicates on feature branches) while maintaining the old PR branch filter.

### Future Enhancements

From the design document's "Future Work" section:

- Add `workflow_dispatch` for manual CI triggers
- Consider `paths` filters to skip CI for doc-only changes
- Monitor GitHub Actions usage to quantify cost savings
- Document best practices for stacked PR workflows

These can be separate follow-up tasks.

---

## Timeline

**Estimated Total Time**: 1-2 hours

- Phase 1 (Update Configuration): 15 minutes
- Phase 2 (Testing): 45-60 minutes (most time-consuming due to creating test PRs)
- Phase 3 (Documentation & PR): 15-30 minutes

**Note**: Testing takes the most time because you need to create actual PRs and branches to verify behavior.

---

## References

- **Design Document**: [design.md](design.md) - Complete design with detailed explanations
- **GitHub Actions Events**: https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows
- **Avoiding Duplicate Workflows**: https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows#avoiding-duplicate-workflows

---

**Next Steps**:

1. Start with Phase 1: Update the CI configuration
2. Commit the config change
3. Create a PR (which itself becomes a test case!)
4. Run through all test scenarios
5. Document results
6. Merge when all tests pass

