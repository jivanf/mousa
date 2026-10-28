"""Admit an exact merged release commit, then publish verified immutable assets."""
import argparse
import hashlib
import os
import re
import subprocess
import tempfile
from pathlib import Path
from policy import BRANCH, api, endpoint, history, pages, proposal, release_gate, repo, require, run, validate_release_diff, versions

TARGETS = ("x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc")


def optional(path):
    try:
        return api(path)
    except RuntimeError as error:
        if "HTTP 404" in str(error):
            return None
        raise


def admission(commit):
    require(re.fullmatch(r"[0-9a-f]{40}", commit), "An exact commit SHA is required")
    subprocess.run(["git", "merge-base", "--is-ancestor", commit, "origin/main"], check=True)
    prs = pages(endpoint(f"commits/{commit}/pulls"))
    matches = [p for p in prs if p.get("merged_at") and p.get("merge_commit_sha") == commit and p["head"]["ref"] == BRANCH and p["base"]["ref"] == "main"]
    if not matches:
        return None
    require(len(matches) == 1, "Ambiguous release PR")
    pr = matches[0]
    require(os.environ.get("RELEASE_BOT_LOGIN") and pr["user"]["login"] == os.environ["RELEASE_BOT_LOGIN"], "Release commit must come from the configured App")
    version = versions(commit)
    require(pr["title"] == f"chore(release): v{version}", "Unexpected release title")
    existing = optional(endpoint(f"releases/tags/v{version}"))
    if existing and not existing["draft"]:
        require(run("git", "rev-parse", f"v{version}^{{commit}}") == commit, "Published tag points to another commit")
        print(f"v{version} is already published; nothing to do.")
        return None
    latest = api(endpoint("releases/latest"))
    parent = run("git", "rev-parse", f"{commit}^")
    require(versions(parent) == latest["tag_name"].removeprefix('v'), "Earlier release must finish publication first")
    expected, needs_approval = proposal(versions(parent), history(latest["tag_name"], parent), stable=version == "1.0.0" and versions(parent).startswith('0.'))
    require(expected == version, "Release version does not match included changes")
    validate_release_diff(parent, commit, run("git", "diff", "--name-only", parent, commit).splitlines())
    release_gate([x["name"] for x in pr["labels"]], needs_approval,
                 pages(endpoint(f"pulls/{pr['number']}/reviews")), repo().split('/')[0], pr["head"]["sha"])
    return version


def digest(path):
    with path.open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def asset_matches(asset, path):
    return asset.get('digest') == 'sha256:' + digest(path) and asset['size'] == path.stat().st_size


def publish(commit, version, directory):
    require(admission(commit) == version, "Publication is not eligible")
    require(run("git", "rev-parse", "HEAD") == commit, "Checkout is not the admitted release commit")
    names = [f"mousa-{version}-{target}.tar.gz" for target in TARGETS]
    require(sorted(p.name for p in directory.iterdir()) == sorted(names), "Expected exactly both platform archives")
    # The private signing key exists only in an ephemeral file inside this job.
    import base64
    private = base64.b64decode(os.environ.pop("MOUSA_RELEASE_SIGNING_KEY"), validate=True)
    with tempfile.TemporaryDirectory() as temporary:
        key, public = Path(temporary) / 'key', Path(temporary) / 'public'
        descriptor = os.open(key, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        with os.fdopen(descriptor, 'wb') as stream:
            stream.write(private)
        del private
        public.write_bytes(bytes.fromhex(Path("updater/release-public-key.hex").read_text().strip()))
        try:
            for name in names:
                subprocess.run(["zipsign", "sign", "tar", str(directory / name), str(key)], check=True)
                subprocess.run(["zipsign", "verify", "tar", str(directory / name), str(public)], check=True)
        finally:
            key.unlink(missing_ok=True)
    (directory / 'SHA256SUMS').write_text(''.join(f"{digest(directory / name)}  {name}\n" for name in names))
    tag = f"v{version}"
    ref = optional(endpoint(f"git/ref/tags/{tag}"))
    if ref:
        subprocess.run(["git", "fetch", "origin", f"refs/tags/{tag}"], check=True)
        require(run("git", "rev-parse", "FETCH_HEAD^{commit}") == commit, "Existing tag points to another commit")
    else:
        api(endpoint('git/refs'), 'POST', {'ref': f'refs/tags/{tag}', 'sha': commit})
    marker = f"<!-- mousa-release-commit:{commit} -->"
    release = optional(endpoint(f"releases/tags/{tag}"))
    if release:
        require(release['draft'] and marker in (release.get('body') or ''), "Existing release is not this commit's draft")
    else:
        changelog = Path('CHANGELOG.md').read_text()
        section = re.search(rf"(?ms)^## .*?{re.escape(version)}[^\n]*\n(.*?)(?=^## |\Z)", changelog)
        require(section, "Changelog has no release section")
        release = api(endpoint('releases'), 'POST', {'tag_name': tag, 'target_commitish': commit,
                      'name': f'Mousa {tag}', 'body': marker + '\n\n' + section[1].strip(), 'draft': True})
    assets = {a['name']: a for a in pages(endpoint(f"releases/{release['id']}/assets"))}
    expected = names + ['SHA256SUMS']
    require(set(assets) <= set(expected), "Draft contains unexpected assets")
    for name in expected:
        path = directory / name
        if name in assets:
            require(asset_matches(assets[name], path), f"Draft asset conflict: {name}; never overwrite it")
        else:
            subprocess.run(['gh', 'release', 'upload', tag, str(path)], check=True)
    assets = {a['name']: a for a in pages(endpoint(f"releases/{release['id']}/assets"))}
    require(set(assets) == set(expected), "Draft assets are incomplete")
    require(all(asset_matches(assets[name], directory / name) for name in expected), "Uploaded asset digest mismatch")
    # Recheck the owner gate and hold label immediately before making it public.
    require(admission(commit) == version, "Release eligibility changed during publication")
    api(endpoint(f"releases/{release['id']}"), 'PATCH', {'draft': False, 'make_latest': 'true'})


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('command', choices=['admit', 'publish'])
    parser.add_argument('commit')
    parser.add_argument('--version')
    parser.add_argument('--directory', type=Path, default=Path('dist'))
    args = parser.parse_args()
    if args.command == 'admit':
        version = admission(args.commit)
        if output := os.environ.get('GITHUB_OUTPUT'):
            with open(output, 'a') as stream:
                stream.write(f"version={version or ''}\ncommit={args.commit}\n")
    else:
        publish(args.commit, args.version, args.directory)
