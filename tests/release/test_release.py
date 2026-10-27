import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'scripts' / 'release'))
import policy
import prepare
import publish


class VersionTests(unittest.TestCase):
    def propose(self, *messages, current='0.1.0', stable=False):
        return policy.proposal(current, [(str(i), m) for i, m in enumerate(messages)], stable)

    def test_fix_feature_mixed_and_no_release(self):
        self.assertEqual(self.propose('fix: restore playback'), ('0.1.1', False))
        self.assertEqual(self.propose('perf(player): reduce allocations'), ('0.1.1', False))
        self.assertEqual(self.propose('feat: add playlists'), ('0.2.0', False))
        self.assertEqual(self.propose('feat: add playlists', 'fix: restore playback'), ('0.2.0', False))
        self.assertEqual(self.propose('ci: improve builds', 'docs: explain releases', 'refactor: simplify helpers'), (None, False))

    def test_breaking_and_stable(self):
        message = 'feat!: replace configuration\n\nBREAKING CHANGE: migrate the old config first'
        self.assertEqual(self.propose(message), ('0.2.0', True))
        self.assertEqual(self.propose(message, current='1.2.3'), ('2.0.0', True))
        self.assertEqual(self.propose(stable=True), ('1.0.0', True))
        with self.assertRaises(ValueError):
            self.propose('fix!: unexplained break')
        with self.assertRaises(ValueError):
            self.propose('Improve stuff')

    def test_pre_policy_history_is_explicit(self):
        sha = next(iter(policy.LEGACY))
        self.assertEqual(policy.proposal('0.1.0', [(sha, 'original subject')]), ('0.1.1', False))

    def test_only_canonical_stable_versions(self):
        for bad in ('v0.1.0', '0.1.0-rc.1', '0.1.0+test', '01.0.0', '../../bad'):
            with self.assertRaises(ValueError):
                policy.parse_version(bad)


class ApprovalTests(unittest.TestCase):
    def review(self, state='APPROVED', commit='head', user='owner', number=1):
        return {'id': number, 'state': state, 'commit_id': commit, 'user': {'login': user}}

    def test_current_owner_review_only(self):
        self.assertTrue(policy.approved([self.review()], 'owner', 'head'))
        for review in (self.review(commit='old'), self.review(user='agent'), self.review(state='DISMISSED')):
            self.assertFalse(policy.approved([review], 'owner', 'head'))
        self.assertFalse(policy.approved([self.review(), self.review(state='CHANGES_REQUESTED', number=2)], 'owner', 'head'))

    def test_hold_and_approval_are_independent_of_urgent_dispatch(self):
        policy.release_gate([], False, [], 'owner', 'head')
        with self.assertRaises(ValueError):
            policy.release_gate([], True, [], 'owner', 'head')
        with self.assertRaises(ValueError):
            policy.release_gate(['release/hold'], False, [self.review()], 'owner', 'head')
        policy.release_gate([], True, [self.review()], 'owner', 'head')


class GitHistoryTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.old = Path.cwd()
        os.chdir(self.temp.name)
        self.git('init', '-q')
        self.git('config', 'user.email', 'test@example.invalid')
        self.git('config', 'user.name', 'Release Test')
        self.git('config', 'commit.gpgsign', 'false')
        self.git('config', 'tag.gpgsign', 'false')
        Path('updater').mkdir()
        for path, name in [('Cargo.toml', 'mousa'), ('updater/Cargo.toml', 'mousa-launcher')]:
            Path(path).write_text(f'[package]\nname = "{name}"\nversion = "0.1.0"\n')
        Path('Cargo.lock').write_text('version = 4\n[[package]]\nname = "dependency"\nversion = "1.0.0"\n[[package]]\nname = "mousa"\nversion = "0.1.0"\n')
        Path('CHANGELOG.md').write_text('# Changelog\n\n## 0.1.0\nBaseline\n')
        self.commit('chore: baseline')
        self.git('tag', 'v0.1.0')

    def tearDown(self):
        os.chdir(self.old)
        self.temp.cleanup()

    def git(self, *args):
        return subprocess.check_output(['git', *args], text=True, stderr=subprocess.DEVNULL).strip()

    def commit(self, message):
        self.git('add', '.')
        self.git('commit', '-q', '--allow-empty', '-m', message)

    def test_launcher_only_change_is_in_shared_history(self):
        Path('updater/new.rs').write_text('// change\n')
        self.commit('fix(launcher): recover a failed update')
        self.assertEqual(policy.proposal('0.1.0', policy.history('v0.1.0')), ('0.1.1', False))

    def test_version_only_diff_and_dependency_rejection(self):
        for path in ('Cargo.toml', 'updater/Cargo.toml', 'Cargo.lock'):
            p = Path(path)
            p.write_text(p.read_text().replace('0.1.0', '0.1.1'))
        Path('CHANGELOG.md').write_text('# Changelog\n## 0.1.1\nFixes\n\n## 0.1.0\nBaseline\n')
        self.commit('chore(release): v0.1.1')
        policy.validate_release_diff('HEAD^', 'HEAD', policy.FILES)
        self.assertEqual(policy.versions(), '0.1.1')
        p = Path('Cargo.lock')
        p.write_text(p.read_text().replace('1.0.0', '1.0.1'))
        self.commit('chore: unwanted dependency change')
        with self.assertRaisesRegex(ValueError, 'dependencies'):
            policy.validate_release_diff('v0.1.0', 'HEAD', policy.FILES)

    def test_noop_preparation_has_no_mutations(self):
        self.commit('ci: improve automation')
        with patch.object(prepare, 'api', return_value={'tag_name': 'v0.1.0'}), patch.object(prepare, 'pages', return_value=[]):
            prepare.prepare(dry_run=True)
        self.assertEqual(self.git('status', '--porcelain'), '')

    def test_pending_release_blocks_preparation(self):
        with patch.object(prepare, 'api', return_value={'tag_name': 'v0.0.9'}):
            with self.assertRaisesRegex(ValueError, 'unpublished'):
                prepare.prepare(dry_run=True)


