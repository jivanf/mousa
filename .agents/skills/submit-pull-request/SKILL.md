---
name: submit-pull-request
description: Create or update an approved Mousa pull request with a Conventional Commit title and required squash auto-merge. Use whenever repository work is ready to submit to GitHub.
---

# Submit Pull Request

Do not submit until the user has approved the completed task, unless the user
explicitly preapproved PR creation for that task. Approval is task-specific.

Before submission, ensure the task branch is committed, verified, and clean.
Choose a Conventional Commit PR title that reflects user impact. Put the summary,
verification, compatibility notes, and any `BREAKING CHANGE:` footer in a body
file.

You must create or update the PR by running:

```text
.agents/skills/submit-pull-request/scripts/pr --title "<title>" --body-file <path>
```

Do not substitute direct `gh pr create`, `gh pr edit`, or `gh pr merge` commands.
The script safely updates the current task branch with force-with-lease when a
rebase requires it, creates or updates its open PR, enables squash auto-merge,
and verifies that auto-merge is active.

After submission, monitor the required Linux, Windows, and Release policy checks.
Address failures and revisions on the same branch and PR. Do not bypass checks or
push directly to `main`. Verify the actual merged state before reporting success.
