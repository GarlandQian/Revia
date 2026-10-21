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

VMMAP_SAMPLE = """Physical footprint: 273.8M
Physical footprint (peak): 376.8M
REGION TYPE SIZE SIZE SIZE
WebKit Malloc 648.8M 369.1M 302.9M

 VIRTUAL RESIDENT DIRTY SWAPPED ALLOCATION BYTES DIRTY+SWAP REGION
MALLOC ZONE SIZE SIZE SIZE SIZE COUNT ALLOCATED FRAG SIZE % FRAG COUNT
=========== ======= ========= ========= ========= ========= ========= ========= ====== ======
WebKit Malloc_0x10200e678 1.5G 383.2M 309.8M 22.3M 725883 164.3M 167.8M 51% 32
WebKit Using System Malloc_0x100bd0000 0K 0K 0K 0K 0 0K 0K 0% 0
=========== ======= ========= ========= ========= ========= ========= ========= ====== ======
TOTAL 1.6G 387.4M 313.5M 23.0M 745593 167.0M 169.4M 51% 471
"""


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

    def test_vmmap_reads_allocated_column_and_footprint_from_one_snapshot(self):
        with mock.patch.object(bench.subprocess, "check_output", return_value=VMMAP_SAMPLE) as command:
            result = bench.webcontent_memory(45)
        self.assertEqual(result, {"pid": 45, "footprintMb": 273.8, "webkitMallocAllocatedMb": 164.3})
        command.assert_called_once_with(["vmmap", "-summary", "45"], text=True, timeout=10)
        for value, expected in (("1048576", 1), ("512K", 0.5), ("1.5G", 1536)):
            with self.subTest(value=value):
                self.assertEqual(bench.parse_webkit_allocated(VMMAP_SAMPLE.replace("164.3M", value)), expected)

    def test_vmmap_refuses_ambiguous_or_changed_allocation_columns(self):
        invalid = (
            VMMAP_SAMPLE.replace("BYTES DIRTY+SWAP", "PEAK DIRTY+SWAP"),
            VMMAP_SAMPLE.replace("WebKit Malloc_0x10200e678", "OtherMalloc_0x10200e678"),
            VMMAP_SAMPLE.replace("725883 164.3M", "725883"),
            VMMAP_SAMPLE.replace("164.3M", "164.3Q"),
            VMMAP_SAMPLE.replace("51% 32", "51% 32 99"),
            VMMAP_SAMPLE.replace("WebKit Using System Malloc_0x100bd0000", "WebKit Malloc_0x10200e678"),
        )
        for output in invalid:
            with self.subTest(output=output):
                with self.assertRaises(RuntimeError):
                    bench.parse_webkit_allocated(output)

    def test_checkpoint_log_preserves_partial_lines_and_attempts_each_id_once(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "frontend.log"
            checkpoints = bench.CheckpointLog(path)
            checkpoints.poll()
            path.write_bytes("中文日志\n[SOAK_CHECKPOINT_BEG".encode())
            checkpoints.poll()
            self.assertFalse(checkpoints.pending)
            with path.open("ab") as log:
                log.write(b"IN] 12\n")
            checkpoints.poll()
            first = checkpoints.pending.popleft()
            self.assertEqual(first["checkpointId"], 12)
            self.assertEqual(checkpoints.invalid_reasons(first), [])
            checkpoints.poll()
            self.assertFalse(checkpoints.pending)
            with path.open("ab") as log:
                log.write(b"[SOAK_CHECKPOINT_END] 12\n[SOAK_CHECKPOINT_BEGIN] 13\n[SOAK_CHECKPOINT_BEGIN] 13\n")
            checkpoints.poll()
            second = checkpoints.pending.popleft()
            self.assertTrue(checkpoints.invalid_reasons(first))
            self.assertTrue(checkpoints.invalid_reasons(second))
            self.assertFalse(checkpoints.pending)

    def test_checkpoint_log_refuses_truncation_or_replacement(self):
        for replace in (False, True):
            with self.subTest(replace=replace), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "frontend.log"
                path.write_text("[SOAK_CHECKPOINT_BEGIN] 1\n")
                checkpoints = bench.CheckpointLog(path)
                checkpoints.poll()
                if replace:
                    replacement = Path(directory) / "replacement.log"
                    replacement.write_text("[SOAK_CHECKPOINT_BEGIN] 2\n")
                    replacement.replace(path)
                else:
                    path.write_text("")
                with self.assertRaises(RuntimeError):
                    checkpoints.poll()

    def test_phase_sample_rechecks_boundaries_exit_and_owned_processes(self):
        for transition in ("none", "end", "begin", "exit", "ownership", "failure", "timeout"):
            with self.subTest(transition=transition), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "frontend.log"
                path.write_text("[SOAK_CHECKPOINT_BEGIN] 1\n")
                checkpoints = bench.CheckpointLog(path)
                checkpoints.poll()
                checkpoint = checkpoints.pending.popleft()
                checkpoint["observedAt"] = 96 if transition == "timeout" else 100
                services = {"WebContent": [45], "Networking": [47], "GPU": [46]}
                rss = {"rustRssMb": 100, "webContentRssMb": 200,
                       "webContentPids": [45], "networkingPids": [47], "gpuPids": [46]}
                proc = mock.Mock(pid=12)
                proc.poll.return_value = None

                def snapshot(_):
                    if transition in ("end", "begin"):
                        marker = b"[SOAK_CHECKPOINT_END] 1\n" if transition == "end" else b"[SOAK_CHECKPOINT_BEGIN] 2\n"
                        with path.open("ab") as log:
                            log.write(marker)
                    elif transition == "exit":
                        proc.poll.return_value = 0
                    elif transition == "failure":
                        raise RuntimeError("vmmap failed")
                    return {"pid": 45, "footprintMb": 90, "webkitMallocAllocatedMb": 60}

                after = dict(rss, webContentPids=[99]) if transition == "ownership" else rss
                with mock.patch.object(bench, "sample_memory", side_effect=[rss, after]), \
                        mock.patch.object(bench, "physical_footprint", return_value=50), \
                        mock.patch.object(bench, "webcontent_memory", side_effect=snapshot), \
                        mock.patch.object(bench.time, "monotonic", return_value=101):
                    sample = bench.sample_phase_memory(proc, checkpoints, checkpoint, services, 0)
                self.assertFalse(sample["valid"])
                self.assertEqual(sample["samplingChecksPassed"], transition == "none")
                self.assertEqual(sample["checkpointId"], 1)
                self.assertGreaterEqual(sample["samplingDurationSeconds"], 0)
                if transition == "none":
                    self.assertEqual(sample["webContentMallocAllocatedMb"], 60)
                    self.assertEqual(sample["rustFootprintMb"], 50)
                    self.assertEqual(sample["errors"], [])
                else:
                    self.assertTrue(sample["errors"])
                self.assertEqual(len(checkpoints.pending), 1 if transition == "begin" else 0)

    def test_expired_checkpoint_is_retained_without_sampling_or_retry(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "frontend.log"
            path.write_text("[SOAK_CHECKPOINT_BEGIN] 1\n[SOAK_CHECKPOINT_END] 1\n")
            checkpoints = bench.CheckpointLog(path)
            checkpoints.poll()
            proc = mock.Mock(pid=12)
            proc.poll.return_value = None
            sample = checkpoints.pending.popleft()
            with mock.patch.object(bench, "sample_memory") as memory:
                result = bench.sample_phase_memory(proc, checkpoints, sample, None, 0)
            self.assertFalse(result["valid"])
            memory.assert_not_called()
            with path.open("a") as log:
                log.write("[SOAK_CHECKPOINT_BEGIN] 1\n")
            checkpoints.poll()
            self.assertFalse(checkpoints.pending)

    def test_phase_requires_matching_end_and_frontend_interval_before_valid(self):
        for case in ("closed", "missing_end", "outside_interval", "missing_frontend"):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "frontend.log"
                path.write_text("[SOAK_CHECKPOINT_BEGIN] 1\n")
                checkpoints = bench.CheckpointLog(path)
                checkpoints.poll()
                if case != "missing_end":
                    with path.open("a") as log:
                        log.write("[SOAK_CHECKPOINT_END] 1\n")
                    checkpoints.poll()
                sample = {"checkpointId": 1, "valid": False, "errors": [],
                          "timestamp": "2026-10-21T07:00:01+00:00", "samplingEndedAt": "2026-10-21T07:00:02+00:00"}
                frontend = {"phaseCheckpoints": [{"cycle": 1, "file": "src/staged.ts", "view": "split",
                            "startedAt": "2026-10-21T07:00:00Z", "endedAt": "2026-10-21T07:00:03.500Z"}]}
                if case == "outside_interval":
                    frontend["phaseCheckpoints"][0]["endedAt"] = "2026-10-21T07:00:01.500Z"
                if case == "missing_frontend":
                    frontend = {}
                bench.finalize_phase_samples([sample], checkpoints, frontend)
                self.assertEqual(sample["valid"], case == "closed")
                self.assertEqual(sample["validation"], "final")
                self.assertEqual(bool(sample["errors"]), case != "closed")

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
