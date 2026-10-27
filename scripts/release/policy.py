"""Release decisions shared by trusted CI, preparation, and publication (Python 3.11+)."""
import json
import os
import re
import subprocess
import sys
import tomllib

BRANCH = "release-plz/mousa"
CONTEXT = "Release policy"
FILES = {"Cargo.toml", "updater/Cargo.toml", "Cargo.lock", "CHANGELOG.md"}
CONVENTIONAL = re.compile(r"^(feat|fix|perf|docs|test|ci|build|chore|refactor|style|revert)(?:\([^\r\n()]+\))?(!)?: \S[^\r\n]*$")
VERSION = re.compile(r"(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)")
# Explicit classification of the only pre-policy, post-v0.1.0 non-conventional commit.
LEGACY = {"f3d5c929a8db6743b38eb30beb025136b5c98e5e": "fix: keep queued audio gapless with native mpv playlist handoffs"}


def require(condition, message):
    if not condition:
        raise ValueError(message)


def run(*args):
    return subprocess.check_output(args, text=True).strip()


def api(path, method="GET", data=None):
    args = ["gh", "api", path, "--method", method]
    if data is not None:
        args += ["--input", "-"]
    result = subprocess.run(args, input=json.dumps(data) if data is not None else None,
                            text=True, capture_output=True)
    if result.returncode:
        raise RuntimeError(f"GitHub API {method} {path}: {result.stderr.strip()}")
    return json.loads(result.stdout) if result.stdout.strip() else None


def pages(path):
    result = []
    for page in range(1, 1000):
        values = api(f"{path}{'&' if '?' in path else '?'}per_page=100&page={page}")
        result.extend(values)
        if len(values) < 100:
            return result
    raise ValueError("GitHub pagination limit exceeded")


def repo():
    return os.environ.get("GITHUB_REPOSITORY", "jivanf/mousa")


def endpoint(path):
    return f"repos/{repo()}/{path}"


def parse_version(value):
    require(VERSION.fullmatch(value), f"Invalid stable version: {value}")
    return tuple(map(int, value.split('.')))


def classify(title, body=""):
    match = CONVENTIONAL.fullmatch(title)
    require(match, f"Use a Conventional Commit title: {title}")
    footer = re.search(r"(?m)^BREAKING CHANGE: \S.+", body) is not None
    require(not match[2] or footer, "Breaking titles need an explanatory BREAKING CHANGE: footer")
    breaking = bool(match[2] or footer)
    kind = "major" if breaking else "minor" if match[1] == "feat" else "patch" if match[1] in ("fix", "perf") else None
    return kind, breaking


def proposal(current, commits, stable=False):
    major, minor, patch = parse_version(current)
    ranks = {None: 0, "patch": 1, "minor": 2, "major": 3}
    level, breaking = None, False
    for sha, message in commits:
        message = LEGACY.get(sha, message)
        title, _, body = message.partition('\n')
        kind, incompatible = classify(title, body)
        if ranks[kind] > ranks[level]:
            level = kind
        breaking |= incompatible
    if stable:
        require(major == 0, "The stable milestone is only available before 1.0")
        return "1.0.0", True
    if level is None:
        return None, False
    if level == "major":
        next_version = (major + 1, 0, 0) if major else (0, minor + 1, 0)
    elif level == "minor":
        next_version = (major, minor + 1, 0)
    else:
        next_version = (major, minor, patch + 1)
    return '.'.join(map(str, next_version)), breaking


def history(tag, ref="HEAD"):
    require(re.fullmatch(r"v\d+\.\d+\.\d+", tag), "Unexpected release tag")
    subprocess.run(["git", "merge-base", "--is-ancestor", tag, ref], check=True)
    shas = run("git", "rev-list", "--first-parent", f"{tag}..{ref}").splitlines()
    return [(sha, run("git", "show", "-s", "--format=%B", sha)) for sha in shas]


def contents(ref, path):
    return run("git", "show", f"{ref}:{path}")


def versions(ref="HEAD"):
    result = [tomllib.loads(contents(ref, p))["package"]["version"] for p in ("Cargo.toml", "updater/Cargo.toml")]
    require(result[0] == result[1], "Application and launcher versions must match")
    parse_version(result[0])
    return result[0]


