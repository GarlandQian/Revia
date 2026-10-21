"""Regression checks for false success and unrelated WebKit PID attribution."""
import importlib.util
from pathlib import Path
import unittest
import tempfile

spec = importlib.util.spec_from_file_location("bench", Path(__file__).with_name("bench-long-running.py"))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


class BenchmarkSafetyTests(unittest.TestCase):
    def test_only_live_services_in_target_domain(self):
        domain = """pid/12 = {
 services = {
    45 - com.apple.WebKit.WebContent.ABC-123
    0 - com.apple.WebKit.WebContent
    46 - com.apple.WebKit.GPU.DEF-123
    47 - com.apple.WebKit.Networking.GHI-123
    99 - com.example.Other
 }
 service stubs = {
    999 - com.apple.WebKit.WebContent.UNRELATED
 }
}"""
        self.assertEqual(bench.webkit_services(domain), {"WebContent": [45], "Networking": [47], "GPU": [46]})

    def test_empty_result_never_completes(self):
        self.assertTrue(bench.completion_errors({}, [], 1800, 0))

    def test_short_or_missing_operations_never_complete(self):
        frontend = {"ok": True, "actualDurationSeconds": 30,
                    "totalFileSwitches": 1, "totalViewSwitches": 1,
                    "totalScrolls": 1, "totalJumps": 1}
        samples = [{"webContentPids": [45]}]
        self.assertTrue(bench.completion_errors(frontend, samples, 1800, 0))
        frontend["actualDurationSeconds"] = 1800
        self.assertEqual(bench.completion_errors(frontend, samples, 1800, 0), [])
        frontend["totalScrolls"] = 0
        self.assertTrue(bench.completion_errors(frontend, samples, 1800, 0))
        self.assertTrue(bench.completion_errors(frontend, samples, 1800, -15))

    def test_rebuilt_or_removed_binary_invalidates_run(self):
        with tempfile.TemporaryDirectory() as directory:
            binary = Path(directory) / "revia"
            binary.write_bytes(b"original build")
            digest = bench.binary_fingerprint(binary)
            self.assertTrue(bench.binary_unchanged(binary, digest))
            binary.write_bytes(b"different build")
            self.assertFalse(bench.binary_unchanged(binary, digest))
            binary.unlink()
            self.assertFalse(bench.binary_unchanged(binary, digest))

    def test_physical_footprint_is_not_rss_or_peak(self):
        self.assertEqual(bench.parse_footprint("Physical footprint: 291.0M\nPhysical footprint (peak): 347.4M\n"), 291)
        self.assertEqual(bench.parse_footprint("Physical footprint: 1.5G\n"), 1536)
        with self.assertRaises(RuntimeError):
            bench.parse_footprint("RSS: 500M\nPhysical footprint (peak): 347.4M\n")

    def test_short_run_cannot_claim_steady_memory(self):
        self.assertEqual(bench.analyze_memory([{"elapsedSec": 10}], 0)["steadySampleCount"], 0)
        self.assertEqual(bench.analyze_memory([], 0)["series"], {})


if __name__ == "__main__":
    unittest.main()
