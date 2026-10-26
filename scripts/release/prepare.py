"""Prepare one release-plz change set; GitHub App owns the single release PR."""
import argparse
import os
import subprocess
import tomllib
from pathlib import Path
from policy import BRANCH, FILES, api, endpoint, history, pages, proposal, repo, require, run, versions


def prepare(dry_run=False, stable=False):
    require(not run("git", "status", "--porcelain"), "Preparation requires a clean checkout")
    latest = api(endpoint("releases/latest"))
    current = latest["tag_name"].removeprefix('v')
    require(versions() == current, "A merged release is unpublished; retry its publication first")
    drafts = [r for r in pages(endpoint("releases")) if r["draft"] and not r["prerelease"]]
    require(not drafts, "A draft release is pending; finish its publication first")
    expected, needs_approval = proposal(current, history(latest["tag_name"]), stable)
    if expected is None:
        print("No releasable changes; no release PR created.")
        return
    print(f"Preparing {expected}; owner approval required: {needs_approval}")
    # release-plz supplies the changelog and manifest updates. Normalize both package
    # versions explicitly: version_group alone can leave an unchanged crate behind.
    old_notes = Path("CHANGELOG.md").read_text()
    subprocess.run(["release-plz", "update"], check=True)
    unchanged_notes = Path("CHANGELOG.md").read_text() == old_notes
    require(not unchanged_notes or stable, "release-plz did not generate notes for the releasable changes")
    subprocess.run(["release-plz", "set-version", f"mousa@{expected}", f"mousa-launcher@{expected}"], check=True)
    if unchanged_notes and stable:
        # Declaring stability can be intentional even without a new feature/fix.
        Path("CHANGELOG.md").write_text(old_notes.replace('\n## ', '\n## [1.0.0]\n\n- First stable release.\n\n## ', 1))
    subprocess.run(["cargo", "metadata", "--format-version", "1", "--no-deps"], stdout=subprocess.DEVNULL, check=True)
    actual = [tomllib.loads(Path(p).read_text())["package"]["version"] for p in ("Cargo.toml", "updater/Cargo.toml")]
    require(actual == [expected, expected], "release-plz did not synchronize package versions")
    require(f"{expected}" in Path("CHANGELOG.md").read_text(), "Missing generated release notes")
    require(old_notes[old_notes.index('\n## '):].strip() in Path("CHANGELOG.md").read_text(), "Previous changelog entries were modified")
    changed = set(run("git", "diff", "--name-only").splitlines())
    require(changed <= FILES, "Unexpected files changed during release preparation")
    if dry_run:
        Path(os.environ.get("RUNNER_TEMP", "/tmp"), "mousa-release-preview.patch").write_text(run("git", "diff") + '\n')
        print("Dry run complete; no branch, PR, tag, or release was created.")
        return
    bot = os.environ["RELEASE_BOT_LOGIN"]
    prs = pages(endpoint(f"pulls?state=open&head={repo().split('/')[0]}:{BRANCH}"))
    require(len(prs) <= 1, "Multiple release PRs exist")
    if prs:
        require(prs[0]["user"]["login"] == bot, "Existing release PR is not owned by the release App")
        subprocess.run(["gh", "pr", "merge", str(prs[0]["number"]), "--disable-auto"], check=True)
    remote = run("git", "ls-remote", "origin", f"refs/heads/{BRANCH}")
    old = remote.split()[0] if remote else ""
    subprocess.run(["git", "switch", "-C", BRANCH], check=True)
    subprocess.run(["git", "config", "user.name", bot], check=True)
    subprocess.run(["git", "config", "user.email", f"{bot}@users.noreply.github.com"], check=True)
    subprocess.run(["git", "add", *sorted(FILES)], check=True)
    title = f"chore(release): v{expected}"
    subprocess.run(["git", "commit", "-m", title], check=True)
    # Fail locally before submitting the bot's changes for platform CI.
    from policy import validate_release_diff
    validate_release_diff("HEAD^", "HEAD", run("git", "diff", "--name-only", "HEAD^", "HEAD").splitlines())
    subprocess.run(["git", "push", f"--force-with-lease=refs/heads/{BRANCH}:{old}", "origin", f"HEAD:refs/heads/{BRANCH}"], check=True)
    body = (f"Release Mousa and its launcher as **{expected}**.\n\n"
            "Generated with release-plz. Both platform checks and release policy must pass.\n\n"
            + ("**Owner approval of this exact revision is required** for a breaking change or the 1.0 milestone.\n\n" if needs_approval else "Routine release; automatic merging is enabled.\n\n")
            + "Add `release/hold` to pause this release. Changes to this PR invalidate previous approval.\n")
    if prs:
        pr = api(endpoint(f"pulls/{prs[0]['number']}"), "PATCH", {"title": title, "body": body})
    else:
        pr = api(endpoint("pulls"), "POST", {"head": BRANCH, "base": "main", "title": title, "body": body})
    api(endpoint(f"issues/{pr['number']}/labels"), "POST", {"labels": ["release"]})
    subprocess.run(["gh", "pr", "merge", str(pr["number"]), "--auto", "--squash", "--match-head-commit", run("git", "rev-parse", "HEAD")], check=True)
    print(pr["html_url"])


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--dry-run", action="store_true")
    parser.add_argument("--stable", action="store_true")
    args = parser.parse_args()
    prepare(args.dry_run, args.stable)
