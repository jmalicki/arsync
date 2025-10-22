/**
 * GitHub Action script to prevent PRs from targeting branches from closed/merged PRs.
 * This helps prevent accidental data loss when merging into stale feature branches.
 * 
 * Logic:
 * 1. Look at the base branch of the current PR (where it's merging INTO)
 * 2. Check if that base branch is the HEAD branch (source) of any OTHER PRs
 * 3. If those other PRs are closed/merged, warn - that branch was already merged elsewhere!
 */

module.exports = async ({github, context, core}) => {
  const currentPR = context.payload.pull_request;
  const baseBranch = currentPR.base.ref;
  
  console.log(`Checking PR #${currentPR.number} targeting base branch: ${baseBranch}`);
  
  // Allow standard branches - these are always safe targets
  const allowedBases = ['main', 'master', 'develop', 'dev'];
  if (allowedBases.includes(baseBranch)) {
    console.log(`✅ Base branch "${baseBranch}" is a standard branch`);
    return;
  }
  
  // Find all PRs where our base branch was the HEAD (source branch being merged)
  // This means "find PRs that were merging FROM this branch"
  const { data: prs } = await github.rest.pulls.list({
    owner: context.repo.owner,
    repo: context.repo.repo,
    state: 'all',
    head: `${context.repo.owner}:${baseBranch}`,
    per_page: 100
  });
  
  // Find closed or merged PRs
  const closedPRs = prs.filter(pr => pr.state === 'closed');
  
  if (closedPRs.length > 0) {
    const prDetails = closedPRs.map(pr => {
      const status = pr.merged_at ? 'merged' : 'closed';
      return `#${pr.number} (${status})`;
    }).join(', ');
    
    core.setFailed(
      `❌ This PR is trying to merge into "${baseBranch}", which was the HEAD branch of closed PR(s): ${prDetails}\n\n` +
      `This means "${baseBranch}" was already merged or closed in another PR, and is likely stale/outdated.\n` +
      `Merging into this branch risks losing your changes if the branch gets deleted!\n\n` +
      `To fix this, change your PR to target "main" instead:\n` +
      `1. Update the base branch on GitHub (edit PR and change base)\n` +
      `   OR\n` +
      `2. Rebase your branch onto main:\n` +
      `   git fetch origin main\n` +
      `   git rebase origin/main\n` +
      `   git push -f origin ${currentPR.head.ref}`
    );
  } else {
    console.log(`✅ Base branch "${baseBranch}" is not from a closed PR`);
  }
};