def approved(reviews, owner, head):
    owner_reviews = [r for r in reviews if r["user"]["login"] == owner and r["state"] in ("APPROVED", "CHANGES_REQUESTED", "DISMISSED")]
    if not owner_reviews:
        return False
    latest = max(owner_reviews, key=lambda r: r["id"])
    return latest["state"] == "APPROVED" and latest["commit_id"] == head


def release_gate(labels, needs_approval, reviews, owner, head):
    require("release/hold" not in labels, "Release held by release/hold label")
    require(not needs_approval or approved(reviews, owner, head), "Owner approval of the current release PR revision is required")


def validate_release_diff(base, head, changed):
    require(set(changed) <= FILES, "Release PR may only change manifests, lockfile, and changelog")
    require("CHANGELOG.md" in changed, "Release PR must contain release notes")
    for path in ("Cargo.toml", "updater/Cargo.toml"):
        old, new = [tomllib.loads(contents(ref, path)) for ref in (base, head)]
        old["package"].pop("version")
        new["package"].pop("version")
        require(old == new, f"Release PR changed more than the version in {path}")
    def normalized_lock(ref):
        value = tomllib.loads(contents(ref, "Cargo.lock"))
        for package in value["package"]:
            if package["name"] in ("mousa", "mousa-launcher") and "source" not in package:
                package["version"] = "WORKSPACE"
        return value
    require(normalized_lock(base) == normalized_lock(head), "Release preparation must not update dependencies")
    old_notes = contents(base, "CHANGELOG.md")
    history_start = old_notes.find('\n## ')
    require(history_start >= 0 and old_notes[history_start:].strip() in contents(head, "CHANGELOG.md"),
            "Release preparation must preserve previous changelog entries")


def validate_pr(pr, reviews, latest):
    title, body = pr["title"], pr.get("body") or ""
    classify(title, body)
    base, head = pr["base"]["sha"], pr["head"]["sha"]
    new_version = versions(head)
    if pr["head"]["ref"] != BRANCH:
        require(new_version == versions(base), "Only the release bot may bump versions")
        require(not title.startswith("chore(release):"), "The release title is reserved for automation")
        return
    bot = os.environ.get("RELEASE_BOT_LOGIN")
    require(bot and pr["user"]["login"] == bot, "Release PR must be authored by the configured GitHub App")
    require(pr["head"]["repo"]["full_name"] == repo(), "Release branch must belong to this repository")
    current = latest["tag_name"].removeprefix('v')
    require(versions(base) == current, "Finish publishing the pending version before preparing another release")
    expected, needs_approval = proposal(current, history(latest["tag_name"], base), stable=new_version == "1.0.0" and parse_version(current)[0] == 0)
    require(expected is not None and new_version == expected, f"Expected release version {expected}, found {new_version}")
    require(title == f"chore(release): v{expected}", "Release PR title does not match the version")
    changed = run("git", "diff", "--name-only", base, head).splitlines()
    validate_release_diff(base, head, changed)
    release_gate([x["name"] for x in pr["labels"]], needs_approval, reviews, repo().split('/')[0], head)


def check_pr(number):
    pr = api(endpoint(f"pulls/{number}"))
    require(pr["base"]["ref"] == "main", "Release policy only supports main")
    head = pr["head"]["sha"]
    status_url = endpoint(f"statuses/{head}")
    target = f"https://github.com/{repo()}/actions/runs/{os.environ.get('GITHUB_RUN_ID', '')}"
    def status(state, description):
        api(status_url, "POST", {"state": state, "context": CONTEXT, "description": description[:140], "target_url": target})
    status("pending", "Checking release policy")
    try:
        subprocess.run(["git", "fetch", "origin", f"refs/pull/{number}/head"], check=True)
        subprocess.run(["git", "fetch", "origin", "main", "--tags"], check=True)
        validate_pr(pr, pages(endpoint(f"pulls/{number}/reviews")), api(endpoint("releases/latest")))
        status("success", "Release policy passed")
    except Exception as error:
        status("failure", str(error))
        raise


if __name__ == "__main__":
    try:
        check_pr(int(sys.argv[1]))
    except Exception as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
