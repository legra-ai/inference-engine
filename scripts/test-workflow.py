"""Publication requires all native artifacts and an explicit alpha channel."""
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[1]
class WorkflowTests(unittest.TestCase):
    def test_one_pipeline_binds_all_jobs_to_the_tested_revision(self):
        workflow = (ROOT / '.github/workflows/ci.yml').read_text()
        self.assertIn("version: ${{ steps.manifest.outputs.version }}", workflow)
        self.assertIn("needs: [prepare, verify, sdist, native]", workflow)
        self.assertIn("needs: [prepare, test]", workflow)
        self.assertIn("needs.prepare.outputs.revision", workflow)
        self.assertIn("--tag alpha", workflow)
        self.assertIn("prerelease: true", workflow)
        self.assertIn("publish-tested-crate@", workflow)
        self.assertIn("gh-action-pypi-publish@", workflow)
        self.assertNotIn('--skip-existing', workflow)
        self.assertNotIn('--clobber', workflow)
        for target in ['ubuntu-24.04-arm', 'macos-15-intel', 'macos-15', 'windows-latest']:
            self.assertIn(target, workflow)

if __name__ == '__main__':
    unittest.main()
