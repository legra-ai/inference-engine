"""Test installed native packages; import errors must fail the test run."""
import unittest
import inference_engine as engine

class NativeTests(unittest.TestCase):
    def test_shared_profile_compiles(self):
        self.assertIn('rdfs-core', engine.profiles())
        result = engine.compile_profile('rdfs-core')
        self.assertEqual(len(result['program']['rules']), 14)
        self.assertTrue(result['exclusions'])

    def test_full_unsupported_profile_fails(self):
        with self.assertRaises(ValueError):
            engine.compile_profile('owl2-rl')

if __name__ == '__main__':
    unittest.main()
