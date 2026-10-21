"""Regression checks for false success and unrelated WebKit PID attribution."""
import importlib.util
from pathlib import Path
import unittest
import tempfile
from unittest import mock

spec = importlib.util.spec_from_file_location("bench", Path(__file__).with_name("bench-long-running.py"))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)
fixture_spec = importlib.util.spec_from_file_location("cold_fixture", Path(__file__).parent / "fixtures/setup-cold-jump.py")
cold_fixture = importlib.util.module_from_spec(fixture_spec)
fixture_spec.loader.exec_module(cold_fixture)


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

    def test_cold_fixture_ignores_inherited_git_targets(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "fixture"
            inherited = {"GIT_DIR": "/unrelated/.git", "GIT_WORK_TREE": "/unrelated",
                         "GIT_INDEX_FILE": "/unrelated/index", "GIT_CONFIG_COUNT": "1"}
            with mock.patch.dict(cold_fixture.os.environ, inherited), \
                    mock.patch("sys.argv", ["setup-cold-jump.py", str(root)]), \
                    mock.patch.object(cold_fixture.subprocess, "run") as run, \
                    mock.patch("builtins.print"):
                cold_fixture.main()
            self.assertEqual(len(run.call_args_list), 3)
            for call in run.call_args_list:
                self.assertEqual(call.args[0][1:3], ["-C", str(root.resolve())])
                self.assertFalse(set(inherited).intersection(call.kwargs["env"]))
            self.assertEqual(len((root / "cold.ts").read_text().splitlines()), 30_000)

    def test_cold_fixture_refuses_existing_directory(self):
        with tempfile.TemporaryDirectory() as directory:
            with mock.patch("sys.argv", ["setup-cold-jump.py", directory]), \
                    mock.patch.object(cold_fixture.subprocess, "run") as run:
                with self.assertRaises(FileExistsError):
                    cold_fixture.main()
            run.assert_not_called()

    def test_short_run_cannot_claim_steady_memory(self):
        self.assertEqual(bench.analyze_memory([{"elapsedSec": 10}], 0)["steadySampleCount"], 0)
        self.assertEqual(bench.analyze_memory([], 0)["series"], {})


if __name__ == "__main__":
    unittest.main()
