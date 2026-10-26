# Release maintenance

Mousa uses Conventional Commit squash titles, release-plz 0.4.1, and one version
for the app and launcher. GitHub Releases are the distribution source; nothing
is published to crates.io. `v0.1.0` is the immutable baseline. The pre-policy
gapless audio fix (included in the bootstrap PR squash commit) is explicitly classified as a fix in the policy and
release-plz configuration; unknown unclassified commits fail preparation.

## Normal operation

Agents open PRs and enable squash auto-merge. Required checks are the two
platform builds and the trusted `Release policy` commit status. PR builds run
without bot or signing credentials. The policy workflow reads PR files as data
and always checks out main's policy implementation.

At 17:00 UTC each day (10:00 PDT / 09:00 PST; GitHub schedules may be delayed),
Prepare release checks for releasable commits since the latest published tag.
It runs release-plz to generate the manifests and changelog, explicitly aligns
both package versions, verifies that dependencies did not change, and opens or
updates the single `release-plz/mousa` PR. No dependency upgrade is performed.
CI maintenance and documentation alone do not create a release.

`fix`/`perf` increment patch, `feat` increments minor, and the highest increment
wins. Breaking changes increment minor before 1.0 and major after 1.0. They need
an explanatory `BREAKING CHANGE:` footer. Breaking releases and the first 1.0.0
release require an APPROVED review from the repository owner on the current PR
head commit. A later change or dismissed/request-changes review invalidates it.
The `stable` dispatch input proposes 1.0.0 but never bypasses that approval.

Add `release/hold` to pause merging or publication. If added just as merging
finishes, publication also checks the label and stops; remove it and retry the
Release workflow for that exact merge SHA. Review and label events refresh the
policy gate without recompiling the application.

For an urgent release, dispatch Prepare release with `dry_run=false`. This only
bypasses the daily schedule. It never bypasses policy, checks, or owner approval.
The default manual invocation is a dry run: it uploads a preview patch and does
not create a branch, commit, PR, tag, or release.

## Publication and recovery

After a release PR merges, Release admits its exact squash commit, tests and
builds both operating systems, then signs the archives from that run. It creates
one tag, uploads both archives and SHA256SUMS to a draft, verifies asset hashes,
rechecks approval/hold state, and publishes the immutable release. There is no
tag-triggered second build. Ordinary merges do not run release builds.

Publication runs are serialized. If a build fails, no tag or release is created.
If publication fails, use GitHub's **Re-run failed jobs** so the successful build
artifacts are reused. A manual Release dispatch accepts the exact merged release
commit for recovery. A rebuild can produce different bytes: if a draft already
contains a different asset, publication stops rather than overwriting it. Inspect
the original run and recover its artifacts; do not delete/recreate a published
release or move its tag. An existing published release at the same commit is a
no-op. Preparation refuses to proceed while a merged version or draft remains
unpublished. Workflow failures remain visible in GitHub Actions.

The updater archive names, signature key, supported targets, and startup rollback
behavior are unchanged. Rollback still requires backward-compatible user data.

## GitHub setup and rollout

1. Create a GitHub App installed **only on jivanf/mousa**, with Contents and Pull
   requests read/write and implicit Metadata read. Disable webhooks; no callback
   or user authorization is needed. Store its ID in `RELEASE_APP_ID`, its bot login
   (`<app-slug>[bot]`) in `RELEASE_BOT_LOGIN`, and PEM key in the repository secret
   `RELEASE_APP_PRIVATE_KEY`. Never place the key in the repository or logs.
2. Keep `RELEASE_AUTOMATION_ENABLED=false` during setup. Keep the existing
   `MOUSA_RELEASE_SIGNING_KEY` available only to the trusted publication job.
3. Enable squash merging and auto-merge. Configure squash messages to preserve
   the PR title and body, including breaking-change footers. Disable merge and
   rebase merging. Protect main with PRs and required `Build / Linux`,
   `Build / Windows`, and `Release policy` checks, strict up-to-date validation,
   no required human review for routine PRs, and no force pushes or deletion.
4. Bootstrap this change through its implementation PR. Before the trusted policy
   workflow exists on main, run that policy locally against the implementation
   PR and post its status using the maintainer's authenticated CLI. Establish the
   required check names before enabling branch protection.
5. Run a dry preparation preview. Dispatch Prepare release with `probe=true` to
   create a temporary documentation PR using the App. Its intentionally invalid
   title blocks merging while the probe verifies both platform checks start and
   auto-merge can be requested. The probe closes the PR and removes its branch. Only then set `RELEASE_AUTOMATION_ENABLED=true`.

Python policy tests use only the standard library and run on Windows and Linux.
Run `python -m unittest discover -s tests/release -v` and `actionlint` when changing
the automation. Local agent commits still require the post-commit installation
skill; GitHub's ephemeral bot checkout is validated by the platform installer
and build jobs, not by accessing a developer's workstation.