class GitHubTests(unittest.TestCase):
    def test_release_commit_must_be_the_exact_bot_merge(self):
        sha = 'a' * 40
        wrong = {'merged_at': 'today', 'merge_commit_sha': 'b' * 40, 'head': {'ref': policy.BRANCH}, 'base': {'ref': 'main'}}
        with patch.object(publish.subprocess, 'run'), patch.object(publish, 'pages', return_value=[wrong]):
            self.assertIsNone(publish.admission(sha))

    def test_already_published_commit_is_a_noop_but_tag_conflicts_fail(self):
        sha = 'a' * 40
        pr = {'merged_at': 'today', 'merge_commit_sha': sha, 'head': {'ref': policy.BRANCH}, 'base': {'ref': 'main'},
              'title': 'chore(release): v0.1.1', 'user': {'login': 'release[bot]'}}
        with patch.dict(os.environ, {'RELEASE_BOT_LOGIN': 'release[bot]'}), patch.object(publish.subprocess, 'run'), patch.object(publish, 'pages', return_value=[pr]), patch.object(publish, 'versions', return_value='0.1.1'), patch.object(publish, 'optional', return_value={'draft': False}), patch.object(publish, 'run', return_value=sha):
            self.assertIsNone(publish.admission(sha))
            with patch.object(publish, 'run', return_value='b' * 40):
                with self.assertRaisesRegex(ValueError, 'another commit'):
                    publish.admission(sha)

    def test_pagination_preserves_all_reviews(self):
        with patch.object(policy, 'api', side_effect=[[{'id': n} for n in range(100)], [{'id': 101}]]) as call:
            self.assertEqual(len(policy.pages('reviews')), 101)
            self.assertIn('page=2', call.call_args[0][0])

    def test_api_failure_does_not_become_missing_release(self):
        with patch.object(publish, 'api', side_effect=RuntimeError('HTTP 403')):
            with self.assertRaises(RuntimeError):
                publish.optional('release')
        with patch.object(publish, 'api', side_effect=RuntimeError('HTTP 404')):
            self.assertIsNone(publish.optional('release'))

    def test_draft_asset_conflicts(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'archive'
            path.write_bytes(b'signed archive')
            good = {'digest': 'sha256:' + publish.digest(path), 'size': path.stat().st_size}
            self.assertTrue(publish.asset_matches(good, path))
            self.assertFalse(publish.asset_matches({**good, 'digest': 'sha256:wrong'}, path))
            self.assertFalse(publish.asset_matches({**good, 'size': 1}, path))

    def test_policy_reports_failure_to_exact_head(self):
        pr = {'base': {'ref': 'main'}, 'head': {'sha': 'a' * 40}}
        with patch.object(policy, 'api', side_effect=[pr, None, {}, None]) as api, patch.object(policy, 'pages', return_value=[]), patch.object(policy.subprocess, 'run'), patch.object(policy, 'validate_pr', side_effect=ValueError('missing approval')):
            with self.assertRaises(ValueError):
                policy.check_pr(42)
            failure = api.call_args
            self.assertTrue(failure[0][0].endswith('a' * 40))
            self.assertEqual(failure[0][2]['state'], 'failure')


class PublicationTests(unittest.TestCase):
    def test_resume_draft_checks_every_asset_before_publishing(self):
        sha, version = 'a' * 40, '0.1.1'
        with tempfile.TemporaryDirectory() as directory:
            dist = Path(directory)
            for target in publish.TARGETS:
                (dist / f'mousa-{version}-{target}.tar.gz').write_bytes(b'archive')
            draft = {'id': 3, 'draft': True, 'body': f'<!-- mousa-release-commit:{sha} -->'}
            assets = {}
            mutations = []
            def api(path, method='GET', data=None):
                mutations.append((method, path, data))
                return {}
            def execute(args, **kwargs):
                if args[:3] == ['gh', 'release', 'upload']:
                    path = Path(args[4])
                    assets[path.name] = {'name': path.name, 'digest': 'sha256:' + publish.digest(path), 'size': path.stat().st_size}
            with patch.dict(os.environ, {'MOUSA_RELEASE_SIGNING_KEY': 'dGVzdC1rZXk='}), patch.object(publish, 'admission', return_value=version), patch.object(publish, 'run', return_value=sha), patch.object(publish, 'optional', side_effect=[{'object': {'sha': sha}}, draft]), patch.object(publish, 'api', side_effect=api), patch.object(publish, 'pages', side_effect=lambda _: list(assets.values())), patch.object(publish.subprocess, 'run', side_effect=execute):
                publish.publish(sha, version, dist)
            self.assertEqual(len(assets), 3)
            self.assertEqual(mutations[-1][0], 'PATCH')
            self.assertFalse(mutations[-1][2]['draft'])

    def test_missing_platform_never_creates_a_tag(self):
        with tempfile.TemporaryDirectory() as directory, patch.object(publish, 'admission', return_value='0.1.1'), patch.object(publish, 'run', return_value='a' * 40), patch.object(publish, 'api') as api:
            with self.assertRaisesRegex(ValueError, 'both platform'):
                publish.publish('a' * 40, '0.1.1', Path(directory))
            api.assert_not_called()


class WorkflowTests(unittest.TestCase):
    def test_publication_requires_both_platforms_and_has_no_tag_trigger(self):
        root = Path(__file__).resolve().parents[2]
        workflow = (root / '.github/workflows/release.yml').read_text()
        self.assertIn('needs: [eligible, build]', workflow)
        self.assertNotIn("tags:", workflow)
        self.assertIn('group: mousa-publication', workflow)
        self.assertIn('cancel-in-progress: false', workflow)
        self.assertIn('ref: ${{ needs.eligible.outputs.commit }}', workflow)
        preparation = (root / '.github/workflows/prepare-release.yml').read_text()
        self.assertIn("cron: '0 17 * * *'", preparation)
        self.assertIn("RELEASE_AUTOMATION_ENABLED == 'true'", preparation)
        for path in (root / '.github/workflows').glob('*.yml'):
            for line in path.read_text().splitlines():
                if 'uses:' in line and '@' in line:
                    ref = line.split('@')[1].split()[0]
                    self.assertRegex(ref, r'^[0-9a-f]{40}$', str(path))


if __name__ == '__main__':
    unittest.main()
