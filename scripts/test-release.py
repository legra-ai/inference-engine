"""Regression tests for alpha identity, complete archives and retry verification."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
def load(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), ROOT / 'scripts' / f'{name}.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

class ReleaseTests(unittest.TestCase):
    def test_alpha_versions_have_one_identity_across_registries(self):
        release = load('release-version')
        self.assertEqual(release.python_version('0.1.0-alpha.12'), '0.1.0a12')
        for invalid in ['0.1.0', '0.1.0-beta.1', '0.1.0-alpha.01', '1.0.0-alpha.1']:
            with self.subTest(invalid=invalid), self.assertRaises(ValueError):
                release.python_version(invalid)

    def test_missing_platform_binary_cannot_be_published(self):
        packages = load('release-packages')
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, 'inventory'):
                packages.inventory(Path(directory), '0.1.0-alpha.1')

    def test_retry_rejects_wrong_archive_revision(self):
        packages = load('release-packages')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'example.tgz'
            source.write_bytes(b'original')
            packages.write_bundle(root / 'bundle.zip', '0.1.0-alpha.1', 'a' * 40, [source])
            with self.assertRaisesRegex(ValueError, 'revision'):
                packages.read_bundle(root / 'bundle.zip', '0.1.0-alpha.1', 'b' * 40, root / 'out')

    def test_retry_rejects_published_npm_with_different_bytes(self):
        packages = load('release-packages')
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / 'example.tgz'
            archive.write_bytes(b'original')
            with patch.object(packages, 'registry_json', return_value={'dist': {'integrity': 'sha512-other'}}):
                with self.assertRaisesRegex(ValueError, 'identity'):
                    packages.npm_exists('0.1.0-alpha.1', archive)

    def test_python_candidate_spelling_and_sdist_are_checked_on_retry(self):
        packages = load('release-packages')
        with tempfile.TemporaryDirectory() as directory:
            archive = Path(directory) / 'inference_engine-0.1.0a1.tar.gz'
            archive.write_bytes(b'original')
            with patch.object(packages, 'registry_json', return_value={'urls': []}) as get:
                self.assertEqual(packages.missing_python('0.1.0-alpha.1', [archive]), [archive])
                self.assertTrue(get.call_args.args[0].endswith('/0.1.0a1/json'))

if __name__ == '__main__':
    unittest.main()
