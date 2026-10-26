"""Opt-in integration tests against the pinned real tool, with temporary Git histories."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import tomllib
import unittest

ROOT = Path(__file__).resolve().parents[2]
TOOL = os.environ.get('MOUSA_TEST_RELEASE_PLZ')


@unittest.skipUnless(TOOL, 'set MOUSA_TEST_RELEASE_PLZ to the pinned binary for tool integration tests')
class ReleasePlzTests(unittest.TestCase):
    def scenario(self, changes):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            def run(*args):
                return subprocess.run(args, cwd=root, check=True, capture_output=True, text=True)
            run('git', 'init', '-q')
            for key, value in [('user.name', 'Test'), ('user.email', 'test@example.invalid'), ('commit.gpgsign', 'false'), ('tag.gpgsign', 'false')]:
                run('git', 'config', key, value)
            run('git', 'remote', 'add', 'origin', 'https://github.com/jivanf/mousa.git')
            for base, name in [(root, 'mousa'), (root / 'updater', 'mousa-launcher')]:
                (base / 'src').mkdir(parents=True)
                (base / 'src/main.rs').write_text('fn main() {}\n')
                (base / 'Cargo.toml').write_text(f'[package]\nname = "{name}"\nversion = "0.1.0"\nedition = "2021"\n')
            with (root / 'Cargo.toml').open('a') as stream:
                stream.write('\n[workspace]\nmembers = ["updater"]\nresolver = "2"\n')
            shutil.copy2(ROOT / 'release-plz.toml', root / 'release-plz.toml')
            (root / 'CHANGELOG.md').write_text('# Changelog\n\n## [0.1.0]\n\n- Baseline.\n')
            run('cargo', 'generate-lockfile', '--offline')
            run('git', 'add', '.')
            run('git', 'commit', '-qm', 'chore: baseline')
            run('git', 'tag', 'v0.1.0')
            for index, (path, message) in enumerate(changes):
                with (root / path).open('a') as stream:
                    stream.write(f'// change {index}\n')
                run('git', 'add', '.')
                run('git', 'commit', '-qm', message)
            result = run(TOOL, 'update')
            notes = (root / 'CHANGELOG.md').read_text()
            self.assertIn('## [0.1.0]\n\n- Baseline.', notes)
            return notes, [tomllib.loads((root / p).read_text())['package']['version'] for p in ('Cargo.toml', 'updater/Cargo.toml')]

    def test_launcher_only_has_shared_changelog(self):
        notes, versions = self.scenario([('updater/src/main.rs', 'fix: recover launcher startup')])
        self.assertIn('recover launcher startup', notes)
        self.assertEqual(versions, ['0.1.1', '0.1.1'])

    def test_feature_beats_fix(self):
        notes, versions = self.scenario([('src/main.rs', 'feat: add playlists'), ('updater/src/main.rs', 'fix: recover launcher startup')])
        self.assertIn('add playlists', notes)
        self.assertIn('recover launcher startup', notes)
        self.assertEqual(versions, ['0.2.0', '0.2.0'])

    def test_docs_alone_do_not_release(self):
        notes, versions = self.scenario([('src/main.rs', 'docs: explain main')])
        self.assertNotIn('0.1.1', notes)
        self.assertEqual(versions, ['0.1.0', '0.1.0'])

    def test_bootstrap_squash_preserves_audio_fix(self):
        notes, versions = self.scenario([('src/main.rs', 'ci: automate agent-driven releases (#1)\n\nBootstrap release automation and include the existing audio fix.')])
        self.assertIn('keep queued audio gapless', notes)
        self.assertEqual(versions, ['0.1.1', '0.1.1'])
