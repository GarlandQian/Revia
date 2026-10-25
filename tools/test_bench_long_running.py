"""Regression checks for false success and unrelated WebKit PID attribution."""
import importlib.util
import json
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

    def test_diagnostic_cannot_pass_as_baseline_or_with_wrong_load(self):
        config = {"loggingMode": "minimal", "idleSeconds": 90, "cycles": 120}
        frontend = {"ok": True, "actualDurationSeconds": 360, "diagnostic": config,
                    "totalCycles": 120, "totalFileSwitches": 840, "totalViewSwitches": 1440,
                    "totalScrolls": 3960, "totalJumps": 720, "files": [{}] * 7,
                    "ipcFinal": {}, "idle": {"actualSeconds": 90}}
        samples = [{"webContentPids": [45]}]
        self.assertEqual(bench.completion_errors(frontend, samples, 900, 0, config), [])
        self.assertTrue(bench.completion_errors(frontend, samples, 1800, 0))
        self.assertTrue(bench.completion_errors({**frontend, "actualDurationSeconds": 1800}, samples, 1800, 0))
        for key, value in (("totalCycles", 119), ("totalFileSwitches", 839), ("ipcFinal", None),
                           ("diagnostic", None), ("idle", {"actualSeconds": 89})):
            with self.subTest(key=key):
                self.assertTrue(bench.completion_errors({**frontend, key: value}, samples, 900, 0, config))

    def test_minimal_logging_baseline_still_requires_full_duration_and_mode_match(self):
        frontend = {"ok": True, "actualDurationSeconds": 1800, "diagnostic": None,
                    "minimalSoakLogging": True, "ipcFinal": {},
                    "totalFileSwitches": 7, "totalViewSwitches": 12,
                    "totalScrolls": 33, "totalJumps": 6}
        samples = [{"webContentPids": [45]}]
        self.assertEqual(bench.completion_errors(frontend, samples, 1800, 0, None, True), [])
        for key, value in (("actualDurationSeconds", 1799), ("minimalSoakLogging", False), ("ipcFinal", None)):
            with self.subTest(key=key):
                self.assertTrue(bench.completion_errors({**frontend, key: value}, samples, 1800, 0, None, True))

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

                def snapshot(_pid, _evidence=None):
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
        for case in ("closed", "missing_end", "outside_interval", "missing_frontend", "hidden_during_hold"):
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
                if case == "hidden_during_hold":
                    frontend["phaseCheckpoints"][0]["hiddenDuringHold"] = True
                bench.finalize_phase_samples([sample], checkpoints, frontend)
                self.assertEqual(sample["valid"], case == "closed")
                self.assertEqual(sample["validation"], "final")
                self.assertEqual(bool(sample["errors"]), case != "closed")

    def test_pressure_checkpoint_posts_low_memory_then_samples_after_settle(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "frontend.log"
            path.write_text("[SOAK_CHECKPOINT_BEGIN] 1\n[SOAK_CHECKPOINT_END] 1\n[SOAK_CHECKPOINT_BEGIN] 7 pressure\n")
            checkpoints = bench.CheckpointLog(path)
            checkpoints.poll()
            regular, pressure = checkpoints.pending.popleft(), checkpoints.pending.popleft()
            self.assertFalse(regular["pressure"])
            self.assertTrue(pressure["pressure"])
            pressure["observedAt"] = 100
            services = {"WebContent": [45], "Networking": [47], "GPU": [46]}
            rss = {"rustRssMb": 100, "webContentRssMb": 200,
                   "webContentPids": [45], "networkingPids": [47], "gpuPids": [46]}
            proc = mock.Mock(pid=12)
            proc.poll.return_value = None
            clock = iter([100, 100.2, 100.3, 113])
            with mock.patch.object(bench, "sample_memory", side_effect=[rss, rss]), \
                    mock.patch.object(bench, "physical_footprint", return_value=50), \
                    mock.patch.object(bench, "webcontent_memory",
                                      return_value={"pid": 45, "footprintMb": 90, "webkitMallocAllocatedMb": 20}), \
                    mock.patch.object(bench.subprocess, "run") as run, \
                    mock.patch.object(bench.time, "sleep") as sleep, \
                    mock.patch.object(bench.time, "monotonic", side_effect=lambda: next(clock)):
                sample = bench.sample_phase_memory(proc, checkpoints, pressure, services, 0)
            run.assert_called_once()
            self.assertEqual(run.call_args.args[0], ["notifyutil", "-p", "org.WebKit.lowMemory"])
            self.assertAlmostEqual(sleep.call_args.args[0], 9.9)
            self.assertTrue(sample["pressure"])
            self.assertEqual(sample["errors"], [])
            self.assertEqual(sample["webContentMallocAllocatedMb"], 20)

    def test_q98_verdict_uses_post_pressure_samples_and_regular_rust_samples(self):
        def phase(elapsed, footprint, malloc, pressure=True, valid=True):
            # Final-report samples are the authoritative shape; assertions below are unchanged.
            return {"pressure": pressure, "valid": valid, "authoritative": True,
                    "samplingStartedElapsedSec": elapsed,
                    "webContentFootprintMb": footprint, "webContentMallocAllocatedMb": malloc}
        regular = [{"elapsedSec": t, "rustFootprintMb": 40 + t / 600} for t in range(200, 1800, 30)]
        # Pre-warmup, invalid and non-pressure samples must not affect the verdict.
        noise = [phase(100, 500, 500), phase(700, 500, 500, valid=False), phase(800, 500, 500, pressure=False)]
        flat = [phase(t, 130 + (t % 2), 22) for t in (300, 600, 900, 1200, 1500)]
        verdict = bench.analyze_q98(regular, noise + flat, 10)
        self.assertTrue(verdict["passed"], verdict["errors"])
        self.assertEqual(verdict["pressureSampleCount"], 5)
        growing = [phase(t, 130 + t / 60, 22) for t in (300, 600, 900, 1200, 1500)]
        verdict = bench.analyze_q98(regular, growing, 10)
        self.assertFalse(verdict["passed"])
        self.assertFalse(verdict["series"]["webContentFootprintMb"]["passed"])
        self.assertFalse(bench.analyze_q98(regular, flat[:2], 10)["passed"])

    def test_focus_diagnostic_requires_only_its_own_load(self):
        diagnostic = {"loggingMode": "minimal", "idleSeconds": 0, "cycles": None, "focus": "ipc"}
        frontend = {"ok": True, "diagnostic": diagnostic, "minimalSoakLogging": True, "ipcFinal": {},
                    "actualDurationSeconds": 60, "idle": {"actualSeconds": 0}, "totalIpcRequests": 10}
        samples = [{"webContentPids": [45]}]
        self.assertEqual(bench.completion_errors(frontend, samples, 60, 0, diagnostic, True), [])
        self.assertTrue(bench.completion_errors(dict(frontend, totalIpcRequests=0), samples, 60, 0, diagnostic, True))
        idle = dict(diagnostic, focus="idle")
        self.assertEqual(bench.completion_errors(dict(frontend, diagnostic=idle), samples, 60, 0, idle, True), [])
        full = dict(diagnostic, focus=None)
        self.assertTrue(bench.completion_errors(dict(frontend, diagnostic=full), samples, 60, 0, full, True))

    def test_command_evidence_records_argv_exit_and_raw_streams(self):
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "evidence.jsonl"
            evidence = bench.CommandEvidence(journal)
            with mock.patch.object(bench.subprocess, "run",
                                   return_value=mock.Mock(returncode=255, stdout="out", stderr="err",
                                                          check_returncode=mock.Mock(
                                                              side_effect=bench.subprocess.CalledProcessError(255, ["vmmap"])))):
                with self.assertRaises(bench.subprocess.CalledProcessError):
                    evidence.run(["vmmap", "-summary", "90688"], "webcontent-memory", 10)
            record = json.loads(journal.read_text().splitlines()[0])
            self.assertEqual(record["argv"], ["vmmap", "-summary", "90688"])
            self.assertEqual(record["purpose"], "webcontent-memory")
            self.assertEqual(record["returncode"], 255)
            self.assertEqual(record["stdout"], "out")
            self.assertEqual(record["stderr"], "err")
            for key in ("startedAt", "endedAt", "durationSeconds"):
                self.assertIn(key, record)

    def test_command_evidence_keeps_partial_output_on_timeout(self):
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "evidence.jsonl"
            evidence = bench.CommandEvidence(journal)
            timeout = bench.subprocess.TimeoutExpired(["vmmap", "-summary", "45"], 10,
                                                      output="partial out", stderr="partial err")
            with mock.patch.object(bench.subprocess, "run", side_effect=timeout):
                with self.assertRaises(bench.subprocess.TimeoutExpired):
                    evidence.run(["vmmap", "-summary", "45"], "rust-footprint", 10)
            record = json.loads(journal.read_text().splitlines()[0])
            self.assertEqual(record["exception"], "TimeoutExpired")
            self.assertEqual(record["timeoutSeconds"], 10)
            self.assertEqual(record["partialStdout"], "partial out")
            self.assertEqual(record["partialStderr"], "partial err")

    def test_ownership_evidence_never_retains_launchd_environment(self):
        sentinel = "SENTINEL_SECRET_VALUE_DO_NOT_PERSIST"
        domain = ("pid/12 = {\n environment = {\n TOKEN => " + sentinel + "\n }\n"
                  " services = {\n    45 - com.apple.WebKit.WebContent.ABC\n"
                  "    47 - com.apple.WebKit.Networking.GHI\n    46 - com.apple.WebKit.GPU.DEF\n }\n}")
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "evidence.jsonl"
            evidence = bench.CommandEvidence(journal)
            with mock.patch.object(bench.subprocess, "run",
                                   return_value=mock.Mock(returncode=0, stdout=domain, stderr="")):
                returned = bench.launchctl_ownership(evidence, 12, "rss-sample")
            self.assertEqual(returned, domain)
            raw = journal.read_text()
            self.assertNotIn(sentinel, raw)
            record = json.loads(raw.splitlines()[0])
            self.assertEqual(record["services"], {"WebContent": [45], "Networking": [47], "GPU": [46]})
            self.assertIn("redaction", record)
            self.assertTrue(record["webkitServiceRows"])

    def test_ownership_failure_is_missing_evidence_not_success(self):
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "evidence.jsonl"
            evidence = bench.CommandEvidence(journal)
            with mock.patch.object(bench.subprocess, "run",
                                   return_value=mock.Mock(returncode=1, stdout="", stderr="denied")):
                self.assertIsNone(bench.launchctl_ownership(evidence, 12, "rss-sample"))
            record = json.loads(journal.read_text().splitlines()[0])
            self.assertEqual(record["returncode"], 1)
            self.assertNotIn("stdout", record)

    def test_progress_log_exposes_only_closed_unique_checkpoints(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "frontend.log"
            records = [
                {"kind": "run", "runId": "r1", "targetSeconds": 1800},
                {"kind": "checkpoint", "runId": "r1", "cycle": 1, "file": "src/staged.ts", "view": "split",
                 "pressure": False, "startedAt": "2026-10-25T08:00:00Z", "endedAt": "2026-10-25T08:00:04Z",
                 "hiddenDuringHold": False},
            ]
            lines = [f"noise\n[SOAK_PROGRESS] {json.dumps(item)}\n" for item in records]
            # Unterminated tail must not be parsed as progress.
            lines.append('[SOAK_PROGRESS] {"kind": "checkpoint", "runId": "r1", "cycle": 9')
            path.write_text("".join(lines))
            anchor, closed, problems, _tail = bench.ProgressLog(path).read()
            self.assertEqual(anchor["runId"], "r1")
            self.assertEqual([item["cycle"] for item in closed], [1])
            self.assertEqual(problems, [])

    def test_progress_log_fails_closed_on_corruption_duplicates_and_foreign_runs(self):
        cases = {
            "bad_json": '[SOAK_PROGRESS] {not json}\n',
            "duplicate": ('[SOAK_PROGRESS] {"kind": "run", "runId": "r1"}\n'
                          '[SOAK_PROGRESS] {"kind": "checkpoint", "runId": "r1", "cycle": 1, "endedAt": "x"}\n'
                          '[SOAK_PROGRESS] {"kind": "checkpoint", "runId": "r1", "cycle": 1, "endedAt": "x"}\n'),
            "foreign_run": ('[SOAK_PROGRESS] {"kind": "run", "runId": "r1"}\n'
                            '[SOAK_PROGRESS] {"kind": "checkpoint", "runId": "r2", "cycle": 1, "endedAt": "x"}\n'),
            "before_anchor": '[SOAK_PROGRESS] {"kind": "checkpoint", "runId": "r1", "cycle": 1, "endedAt": "x"}\n',
            "second_anchor": ('[SOAK_PROGRESS] {"kind": "run", "runId": "r1"}\n'
                              '[SOAK_PROGRESS] {"kind": "run", "runId": "r2"}\n'),
            "unclosed": ('[SOAK_PROGRESS] {"kind": "run", "runId": "r1"}\n'
                         '[SOAK_PROGRESS] {"kind": "checkpoint", "runId": "r1", "cycle": 1}\n'),
            "unknown_kind": ('[SOAK_PROGRESS] {"kind": "run", "runId": "r1"}\n'
                             '[SOAK_PROGRESS] {"kind": "mystery", "runId": "r1"}\n'),
        }
        for name, body in cases.items():
            with self.subTest(case=name), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "frontend.log"
                path.write_text(body)
                _anchor, closed, problems, _tail = bench.ProgressLog(path).read()
                self.assertTrue(problems, name)
                self.assertFalse([item for item in closed if item.get("cycle") == 1 and not item.get("endedAt")])

    def test_missing_progress_log_is_reported_not_assumed(self):
        with tempfile.TemporaryDirectory() as directory:
            anchor, closed, problems, _tail = bench.ProgressLog(Path(directory) / "absent.log").read()
            self.assertIsNone(anchor)
            self.assertEqual(closed, [])
            self.assertTrue(problems)

    def test_progress_records_never_substitute_for_final_report(self):
        """Progress-derived phase metadata may be diagnostic, never authoritative."""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "frontend.log"
            path.write_text("[SOAK_CHECKPOINT_BEGIN] 1\n[SOAK_CHECKPOINT_END] 1\n")
            checkpoints = bench.CheckpointLog(path)
            checkpoints.poll()
            sample = {"checkpointId": 1, "valid": False, "errors": [], "pressure": False,
                      "timestamp": "2026-10-25T08:00:01+00:00", "samplingEndedAt": "2026-10-25T08:00:02+00:00"}
            progress = [{"cycle": 1, "file": "src/staged.ts", "view": "split", "pressure": False,
                         "startedAt": "2026-10-25T08:00:00Z", "endedAt": "2026-10-25T08:00:04Z",
                         "hiddenDuringHold": False}]
            bench.finalize_phase_samples([sample], checkpoints, {}, progress)
            self.assertEqual(sample["checkpointSource"], "progress-log")
            self.assertFalse(sample["authoritative"])
            # With a real final report the same metadata becomes authoritative.
            final = {"phaseCheckpoints": progress}
            sample2 = {"checkpointId": 1, "valid": False, "errors": [], "pressure": False,
                       "timestamp": "2026-10-25T08:00:01+00:00", "samplingEndedAt": "2026-10-25T08:00:02+00:00"}
            bench.finalize_phase_samples([sample2], checkpoints, final, progress)
            self.assertEqual(sample2["checkpointSource"], "final-report")
            self.assertTrue(sample2["authoritative"])
            self.assertTrue(sample2["valid"])

    def test_progress_without_final_report_still_fails_the_run(self):
        """A run with only progress records must remain failed (fail-closed)."""
        progress_only = {"ok": None, "actualDurationSeconds": 0}
        self.assertTrue(bench.completion_errors(progress_only, [{"webContentPids": [45]}], 1800, 0))
        self.assertTrue(bench.completion_errors({}, [{"webContentPids": [45]}], 1800, 0))


    def test_command_evidence_records_argv_exit_and_raw_streams(self):
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "evidence.jsonl"
            evidence = bench.CommandEvidence(journal)
            with mock.patch.object(bench.subprocess, "run",
                                   return_value=mock.Mock(returncode=255, stdout="out", stderr="err",
                                                          check_returncode=mock.Mock(
                                                              side_effect=bench.subprocess.CalledProcessError(255, ["vmmap"])))):
                with self.assertRaises(bench.subprocess.CalledProcessError):
                    evidence.run(["vmmap", "-summary", "90688"], "webcontent-memory", 10)
            record = json.loads(journal.read_text().splitlines()[0])
            self.assertEqual(record["argv"], ["vmmap", "-summary", "90688"])
            self.assertEqual(record["purpose"], "webcontent-memory")
            self.assertEqual(record["returncode"], 255)
            self.assertEqual(record["stdout"], "out")
            self.assertEqual(record["stderr"], "err")
            for key in ("startedAt", "endedAt", "durationSeconds"):
                self.assertIn(key, record)

    def test_command_evidence_keeps_partial_output_on_timeout(self):
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "evidence.jsonl"
            evidence = bench.CommandEvidence(journal)
            timeout = bench.subprocess.TimeoutExpired(["vmmap", "-summary", "45"], 10,
                                                      output="partial out", stderr="partial err")
            with mock.patch.object(bench.subprocess, "run", side_effect=timeout):
                with self.assertRaises(bench.subprocess.TimeoutExpired):
                    evidence.run(["vmmap", "-summary", "45"], "rust-footprint", 10)
            record = json.loads(journal.read_text().splitlines()[0])
            self.assertEqual(record["exception"], "TimeoutExpired")
            self.assertEqual(record["timeoutSeconds"], 10)
            self.assertEqual(record["partialStdout"], "partial out")
            self.assertEqual(record["partialStderr"], "partial err")

    def test_ownership_evidence_never_retains_launchd_environment(self):
        sentinel = "SENTINEL_SECRET_VALUE_DO_NOT_PERSIST"
        domain = ("pid/12 = {\n environment = {\n TOKEN => " + sentinel + "\n }\n"
                  " services = {\n    45 - com.apple.WebKit.WebContent.ABC\n"
                  "    47 - com.apple.WebKit.Networking.GHI\n    46 - com.apple.WebKit.GPU.DEF\n }\n}")
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "evidence.jsonl"
            evidence = bench.CommandEvidence(journal)
            with mock.patch.object(bench.subprocess, "run",
                                   return_value=mock.Mock(returncode=0, stdout=domain, stderr="")):
                returned = bench.launchctl_ownership(evidence, 12, "rss-sample")
            self.assertEqual(returned, domain)
            raw = journal.read_text()
            self.assertNotIn(sentinel, raw)
            record = json.loads(raw.splitlines()[0])
            self.assertEqual(record["services"], {"WebContent": [45], "Networking": [47], "GPU": [46]})
            self.assertIn("redaction", record)
            self.assertTrue(record["webkitServiceRows"])

    def test_ownership_failure_is_missing_evidence_not_success(self):
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "evidence.jsonl"
            evidence = bench.CommandEvidence(journal)
            with mock.patch.object(bench.subprocess, "run",
                                   return_value=mock.Mock(returncode=1, stdout="", stderr="denied")):
                self.assertIsNone(bench.launchctl_ownership(evidence, 12, "rss-sample"))
            record = json.loads(journal.read_text().splitlines()[0])
            self.assertEqual(record["returncode"], 1)
            self.assertNotIn("stdout", record)

    def test_progress_log_exposes_only_closed_unique_checkpoints(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "frontend.log"
            records = [
                {"kind": "run", "runId": "r1", "targetSeconds": 1800},
                {"kind": "checkpoint", "runId": "r1", "cycle": 1, "file": "src/staged.ts", "view": "split",
                 "pressure": False, "startedAt": "2026-10-25T08:00:00Z", "endedAt": "2026-10-25T08:00:04Z",
                 "hiddenDuringHold": False},
            ]
            lines = [f"noise\n[SOAK_PROGRESS] {json.dumps(item)}\n" for item in records]
            # Unterminated tail must not be parsed as progress.
            lines.append('[SOAK_PROGRESS] {"kind": "checkpoint", "runId": "r1", "cycle": 9')
            path.write_text("".join(lines))
            anchor, closed, problems, _tail = bench.ProgressLog(path).read()
            self.assertEqual(anchor["runId"], "r1")
            self.assertEqual([item["cycle"] for item in closed], [1])
            self.assertEqual(problems, [])

    def test_progress_log_fails_closed_on_corruption_duplicates_and_foreign_runs(self):
        cases = {
            "bad_json": '[SOAK_PROGRESS] {not json}\n',
            "duplicate": ('[SOAK_PROGRESS] {"kind": "run", "runId": "r1"}\n'
                          '[SOAK_PROGRESS] {"kind": "checkpoint", "runId": "r1", "cycle": 1, "endedAt": "x"}\n'
                          '[SOAK_PROGRESS] {"kind": "checkpoint", "runId": "r1", "cycle": 1, "endedAt": "x"}\n'),
            "foreign_run": ('[SOAK_PROGRESS] {"kind": "run", "runId": "r1"}\n'
                            '[SOAK_PROGRESS] {"kind": "checkpoint", "runId": "r2", "cycle": 1, "endedAt": "x"}\n'),
            "before_anchor": '[SOAK_PROGRESS] {"kind": "checkpoint", "runId": "r1", "cycle": 1, "endedAt": "x"}\n',
            "second_anchor": ('[SOAK_PROGRESS] {"kind": "run", "runId": "r1"}\n'
                              '[SOAK_PROGRESS] {"kind": "run", "runId": "r2"}\n'),
            "unclosed": ('[SOAK_PROGRESS] {"kind": "run", "runId": "r1"}\n'
                         '[SOAK_PROGRESS] {"kind": "checkpoint", "runId": "r1", "cycle": 1}\n'),
            "unknown_kind": ('[SOAK_PROGRESS] {"kind": "run", "runId": "r1"}\n'
                             '[SOAK_PROGRESS] {"kind": "mystery", "runId": "r1"}\n'),
        }
        for name, body in cases.items():
            with self.subTest(case=name), tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "frontend.log"
                path.write_text(body)
                _anchor, closed, problems, _tail = bench.ProgressLog(path).read()
                self.assertTrue(problems, name)
                self.assertFalse([item for item in closed if item.get("cycle") == 1 and not item.get("endedAt")])

    def test_missing_progress_log_is_reported_not_assumed(self):
        with tempfile.TemporaryDirectory() as directory:
            anchor, closed, problems, _tail = bench.ProgressLog(Path(directory) / "absent.log").read()
            self.assertIsNone(anchor)
            self.assertEqual(closed, [])
            self.assertTrue(problems)

    def test_progress_records_never_substitute_for_final_report(self):
        """Progress-derived phase metadata may be diagnostic, never authoritative."""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "frontend.log"
            path.write_text("[SOAK_CHECKPOINT_BEGIN] 1\n[SOAK_CHECKPOINT_END] 1\n")
            checkpoints = bench.CheckpointLog(path)
            checkpoints.poll()
            sample = {"checkpointId": 1, "valid": False, "errors": [], "pressure": False,
                      "timestamp": "2026-10-25T08:00:01+00:00", "samplingEndedAt": "2026-10-25T08:00:02+00:00"}
            progress = [{"cycle": 1, "file": "src/staged.ts", "view": "split", "pressure": False,
                         "startedAt": "2026-10-25T08:00:00Z", "endedAt": "2026-10-25T08:00:04Z",
                         "hiddenDuringHold": False}]
            bench.finalize_phase_samples([sample], checkpoints, {}, progress)
            self.assertEqual(sample["checkpointSource"], "progress-log")
            self.assertFalse(sample["authoritative"])
            # With a real final report the same metadata becomes authoritative.
            final = {"phaseCheckpoints": progress}
            sample2 = {"checkpointId": 1, "valid": False, "errors": [], "pressure": False,
                       "timestamp": "2026-10-25T08:00:01+00:00", "samplingEndedAt": "2026-10-25T08:00:02+00:00"}
            bench.finalize_phase_samples([sample2], checkpoints, final, progress)
            self.assertEqual(sample2["checkpointSource"], "final-report")
            self.assertTrue(sample2["authoritative"])
            self.assertTrue(sample2["valid"])

    def test_progress_without_final_report_still_fails_the_run(self):
        """A run with only progress records must remain failed (fail-closed)."""
        progress_only = {"ok": None, "actualDurationSeconds": 0}
        self.assertTrue(bench.completion_errors(progress_only, [{"webContentPids": [45]}], 1800, 0))
        self.assertTrue(bench.completion_errors({}, [{"webContentPids": [45]}], 1800, 0))


    def test_sample_failure_keeps_partial_candidate_and_polls_app(self):
        """A failing footprint step must not lose what was already read."""
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "evidence.jsonl"
            evidence = bench.CommandEvidence(journal)
            # RSS stage succeeds and is recorded; the footprint command then fails.
            domain = ("pid/12 = {\n services = {\n 45 - com.apple.WebKit.WebContent.A\n"
                      " 47 - com.apple.WebKit.Networking.B\n 46 - com.apple.WebKit.GPU.C\n }\n}")

            def dispatch(argv, **kwargs):
                text = "12 100\n45 200\n47 50\n46 60\n" if argv[0] == "ps" else domain
                return mock.Mock(returncode=0, stdout=text, stderr="", check_returncode=mock.Mock())

            with mock.patch.object(bench.subprocess, "run", side_effect=dispatch):
                rss = bench.sample_memory(12, evidence)
            self.assertEqual(rss["rustRssMb"], 0.1)
            with mock.patch.object(bench.subprocess, "run",
                                   return_value=mock.Mock(returncode=255, stdout="", stderr="denied",
                                                          check_returncode=mock.Mock(
                                                              side_effect=bench.subprocess.CalledProcessError(255, ["vmmap"])))):
                with self.assertRaises(bench.subprocess.CalledProcessError):
                    bench.physical_footprint(12, evidence)
            kinds = [json.loads(line)["kind"] for line in journal.read_text().splitlines()]
            self.assertIn("command", kinds)
            failing = [json.loads(line) for line in journal.read_text().splitlines()
                       if json.loads(line).get("purpose") == "rust-footprint"][0]
            self.assertEqual(failing["returncode"], 255)
            self.assertEqual(failing["stderr"], "denied")

    def test_completion_still_requires_final_report_even_with_valid_progress(self):
        """Progress-derived valid phase samples must not complete a run without ok/duration."""
        frontend = {"actualDurationSeconds": 1800, "totalFileSwitches": 7, "totalViewSwitches": 12,
                    "totalScrolls": 33, "totalJumps": 6}
        samples = [{"webContentPids": [45]}]
        # ok missing -> still failed
        self.assertTrue(bench.completion_errors(frontend, samples, 1800, 0))
        self.assertEqual(bench.completion_errors({**frontend, "ok": True}, samples, 1800, 0), [])


    def test_terminate_and_record_covers_success_timeout_and_raising_wait(self):
        """Production cleanup helper: every step is recorded, exceptions never erase evidence."""
        for case in ("already_exited", "terminate_ok", "wait_timeout_kill_ok",
                     "terminate_raises", "wait_raises", "kill_raises"):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                journal = Path(directory) / "evidence.jsonl"
                evidence = bench.CommandEvidence(journal)
                proc = mock.Mock(pid=99)
                cleanup = {}
                proc.wait.return_value = 0
                if case == "already_exited":
                    proc.poll.return_value = 0
                else:
                    proc.poll.return_value = None
                if case == "terminate_raises":
                    proc.terminate.side_effect = OSError("no such process")
                if case == "wait_raises":
                    proc.wait.side_effect = OSError("wait failed")
                if case == "kill_raises":
                    proc.wait.side_effect = [bench.subprocess.TimeoutExpired(["x"], 5), 0]
                    proc.kill.side_effect = OSError("kill failed")
                if case == "wait_timeout_kill_ok":
                    proc.wait.side_effect = [bench.subprocess.TimeoutExpired(["x"], 5), 0]

                bench.terminate_and_record(proc, evidence, cleanup, {"WebContent": [45]})

                records = [json.loads(line) for line in journal.read_text().splitlines()]
                self.assertEqual(len(records), 1)
                record = records[0]
                self.assertEqual(record["kind"], "cleanup")
                self.assertEqual(record["reviaPid"], 99)
                self.assertIn("pollAfterCleanup", record)
                if case == "already_exited":
                    self.assertFalse(record["terminateAttempted"])
                    self.assertEqual(record["signalDelivery"], "not_attempted")
                    proc.terminate.assert_not_called()
                else:
                    self.assertTrue(record["terminateAttempted"])
                    # Delivery itself is never asserted, only reported as unknown.
                    self.assertEqual(record["signalDelivery"], "unknown")
                if case == "terminate_raises":
                    self.assertFalse(record["terminateReturned"])
                    self.assertIn("terminateError", record)
                if case == "wait_raises":
                    self.assertEqual(record["waitAfterTerminate"], "error")
                    self.assertIn("waitAfterTerminateError", record)
                if case == "wait_timeout_kill_ok":
                    self.assertTrue(record["killAttempted"])
                    self.assertTrue(record["killReturned"])
                    self.assertEqual(record["waitAfterKill"], 0)
                if case == "kill_raises":
                    self.assertTrue(record["killAttempted"])
                    self.assertFalse(record["killReturned"])
                    self.assertIn("killError", record)

    def test_preserve_partial_candidate_keeps_parsed_values_and_process_state(self):
        """Parse failures (not just subprocess errors) keep what was already read."""
        for error in (RuntimeError("vmmap 缺少 Physical footprint，不能以 RSS 代替"),
                      bench.subprocess.CalledProcessError(255, ["vmmap"]),
                      bench.subprocess.TimeoutExpired(["vmmap"], 10)):
            with self.subTest(error=type(error).__name__), tempfile.TemporaryDirectory() as directory:
                journal = Path(directory) / "evidence.jsonl"
                evidence = bench.CommandEvidence(journal)
                partials = []
                proc = mock.Mock(pid=7)
                proc.poll.return_value = None
                candidate = {"elapsedSec": 12.5, "samplingStartedAt": "2026-10-25T08:00:00Z"}
                sample = {"rustRssMb": 123.4, "webContentPids": [45], "rustFootprintMb": 150.0}
                bench.preserve_partial_candidate(candidate, sample, error, proc,
                                                 {"WebContent": [45]}, evidence, partials)
                self.assertEqual(len(partials), 1)
                kept = partials[0]
                self.assertEqual(kept["rustRssMb"], 123.4)
                self.assertEqual(kept["rustFootprintMb"], 150.0)
                self.assertEqual(kept["elapsedSec"], 12.5)
                self.assertEqual(kept["reviaPid"], 7)
                self.assertIsNone(kept["pollAtFailure"])
                self.assertIn(type(error).__name__, kept["failure"])
                self.assertEqual(json.loads(journal.read_text().splitlines()[0])["kind"], "sample-failure")

    def test_progress_problems_block_success_in_production_main(self):
        """Corrupt/duplicate/cross-run/unclosed progress must fail the real run."""
        for case in ("bad_json", "duplicate_anchor", "duplicate_cycle", "cross_run",
                     "before_anchor", "unclosed", "unknown_kind", "missing_anchor"):
            with self.subTest(case=case), tempfile.TemporaryDirectory() as directory:
                log = Path(directory) / "run.log"
                anchor = {"kind": "run", "runId": "r1", "outputPath": str(Path(directory) / "out.json"),
                          "targetSeconds": 60}
                body = {
                    "bad_json": f'[SOAK_PROGRESS] {json.dumps(anchor)}\n[SOAK_PROGRESS] {{bad\n',
                    "duplicate_anchor": (f'[SOAK_PROGRESS] {json.dumps(anchor)}\n'
                                         f'[SOAK_PROGRESS] {json.dumps({**anchor, "runId": "r2"})}\n'),
                    "duplicate_cycle": (f'[SOAK_PROGRESS] {json.dumps(anchor)}\n'
                                        f'[SOAK_PROGRESS] {json.dumps({"kind": "checkpoint", "runId": "r1", "cycle": 1, "endedAt": "x"})}\n'
                                        f'[SOAK_PROGRESS] {json.dumps({"kind": "checkpoint", "runId": "r1", "cycle": 1, "endedAt": "x"})}\n'),
                    "cross_run": (f'[SOAK_PROGRESS] {json.dumps(anchor)}\n'
                                  f'[SOAK_PROGRESS] {json.dumps({"kind": "checkpoint", "runId": "r9", "cycle": 1, "endedAt": "x"})}\n'),
                    "before_anchor": f'[SOAK_PROGRESS] {json.dumps({"kind": "checkpoint", "runId": "r1", "cycle": 1, "endedAt": "x"})}\n',
                    "unclosed": (f'[SOAK_PROGRESS] {json.dumps(anchor)}\n'
                                 f'[SOAK_PROGRESS] {json.dumps({"kind": "checkpoint", "runId": "r1", "cycle": 1})}\n'),
                    "unknown_kind": (f'[SOAK_PROGRESS] {json.dumps(anchor)}\n'
                                     f'[SOAK_PROGRESS] {json.dumps({"kind": "mystery", "runId": "r1"})}\n'),
                    "missing_anchor": "",
                }[case]
                log.write_text(body)
                _anchor, _closed, problems, _tail = bench.ProgressLog(log).read(
                    expected_output=str(Path(directory) / "out.json"), expected_target=60)
                self.assertTrue(problems, case)

    def test_progress_anchor_must_match_this_invocation(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "run.log"
            anchor = {"kind": "run", "runId": "r1", "outputPath": "/somewhere/else.json",
                      "targetSeconds": 60}
            log.write_text(f'[SOAK_PROGRESS] {json.dumps(anchor)}\n')
            _a, _c, problems, _t = bench.ProgressLog(log).read(
                expected_output=str(Path(directory) / "out.json"), expected_target=60)
            self.assertTrue(any("输出路径不匹配" in item for item in problems))
            _a, _c, problems, _t = bench.ProgressLog(log).read(
                expected_output="/somewhere/else.json", expected_target=1800)
            self.assertTrue(any("目标时长不匹配" in item for item in problems))

    def test_unterminated_tail_is_preserved_verbatim_and_not_parsed(self):
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "run.log"
            anchor = {"kind": "run", "runId": "r1", "outputPath": "x", "targetSeconds": 60}
            log.write_text(f'[SOAK_PROGRESS] {json.dumps(anchor)}\n[SOAK_PROGRESS] {{"kind": "checkpoint"')
            _a, closed, problems, tail = bench.ProgressLog(log).read("x", 60)
            self.assertEqual(closed, [])
            self.assertEqual(problems, [])
            self.assertEqual(tail, '[SOAK_PROGRESS] {"kind": "checkpoint"')

    def test_missing_final_checkpoints_fail_even_with_valid_progress(self):
        """Progress metadata is diagnostic only; acceptance requires the final report."""
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "run.log"
            log.write_text("[SOAK_CHECKPOINT_BEGIN] 1\n[SOAK_CHECKPOINT_END] 1\n")
            checkpoints = bench.CheckpointLog(log)
            checkpoints.poll()
            progress = [{"cycle": 1, "file": "src/staged.ts", "view": "split", "pressure": True,
                         "startedAt": "2026-10-25T08:00:00Z", "endedAt": "2026-10-25T08:00:04Z",
                         "hiddenDuringHold": False}]
            sample = {"checkpointId": 1, "valid": False, "errors": [], "pressure": True,
                      "timestamp": "2026-10-25T08:00:01+00:00", "samplingEndedAt": "2026-10-25T08:00:02+00:00"}
            # Final report exists but omits phaseCheckpoints -> must fail, not borrow progress.
            bench.finalize_phase_samples([sample], checkpoints, {"ok": True}, progress)
            self.assertFalse(sample["valid"])
            self.assertFalse(sample["authoritative"])
            self.assertEqual(sample["checkpointSource"], "progress-log")
            # And such a sample cannot supply an acceptance pressure point.
            verdict = bench.analyze_q98([], [sample], 0)
            self.assertFalse(verdict["passed"])
            self.assertEqual(verdict["pressureSampleCount"], 0)

    def test_complete_final_report_is_the_authoritative_control(self):
        """Positive control: a real final report makes the same phase sample authoritative."""
        with tempfile.TemporaryDirectory() as directory:
            log = Path(directory) / "run.log"
            log.write_text("[SOAK_CHECKPOINT_BEGIN] 1\n[SOAK_CHECKPOINT_END] 1\n")
            checkpoints = bench.CheckpointLog(log)
            checkpoints.poll()
            report_checkpoint = {"cycle": 1, "file": "src/staged.ts", "view": "split", "pressure": True,
                                 "startedAt": "2026-10-25T08:00:00Z", "endedAt": "2026-10-25T08:00:04Z",
                                 "hiddenDuringHold": False}
            sample = {"checkpointId": 1, "valid": False, "errors": [], "pressure": True,
                      "timestamp": "2026-10-25T08:00:01+00:00", "samplingEndedAt": "2026-10-25T08:00:02+00:00",
                      "samplingStartedElapsedSec": 400, "webContentFootprintMb": 130,
                      "webContentMallocAllocatedMb": 22}
            bench.finalize_phase_samples([sample], checkpoints,
                                         {"ok": True, "phaseCheckpoints": [report_checkpoint]}, [])
            self.assertTrue(sample["valid"])
            self.assertTrue(sample["authoritative"])
            self.assertEqual(sample["checkpointSource"], "final-report")

    def test_final_report_and_progress_must_agree(self):
        report = [{"cycle": 1, "pressure": False, "hiddenDuringHold": False},
                  {"cycle": 2, "pressure": True, "hiddenDuringHold": False}]
        progress = [{"cycle": 1, "pressure": False, "hiddenDuringHold": False},
                    {"cycle": 2, "pressure": True, "hiddenDuringHold": False}]
        self.assertEqual(bench.progress_consistency_errors({"phaseCheckpoints": report}, progress), [])
        mismatched = [{"cycle": 1, "pressure": True, "hiddenDuringHold": False}]
        self.assertTrue(bench.progress_consistency_errors({"phaseCheckpoints": report}, mismatched))
        hidden = [{"cycle": 1, "pressure": False, "hiddenDuringHold": True},
                  {"cycle": 2, "pressure": True, "hiddenDuringHold": False}]
        self.assertTrue(bench.progress_consistency_errors({"phaseCheckpoints": report}, hidden))

    def test_ps_and_notifyutil_go_through_the_audited_boundary(self):
        """ps and notifyutil must record argv/exit and preserve timeout partial output."""
        with tempfile.TemporaryDirectory() as directory:
            journal = Path(directory) / "evidence.jsonl"
            evidence = bench.CommandEvidence(journal)
            domain = ("pid/12 = {\n services = {\n 45 - com.apple.WebKit.WebContent.A\n"
                      " 47 - com.apple.WebKit.Networking.B\n 46 - com.apple.WebKit.GPU.C\n }\n}")

            def dispatch(argv, **kwargs):
                text = "12 100\n45 200\n47 50\n46 60\n" if argv[0] == "ps" else domain
                return mock.Mock(returncode=0, stdout=text, stderr="", check_returncode=mock.Mock())

            with mock.patch.object(bench.subprocess, "run", side_effect=dispatch):
                bench.sample_memory(12, evidence)
            purposes = [json.loads(line)["purpose"] for line in journal.read_text().splitlines()]
            self.assertIn("rss-sample", purposes)

            # notifyutil timeout keeps partial output and re-raises.
            timeout = bench.subprocess.TimeoutExpired(["notifyutil", "-p", "org.WebKit.lowMemory"], 5,
                                                      output="partial", stderr="err")
            with mock.patch.object(bench.subprocess, "run", side_effect=timeout):
                with self.assertRaises(bench.subprocess.TimeoutExpired):
                    evidence.run(["notifyutil", "-p", "org.WebKit.lowMemory"], "low-memory-notification", 5)
            last = json.loads(journal.read_text().splitlines()[-1])
            self.assertEqual(last["purpose"], "low-memory-notification")
            self.assertEqual(last["exception"], "TimeoutExpired")
            self.assertEqual(last["partialStdout"], "partial")


    def _run_production_main(self, directory, frontend=None, log_text="", polls=None, vmmap_text=None):
        """Drive the real bench.main() with only external boundaries mocked."""
        root = Path(directory)
        app = root / "Revia.app"
        (app / "Contents" / "MacOS").mkdir(parents=True)
        (app / "Contents" / "MacOS" / "revia").write_bytes(b"fake build")
        repo = root / "repo"
        repo.mkdir()
        out = root / "run.json"
        domain = ("pid/4242 = {\n services = {\n 45 - com.apple.WebKit.WebContent.A\n"
                  " 47 - com.apple.WebKit.Networking.B\n 46 - com.apple.WebKit.GPU.C\n }\n}")
        vmmap = vmmap_text or ("Physical footprint: 150.0M\n\n"
                               " VIRTUAL RESIDENT DIRTY SWAPPED ALLOCATION BYTES DIRTY+SWAP REGION\n"
                               "MALLOC ZONE SIZE SIZE SIZE SIZE COUNT ALLOCATED FRAG SIZE % FRAG COUNT\n"
                               "=========== ======= ========= ========= ========= ========= ========= ========= ====== ======\n"
                               "WebKit Malloc_0x10200e678 1.5G 383.2M 309.8M 22.3M 725883 20.0M 167.8M 51% 32\n"
                               "=========== ======= ========= ========= ========= ========= ========= ========= ====== ======\n"
                               "TOTAL 1.6G 387.4M 313.5M 23.0M 745593 167.0M 169.4M 51% 471\n")

        def dispatch(argv, **kwargs):
            if argv[0] == "ps":
                return mock.Mock(returncode=0, stdout="4242 100\n45 200\n47 50\n46 60\n",
                                 stderr="", check_returncode=mock.Mock())
            return mock.Mock(returncode=0, stdout=vmmap, stderr="", check_returncode=mock.Mock())

        sequence = list(polls or [None, None, None, 0])
        proc = mock.Mock(pid=4242)
        proc.poll.side_effect = sequence + [0] * 20
        proc.wait.return_value = 0
        proc.returncode = 0
        clock = iter([float(i) for i in range(0, 4000)])
        argv = ["bench-long-running.py", "--seconds", "60", "--interval", "10",
                "--app", str(app), "--repo", str(repo), "--output", str(out)]
        def fake_popen(*args, **kwargs):
            # main() has just opened the log in exclusive mode; append the fixtures the
            # frontend would have produced, so production parsing sees real file content.
            if log_text:
                # Progress/checkpoint records live in the frontend log (REVIA_MEASURE_LOG
                # with its extension replaced), which is what production reads.
                Path(str(out) + ".frontend.log").write_text(log_text)
            if frontend is not None:
                Path(str(out) + ".frontend.json").write_text(json.dumps(frontend))
            return proc

        with mock.patch("sys.argv", argv), \
                mock.patch.object(bench.subprocess, "Popen", side_effect=fake_popen), \
                mock.patch.object(bench.subprocess, "run", side_effect=dispatch), \
                mock.patch.object(bench, "launchctl_ownership", return_value=domain), \
                mock.patch.object(bench.time, "monotonic", side_effect=lambda: next(clock)), \
                mock.patch.object(bench.time, "sleep"), \
                mock.patch.object(bench.platform, "platform", return_value="test-platform"), \
                mock.patch.object(bench.platform, "system", return_value="Darwin"), \
                mock.patch("builtins.print"):
            code = bench.main()
        return code, out

    def test_production_main_fails_on_corrupt_progress_despite_ok_report(self):
        """The real main() must fail when progress records are corrupt, even if ok=true."""
        frontend = {"ok": True, "actualDurationSeconds": 60, "minimalSoakLogging": False,
                    "totalFileSwitches": 7, "totalViewSwitches": 12,
                    "totalScrolls": 33, "totalJumps": 6}
        for name, log_text in (
                ("bad_json", "[SOAK_PROGRESS] {not json}\n"),
                ("no_anchor", ""),
        ):
            with self.subTest(case=name), tempfile.TemporaryDirectory() as directory:
                code, out = self._run_production_main(directory, frontend, log_text)
                report = json.loads(out.read_text())
                self.assertEqual(code, 1, name)
                self.assertFalse(report["runCompleted"], name)
                self.assertTrue(any("进度记录" in error for error in report["errors"]), report["errors"])

    def test_production_main_records_cleanup_and_partial_candidates(self):
        """The real main() writes cleanup evidence and keeps partial candidates."""
        with tempfile.TemporaryDirectory() as directory:
            # vmmap without a footprint line: parse fails after RSS was already read.
            code, out = self._run_production_main(directory, None, "", vmmap_text="no footprint here\n")
            report = json.loads(out.read_text())
            self.assertEqual(code, 1)
            self.assertFalse(report["runCompleted"])
            self.assertTrue(report["partialCandidates"], "部分候选必须保留")
            kept = report["partialCandidates"][0]
            self.assertIn("rustRssMb", kept)
            self.assertIn("failure", kept)
            self.assertIn("pollAtFailure", kept)
            self.assertIn("pollAfterCleanup", report["cleanup"])
            self.assertIn("terminateAttempted", report["cleanup"])
            self.assertTrue(Path(report["commandEvidence"]).exists())

    def test_production_main_completes_with_full_report_and_clean_progress(self):
        """Positive control: a complete report plus clean progress completes the run."""
        frontend = {"ok": True, "actualDurationSeconds": 60, "minimalSoakLogging": False,
                    "totalFileSwitches": 7, "totalViewSwitches": 12,
                    "totalScrolls": 33, "totalJumps": 6}
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            out_path = root / "run.json"
            # config.outputPath is REVIA_MEASURE_LOG, i.e. the frontend report path.
            anchor = {"kind": "run", "runId": "r1",
                      "outputPath": str(out_path.resolve()) + ".frontend.json", "targetSeconds": 60}
            log_text = f'[SOAK_PROGRESS] {json.dumps(anchor)}\n'
            code, out = self._run_production_main(directory, frontend, log_text)
            report = json.loads(out.read_text())
            self.assertEqual(code, 0, report["errors"])
            self.assertTrue(report["runCompleted"])
            self.assertEqual(report["progressEvidence"]["problems"], [])


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
