#!/usr/bin/env python3
"""macOS release soak driver. A completed run still requires desktop/trend review."""

import argparse
from collections import deque
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import subprocess
import sys
import tempfile
import time


ROLES = {"WebContent": "webContent", "Networking": "networking", "GPU": "gpu"}
PHASE_HOLD_SECONDS = 3.5
# Q98: pressure checkpoints hold 16s; post WebKit's low-memory notification, sample about 10s later.
PRESSURE_HOLD_SECONDS = 16.0
PRESSURE_SETTLE_SECONDS = 10.0
Q98_WARMUP_SECONDS = 180
Q98_MAX_RISE_MB = 10.0
Q98_MAX_SLOPE_MB_PER_MINUTE = 0.5
Q98_MIN_PRESSURE_SAMPLES = 3
# Single-operation diagnostics: which frontend counter proves the focused load actually ran.
FOCUS_COUNTERS = {"idle": (), "ipc": ("totalIpcRequests",), "highlight": ("totalIpcRequests",), "channel": ("totalYieldedTasks",), "open": ("totalFileSwitches",),
                  "remount": ("totalFileSwitches",), "switch": ("totalFileSwitches",),
                  "scroll": ("totalScrolls",), "jump": ("totalJumps",), "view": ("totalViewSwitches",)}
FULL_LOAD_COUNTERS = ("totalFileSwitches", "totalViewSwitches", "totalScrolls", "totalJumps")


class CommandEvidence:
    """Per-run append-only record of external sampling commands.

    Every invocation records argv, purpose, timing, exit code or exception, and the raw
    stdout/stderr actually captured — including whatever was already read when a timeout
    fired. Nothing is retried or replaced here: a failure stays a failure, and callers
    decide what to do with it.
    """

    def __init__(self, path):
        self.path = path

    def record(self, entry):
        with self.path.open("a") as journal:
            journal.write(json.dumps(entry, ensure_ascii=False) + "\n")

    def run(self, argv, purpose, timeout):
        """Run one external command under the audited boundary and record the outcome.

        Raises subprocess.CalledProcessError on a non-zero exit and TimeoutExpired on a
        timeout, after the record is written; partial output is preserved for both.
        """
        started = datetime.datetime.now().astimezone().isoformat()
        begin = time.monotonic()
        try:
            completed = subprocess.run(argv, capture_output=True, text=True, timeout=timeout)
        except subprocess.TimeoutExpired as error:
            self.record({"kind": "command", "purpose": purpose, "argv": list(argv),
                         "startedAt": started,
                         "endedAt": datetime.datetime.now().astimezone().isoformat(),
                         "durationSeconds": round(time.monotonic() - begin, 3),
                         "exception": "TimeoutExpired", "timeoutSeconds": timeout,
                         "partialStdout": _as_text(error.output),
                         "partialStderr": _as_text(error.stderr)})
            raise
        except OSError as error:
            self.record({"kind": "command", "purpose": purpose, "argv": list(argv),
                         "startedAt": started,
                         "endedAt": datetime.datetime.now().astimezone().isoformat(),
                         "durationSeconds": round(time.monotonic() - begin, 3),
                         "exception": type(error).__name__, "message": str(error)})
            raise
        self.record({"kind": "command", "purpose": purpose, "argv": list(argv),
                     "startedAt": started,
                     "endedAt": datetime.datetime.now().astimezone().isoformat(),
                     "durationSeconds": round(time.monotonic() - begin, 3),
                     "returncode": completed.returncode,
                     "stdout": completed.stdout, "stderr": completed.stderr})
        completed.check_returncode()
        return completed.stdout


def _as_text(value):
    if isinstance(value, str):
        return value
    return (value or b"").decode("utf-8", "replace")


def webkit_services(domain):
    """Read only this application's launchd service table, never a global PID diff."""
    section = re.search(r"\n\s*services = \{\n(.*?)\n\s*\}", domain, re.S)
    result = {role: [] for role in ROLES}
    if section:
        for line in section[1].splitlines():
            match = re.fullmatch(r"\s*(\d+)\s+\S+\s+com\.apple\.WebKit\.(WebContent|Networking|GPU)(?:\.[\w-]+)?\s*", line)
            if match and int(match[1]) > 0:
                result[match[2]].append(int(match[1]))
    return result


def launchctl_ownership(evidence, pid, purpose):
    """Run `launchctl print pid/<pid>` and record ONLY the WebKit service rows.

    launchctl's full stdout includes the process environment, which can contain
    credentials, so the raw stdout is never persisted. The returned text is used
    in-process for parsing only. Returns None when launchctl itself failed or timed
    out; callers must treat that as missing ownership evidence, never as success.
    """
    started = datetime.datetime.now().astimezone().isoformat()
    try:
        completed = subprocess.run(["launchctl", "print", f"pid/{pid}"], capture_output=True,
                                   text=True, timeout=5)
    except subprocess.TimeoutExpired:
        evidence.record({"kind": "ownership", "purpose": purpose, "pid": pid, "startedAt": started,
                         "endedAt": datetime.datetime.now().astimezone().isoformat(),
                         "exception": "TimeoutExpired", "timeoutSeconds": 5,
                         "redaction": "launchctl stdout may contain process environment (possible credentials); "
                                      "nothing from stdout retained"})
        return None
    section = re.search(r"\n\s*services = \{\n(.*?)\n\s*\}", completed.stdout, re.S)
    retained = [" ".join(line.split()) for line in (section[1].splitlines() if section else [])
                if "com.apple.WebKit." in line]
    evidence.record({"kind": "ownership", "purpose": purpose, "pid": pid, "startedAt": started,
                     "endedAt": datetime.datetime.now().astimezone().isoformat(),
                     "returncode": completed.returncode,
                     "webkitServiceRows": retained,
                     "services": webkit_services(completed.stdout),
                     "redaction": "launchctl stdout may contain process environment (possible credentials); "
                                  "only WebKit service rows retained, not the raw stdout"})
    if completed.returncode != 0:
        return None
    return completed.stdout


def sample_memory(pid, evidence=None):
    if evidence is None:
        domain = subprocess.check_output(["launchctl", "print", f"pid/{pid}"], text=True, timeout=5)
    else:
        # Ownership evidence keeps only WebKit service rows; the raw stdout may contain secrets.
        domain = launchctl_ownership(evidence, pid, "rss-sample")
        if domain is None:
            raise RuntimeError("launchctl 归属证据不可用，不能接受缺失的内存样本")
    services = webkit_services(domain)
    pids = [pid] + [child for group in services.values() for child in group]
    if evidence is None:
        output = subprocess.check_output(["ps", "-o", "pid=,rss=", "-p", ",".join(map(str, pids))], text=True, timeout=5)
    else:
        output = evidence.run(["ps", "-o", "pid=,rss=", "-p", ",".join(map(str, pids))], "rss-sample", 5)
    rss = {int(row[0]): int(row[1]) / 1024 for line in output.splitlines() if len(row := line.split()) == 2}
    if any(child not in rss for child in pids) or any(not services[role] for role in ROLES):
        raise RuntimeError("Revia/WebKit 进程不完整，不能接受缺失的内存样本")
    values = {"rustRssMb": round(rss[pid], 2)}
    for role, children in services.items():
        key = ROLES[role]
        values[key + "RssMb"] = round(sum(rss[child] for child in children), 2)
        values[key + "Pids"] = children
    values["totalRssMb"] = round(sum(rss.values()), 2)
    # Preserve ownership evidence without copying launchd's environment (possibly secrets).
    values["serviceDomain"] = f"pid/{pid}"
    return values


def physical_footprint(pid, evidence=None):
    if evidence is None:
        output = subprocess.check_output(["vmmap", "-summary", str(pid)], text=True, timeout=10)
    else:
        output = evidence.run(["vmmap", "-summary", str(pid)], "rust-footprint", 10)
    return parse_footprint(output)


def parse_footprint(output):
    match = re.search(r"^Physical footprint:\s+([0-9.]+)([KMGT]?)\s*$", output, re.M)
    if not match:
        raise RuntimeError("vmmap 缺少 Physical footprint，不能以 RSS 代替")
    scale = {"": 1 / 1048576, "K": 1 / 1024, "M": 1, "G": 1024, "T": 1048576}
    return float(match[1]) * scale[match[2]]


def parse_webkit_allocated(output):
    lines = output.splitlines()
    headers = [i for i, line in enumerate(lines) if line.lstrip().startswith("MALLOC ZONE")]
    if len(headers) != 1:
        raise RuntimeError("vmmap 缺少唯一的 MALLOC ZONE 表")
    index = headers[0]
    upper = "VIRTUAL RESIDENT DIRTY SWAPPED ALLOCATION BYTES DIRTY+SWAP REGION"
    lower = "MALLOC ZONE SIZE SIZE SIZE SIZE COUNT ALLOCATED FRAG SIZE % FRAG COUNT"
    if index == 0 or " ".join(lines[index - 1].split()) != upper or " ".join(lines[index].split()) != lower:
        raise RuntimeError("vmmap MALLOC ZONE 列定义改变，不能猜测 BYTES ALLOCATED")

    def megabytes(value):
        match = re.fullmatch(r"([0-9]+(?:\.[0-9]+)?)([KMGT]?)", value)
        if not match:
            raise RuntimeError(f"vmmap 内存单位无效：{value}")
        scale = {"": 1 / 1048576, "K": 1 / 1024, "M": 1, "G": 1024, "T": 1048576}
        return float(match[1]) * scale[match[2]]

    zones = {}
    for line in lines[index + 2:]:
        if line.strip().startswith(("===", "TOTAL")):
            break
        if not line.lstrip().startswith("WebKit Malloc_"):
            continue
        match = re.fullmatch(r"\s*(WebKit Malloc_0x[0-9a-fA-F]+)\s+(.+?)\s*", line)
        fields = match[2].split() if match else []
        if (len(fields) != 9 or not fields[4].isdigit() or not fields[8].isdigit()
                or not re.fullmatch(r"[0-9]+(?:\.[0-9]+)?%", fields[7])):
            raise RuntimeError("vmmap WebKit Malloc_ 行列数或格式无效")
        for column in (0, 1, 2, 3, 5, 6):
            megabytes(fields[column])
        if match[1] in zones:
            raise RuntimeError("vmmap 出现重复 WebKit Malloc_ 区域")
        zones[match[1]] = megabytes(fields[5])
    if not zones:
        raise RuntimeError("vmmap MALLOC ZONE 缺少 WebKit Malloc_ 的 BYTES ALLOCATED")
    return sum(zones.values())


def webcontent_memory(pid, evidence=None):
    if evidence is None:
        output = subprocess.check_output(["vmmap", "-summary", str(pid)], text=True, timeout=10)
    else:
        output = evidence.run(["vmmap", "-summary", str(pid)], "webcontent-memory", 10)
    return {"pid": pid, "footprintMb": parse_footprint(output),
            "webkitMallocAllocatedMb": parse_webkit_allocated(output)}


class ProgressLog:
    """Read the frontend's incremental `[SOAK_PROGRESS] {json}` records from the run log.

    Purpose: explain an interrupted run and cross-check phase windows. These records are
    DIAGNOSTIC. They never substitute for the final frontend report, and they never make a
    phase sample authoritative for acceptance.

    Strictness: a trailing partial line is kept verbatim in `tail` and never parsed as
    progress. The run anchor must match this driver invocation (output path and target
    duration); records must share one anchor, carry unique integer cycles, and be closed.
    """

    def __init__(self, path):
        self.path = path

    def read(self, expected_output=None, expected_target=None):
        """Return (anchor, closed_checkpoints, problems, tail).

        `problems` is a list of integrity violations; any problem must fail the run.
        `tail` is the raw unterminated remainder, preserved for evidence.
        """
        try:
            data = self.path.read_bytes()
        except FileNotFoundError:
            return None, [], ["缺少前端进度记录：日志文件不存在"], ""
        head, separator, tail = data.rpartition(b"\n")
        if not separator:
            head, tail = b"", data
        anchor, closed, problems = None, [], []
        seen = set()
        for raw in head.split(b"\n") if head else []:
            text = raw.decode("utf-8", "replace")
            marker = text.find("[SOAK_PROGRESS] ")
            if marker < 0:
                continue
            payload = text[marker + len("[SOAK_PROGRESS] "):]
            try:
                record = json.loads(payload)
            except ValueError:
                problems.append(f"进度记录不是合法 JSON：{payload[:120]}")
                continue
            if not isinstance(record, dict):
                problems.append("进度记录不是对象")
                continue
            kind = record.get("kind")
            if kind == "run":
                if anchor is None:
                    anchor = record
                    if expected_output is not None and record.get("outputPath") != expected_output:
                        problems.append(
                            f"运行锚点输出路径不匹配：记录 {record.get('outputPath')!r}，本轮 {expected_output!r}")
                    if expected_target is not None and record.get("targetSeconds") != expected_target:
                        problems.append(
                            f"运行锚点目标时长不匹配：记录 {record.get('targetSeconds')!r}，本轮 {expected_target!r}")
                elif record.get("runId") != anchor.get("runId"):
                    problems.append("进度记录出现第二个不同运行锚点")
                continue
            if kind != "checkpoint":
                problems.append(f"未知进度记录类型：{kind!r}")
                continue
            if anchor is None:
                problems.append("checkpoint 记录早于运行锚点")
                continue
            if record.get("runId") != anchor.get("runId"):
                problems.append("checkpoint 与运行锚点不属于同一运行")
                continue
            cycle = record.get("cycle")
            if not isinstance(cycle, int):
                problems.append(f"checkpoint 缺少整数 cycle：{cycle!r}")
                continue
            if cycle in seen:
                problems.append(f"checkpoint 重复：{cycle}")
                continue
            if not record.get("endedAt"):
                problems.append(f"checkpoint 未闭合：{cycle}")
                continue
            seen.add(cycle)
            closed.append(record)
        if anchor is None:
            problems.append("进度记录缺少运行锚点，不能确认属于本轮运行")
        return anchor, closed, problems, tail.decode("utf-8", "replace")


class CheckpointLog:
    """Follow complete log lines by byte offset; never resample a checkpoint ID."""

    def __init__(self, path):
        self.path = path
        self.pending = deque()
        self._offset = 0
        self._tail = b""
        self._identity = None
        self._seen = set()
        self._ended = set()
        self._generation = 0
        self._active = None

    def poll(self):
        try:
            with self.path.open("rb") as log:
                stat = os.fstat(log.fileno())
                identity = (stat.st_dev, stat.st_ino)
                if self._identity is not None and (identity != self._identity or stat.st_size < self._offset):
                    raise RuntimeError("阶段日志被替换或截断，不能继续使用旧边界")
                self._identity = identity
                log.seek(self._offset)
                content = log.read(stat.st_size - self._offset)
                self._offset += len(content)
        except FileNotFoundError:
            if self._identity is not None:
                raise RuntimeError("已读取的阶段日志消失") from None
            return
        lines = (self._tail + content).split(b"\n")
        self._tail = lines.pop()
        for line in lines:
            match = re.fullmatch(rb"(?:\S+\s+)?\[SOAK_CHECKPOINT_(BEGIN|END)\]\s+(\d+)(\s+pressure)?\s*", line)
            if not match:
                continue
            checkpoint_id = int(match[2])
            if match[1] == b"END":
                self._ended.add(checkpoint_id)
                if self._active and self._active[0] == checkpoint_id:
                    self._active = None
                continue
            self._generation += 1
            self._active = (checkpoint_id, self._generation)
            if checkpoint_id not in self._seen:
                self._seen.add(checkpoint_id)
                self.pending.append({"checkpointId": checkpoint_id, "generation": self._generation,
                                     "pressure": match[3] is not None, "observedAt": time.monotonic()})

    def invalid_reasons(self, checkpoint):
        reasons = []
        if checkpoint["checkpointId"] in self._ended:
            reasons.append("checkpoint END 已出现")
        if self._active != (checkpoint["checkpointId"], checkpoint["generation"]):
            reasons.append("checkpoint 已结束或出现新 BEGIN")
        if b"[SOAK_CHECKPOINT_" in self._tail:
            reasons.append("阶段边界日志尚未完整写入")
        return reasons

    def has_ended(self, checkpoint_id):
        return checkpoint_id in self._ended


def sample_phase_memory(proc, checkpoints, checkpoint, expected_services, started, evidence=None):
    beginning = time.monotonic()
    pressure = checkpoint["pressure"]
    hold = PRESSURE_HOLD_SECONDS if pressure else PHASE_HOLD_SECONDS
    sample = {"checkpointId": checkpoint["checkpointId"], "phase": "staged/split/settled", "pressure": pressure,
              "checkpointObservedElapsedSec": round(checkpoint["observedAt"] - started, 3),
              "samplingStartedElapsedSec": round(beginning - started, 3),
              "timestamp": datetime.datetime.now().astimezone().isoformat(), "valid": False}
    errors = checkpoints.invalid_reasons(checkpoint)
    try:
        if proc.poll() is not None:
            errors.append("采样前应用已退出")
        if pressure and not errors:
            if evidence is None:
                subprocess.run(["notifyutil", "-p", "org.WebKit.lowMemory"], check=True, timeout=5)
            else:
                evidence.run(["notifyutil", "-p", "org.WebKit.lowMemory"], "low-memory-notification", 5)
            posted = time.monotonic()
            sample["lowMemoryPostedElapsedSec"] = round(posted - started, 3)
            time.sleep(max(0.0, posted + PRESSURE_SETTLE_SECONDS - time.monotonic()))
            sample["timestamp"] = datetime.datetime.now().astimezone().isoformat()
            checkpoints.poll()
            errors.extend(checkpoints.invalid_reasons(checkpoint))
            if proc.poll() is not None:
                errors.append("低内存通知后应用已退出")
        if not errors:
            sample.update(sample_memory(proc.pid, evidence))
            services = {role: sample[ROLES[role] + "Pids"] for role in ROLES}
            if expected_services is not None and services != expected_services:
                raise RuntimeError("WebKit 进程与常规采样归属不一致")
            sample["rustFootprintMb"] = physical_footprint(proc.pid, evidence)
            sample["webContentVmmap"] = [webcontent_memory(pid, evidence) for pid in services["WebContent"]]
            sample["webContentFootprintMb"] = sum(item["footprintMb"] for item in sample["webContentVmmap"])
            sample["webContentMallocAllocatedMb"] = sum(item["webkitMallocAllocatedMb"] for item in sample["webContentVmmap"])
            after = sample_memory(proc.pid, evidence)
            sample["servicesAfterSampling"] = {role: after[ROLES[role] + "Pids"] for role in ROLES}
            if sample["servicesAfterSampling"] != services:
                raise RuntimeError("阶段采样期间 WebKit 进程归属改变")
    except Exception as error:
        errors.append(f"{type(error).__name__}: {error}")
    finally:
        ended = time.monotonic()
        sample.update(samplingEndedElapsedSec=round(ended - started, 3),
                      samplingDurationSeconds=round(ended - beginning, 3),
                      samplingEndedAt=datetime.datetime.now().astimezone().isoformat())
        try:
            checkpoints.poll()
            errors.extend(checkpoints.invalid_reasons(checkpoint))
        except (OSError, RuntimeError) as error:
            errors.append(f"阶段结束边界不可核验：{error}")
        if proc.poll() is not None:
            errors.append("阶段采样结束时应用已退出")
        if ended - checkpoint["observedAt"] >= hold:
            errors.append(f"阶段采样超过固定 {hold * 1000:.0f}ms 窗口")
        sample["errors"] = list(dict.fromkeys(errors))
        sample["samplingChecksPassed"] = not errors
        sample["validation"] = "pending checkpoint closure and frontend interval review"
    return sample


def finalize_phase_samples(samples, checkpoints, frontend, progress_checkpoints=None):
    """Validate phase samples. Acceptance metadata comes ONLY from the final report.

    A final report that omits `phaseCheckpoints` (or reports an empty list) is a failure:
    the frontend's incremental progress records may only fill DIAGNOSTIC metadata, and
    such samples are never authoritative, so they cannot contribute acceptance samples.
    """
    def instant(value):
        result = datetime.datetime.fromisoformat(value.replace("Z", "+00:00"))
        if result.tzinfo is None:
            raise ValueError("阶段时间缺少时区")
        return result

    reported = frontend.get("phaseCheckpoints")
    has_final = isinstance(reported, list) and bool(reported)
    if has_final:
        source = "final-report"
    elif progress_checkpoints:
        source = "progress-log"
    else:
        source = "missing"

    for sample in samples:
        errors = sample["errors"]
        sample["checkpointSource"] = source
        sample["authoritative"] = source == "final-report"
        sample["checkpointEndConfirmed"] = checkpoints.has_ended(sample["checkpointId"])
        if not sample["checkpointEndConfirmed"]:
            errors.append("缺少匹配的 checkpoint END，不能确认静置窗口闭合")
        if source == "missing":
            errors.append("缺少最终前端报告的阶段 checkpoint，不能作为验收证据")
            sample["errors"] = list(dict.fromkeys(errors))
            sample["valid"] = False
            sample["validation"] = "final"
            continue
        matches = [item for item in reported if isinstance(item, dict) and item.get("cycle") == sample["checkpointId"]] \
            if isinstance(reported, list) else []
        if len(matches) != 1 or matches[0].get("file") != "src/staged.ts" or matches[0].get("view") != "split":
            errors.append("前端报告缺少唯一的 staged/split checkpoint")
        elif bool(matches[0].get("pressure")) != bool(sample.get("pressure")):
            errors.append("前端与驱动的压力阶段标记不一致")
        elif matches[0].get("hiddenDuringHold"):
            errors.append("阶段静置期间窗口被遮挡")
        else:
            try:
                if not (instant(matches[0]["startedAt"]) <= instant(sample["timestamp"])
                        <= instant(sample["samplingEndedAt"]) <= instant(matches[0]["endedAt"])):
                    errors.append("采样墙钟区间不在前端 checkpoint 起止区间内")
            except (KeyError, TypeError, ValueError, AttributeError) as error:
                errors.append(f"前端 checkpoint 起止时间不可核验：{error}")
        sample["errors"] = list(dict.fromkeys(errors))
        sample["valid"] = not sample["errors"]
        sample["validation"] = "final"


def progress_consistency_errors(frontend, progress_checkpoints):
    """When both a final report and progress records exist, they must describe the same run.

    Progress is diagnostic, but a passing run whose incremental records contradict the
    final report is not trustworthy evidence, so the mismatch fails the run.
    """
    reported = frontend.get("phaseCheckpoints")
    if not isinstance(reported, list) or not reported or not progress_checkpoints:
        return []
    final_cycles = {item.get("cycle") for item in reported if isinstance(item, dict)}
    progress_cycles = {item.get("cycle") for item in progress_checkpoints}
    problems = []
    if progress_cycles - final_cycles:
        problems.append(f"进度记录包含最终报告没有的 checkpoint：{sorted(progress_cycles - final_cycles)}")
    if final_cycles - progress_cycles:
        problems.append(f"最终报告包含进度记录没有的 checkpoint：{sorted(final_cycles - progress_cycles)}")
    final_by_cycle = {item.get("cycle"): item for item in reported if isinstance(item, dict)}
    for record in progress_checkpoints:
        final = final_by_cycle.get(record.get("cycle"))
        if not final:
            continue
        if bool(final.get("pressure")) != bool(record.get("pressure")):
            problems.append(f"checkpoint {record.get('cycle')} 的压力标记在最终报告与进度记录间不一致")
        if final.get("hiddenDuringHold") != record.get("hiddenDuringHold"):
            problems.append(f"checkpoint {record.get('cycle')} 的遮挡状态在最终报告与进度记录间不一致")
    return problems


def record_phase_samples(checkpoints, proc, services, started, samples, journal, evidence=None):
    while checkpoints.pending:
        sample = sample_phase_memory(proc, checkpoints, checkpoints.pending.popleft(), services, started, evidence)
        samples.append(sample)
        journal.write(json.dumps(sample) + "\n")
        journal.flush()
        detail = (f" WebContent {sample['webContentFootprintMb']:.1f} MB / malloc {sample['webContentMallocAllocatedMb']:.1f} MB"
                  if "webContentFootprintMb" in sample else "")
        print(f"[phase {sample['checkpointId']}{' pressure' if sample.get('pressure') else ''}] "
              f"samplingChecksPassed={sample['samplingChecksPassed']} "
              f"duration={sample['samplingDurationSeconds']:.3f}s{detail}; pending closure", flush=True)


def analyze_memory(samples, start, warmup=180):
    steady = [s for s in samples if s["elapsedSec"] >= start + warmup]
    result = {"warmupSeconds": warmup, "steadySampleCount": len(steady), "series": {}}
    for key in ("rustRssMb", "webContentRssMb", "networkingRssMb", "gpuRssMb", "totalRssMb", "rustFootprintMb", "webContentFootprintMb"):
        selected = [s for s in steady if key in s]
        if len(selected) < 2:
            continue
        xs, ys = [s["elapsedSec"] / 60 for s in selected], [s[key] for s in selected]
        mx, my = sum(xs) / len(xs), sum(ys) / len(ys)
        denominator = sum((x - mx) ** 2 for x in xs)
        bins = []
        for begin in range(0, len(selected), max(1, len(selected) // 6)):
            group = selected[begin:begin + max(1, len(selected) // 6)]
            bins.append({"fromSec": group[0]["elapsedSec"], "toSec": group[-1]["elapsedSec"],
                         "meanMb": round(sum(s[key] for s in group) / len(group), 2)})
        result["series"][key] = {"firstMb": ys[0], "lastMb": ys[-1], "minMb": min(ys), "maxMb": max(ys),
            "regressionMbPerMinute": round(sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / denominator, 4) if denominator else None,
            "windows": bins}
    return result


def q98_series(points):
    """Q98: every post-warmup sample within first + 10 MiB, and linear trend within 0.5 MiB/min."""
    result = {"sampleCount": len(points)}
    if len(points) < 2:
        return {**result, "passed": False, "reason": "热身后样本不足 2 个，无法判定趋势"}
    xs, ys = [x / 60 for x, _ in points], [y for _, y in points]
    mx, my = sum(xs) / len(xs), sum(ys) / len(ys)
    denominator = sum((x - mx) ** 2 for x in xs)
    slope = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / denominator if denominator else 0.0
    max_rise = max(ys) - ys[0]
    return {**result, "points": [{"elapsedSec": round(x, 2), "mb": round(y, 2)} for x, y in points],
            "firstMb": round(ys[0], 2), "lastMb": round(ys[-1], 2), "maxMb": round(max(ys), 2),
            "maxRiseMb": round(max_rise, 2), "regressionMbPerMinute": round(slope, 4),
            "passed": max_rise <= Q98_MAX_RISE_MB and slope <= Q98_MAX_SLOPE_MB_PER_MINUTE}


def analyze_q98(samples, phase_samples, start):
    cutoff = start + Q98_WARMUP_SECONDS
    # Only samples whose metadata came from the real final report may count for acceptance.
    pressure = [s for s in phase_samples
                if s.get("pressure") and s["valid"] and s.get("authoritative", False)
                and s["samplingStartedElapsedSec"] >= cutoff]
    regular = [s for s in samples if s["elapsedSec"] >= cutoff and "rustFootprintMb" in s]
    series = {
        "webContentFootprintMb": q98_series([(s["samplingStartedElapsedSec"], s["webContentFootprintMb"]) for s in pressure]),
        "webContentMallocAllocatedMb": q98_series(
            [(s["samplingStartedElapsedSec"], s["webContentMallocAllocatedMb"]) for s in pressure]),
        "rustFootprintMb": q98_series([(s["elapsedSec"], s["rustFootprintMb"]) for s in regular]),
    }
    errors = [f"{key}：{value.get('reason', '超过 Q98 阈值')}" for key, value in series.items() if not value["passed"]]
    if len(pressure) < Q98_MIN_PRESSURE_SAMPLES:
        errors.append(f"热身后有效压力后样本 {len(pressure)} 个，少于 {Q98_MIN_PRESSURE_SAMPLES} 个")
    return {"rule": "Q98", "warmupSeconds": Q98_WARMUP_SECONDS, "maxRiseMb": Q98_MAX_RISE_MB,
            "maxSlopeMbPerMinute": Q98_MAX_SLOPE_MB_PER_MINUTE, "pressureSettleSeconds": PRESSURE_SETTLE_SECONDS,
            "pressureSource": "notifyutil -p org.WebKit.lowMemory", "pressureSampleCount": len(pressure),
            "series": series, "errors": errors, "passed": not errors}


def completion_errors(frontend, samples, seconds, returncode, diagnostic=None, minimal_logging=None):
    errors = []
    if returncode != 0 or frontend.get("ok") is not True:
        errors.append("应用未成功完成：" + str(frontend.get("error", returncode)))
    if minimal_logging is not None and frontend.get("minimalSoakLogging") is not minimal_logging:
        errors.append("前端日志模式不匹配")
    if minimal_logging and not isinstance(frontend.get("ipcFinal"), dict):
        errors.append("最小日志基线缺少 IPC 观察证据")
    if diagnostic is None and frontend.get("diagnostic") is not None:
        errors.append("诊断结果不能作为长期运行基线")
    if diagnostic is None or diagnostic.get("cycles") is None:
        if frontend.get("actualDurationSeconds", 0) < seconds:
            errors.append("前端连续操作时长不足")
    if diagnostic is not None:
        if frontend.get("diagnostic") != diagnostic:
            errors.append("前端诊断配置不匹配")
        cycles = diagnostic.get("cycles")
        if cycles is not None and (frontend.get("totalCycles") != cycles
                or frontend.get("totalFileSwitches") != cycles * len(frontend.get("files", []))):
            errors.append("固定循环诊断负载未完成")
        if not isinstance(frontend.get("ipcFinal"), dict):
            errors.append("缺少 IPC 诊断结果")
        idle = frontend.get("idle", {})
        if idle.get("actualSeconds", -1) < diagnostic["idleSeconds"]:
            errors.append("诊断静置时长不足")
    if not samples or any(not s.get("webContentPids") for s in samples):
        errors.append("缺少已归属的 WebContent 内存数据")
    focus = diagnostic.get("focus") if diagnostic else None
    if any(frontend.get(key, 0) <= 0 for key in (FOCUS_COUNTERS[focus] if focus else FULL_LOAD_COUNTERS)):
        errors.append("缺少必需的操作负载")
    return errors


def terminate_and_record(proc, evidence, cleanup, services=None):
    """Terminate the measured app and record every step, even when a call raises.

    The record distinguishes three things that must not be conflated:
      * the intent/attempt to call terminate()/kill() (`terminateAttempted`, `killAttempted`)
      * whether that call returned without raising (`terminateReturned`, `killReturned`)
      * whether the signal was actually delivered — which cannot be observed here and is
        therefore reported as unknown, never asserted, and never inferred from exit code.
    `proc.wait()` results and any exception are recorded, and the final poll is always
    captured, so a raising wait() cannot destroy the cleanup evidence.
    """
    cleanup["startedAt"] = datetime.datetime.now().astimezone().isoformat()
    cleanup["reviaPid"] = proc.pid
    cleanup["servicesBeforeCleanup"] = services
    poll_before = proc.poll()
    cleanup["pollBeforeCleanup"] = poll_before
    if poll_before is not None:
        cleanup.update({"terminateAttempted": False, "terminateReturned": False,
                        "killAttempted": False, "killReturned": False,
                        "signalDelivery": "not_attempted",
                        "note": "清理前主进程已退出；未发送信号"})
    else:
        cleanup["terminateAttempted"] = True
        cleanup["terminateAttemptedAt"] = datetime.datetime.now().astimezone().isoformat()
        try:
            proc.terminate()
            cleanup["terminateReturned"] = True
            cleanup["signalDelivery"] = "unknown"
        except Exception as error:  # noqa: BLE001 - record, never mask
            cleanup["terminateReturned"] = False
            cleanup["terminateError"] = f"{type(error).__name__}: {error}"
            cleanup["signalDelivery"] = "unknown"
        try:
            cleanup["waitAfterTerminate"] = proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            cleanup["waitAfterTerminate"] = "timeout"
            cleanup["killAttempted"] = True
            cleanup["killAttemptedAt"] = datetime.datetime.now().astimezone().isoformat()
            try:
                proc.kill()
                cleanup["killReturned"] = True
            except Exception as error:  # noqa: BLE001 - record, never mask
                cleanup["killReturned"] = False
                cleanup["killError"] = f"{type(error).__name__}: {error}"
            try:
                cleanup["waitAfterKill"] = proc.wait()
            except Exception as error:  # noqa: BLE001 - record, never mask
                cleanup["waitAfterKill"] = "error"
                cleanup["waitAfterKillError"] = f"{type(error).__name__}: {error}"
        except Exception as error:  # noqa: BLE001 - record, never mask
            cleanup["waitAfterTerminate"] = "error"
            cleanup["waitAfterTerminateError"] = f"{type(error).__name__}: {error}"
    try:
        cleanup["pollAfterCleanup"] = proc.poll()
    except Exception as error:  # noqa: BLE001 - record, never mask
        cleanup["pollAfterCleanup"] = "error"
        cleanup["pollAfterCleanupError"] = f"{type(error).__name__}: {error}"
    cleanup["endedAt"] = datetime.datetime.now().astimezone().isoformat()
    evidence.record({"kind": "cleanup", **cleanup})
    return cleanup


def preserve_partial_candidate(candidate, sample, error, proc, services, evidence, partials):
    """Keep everything a failing sampling candidate already obtained.

    Any exception type (subprocess errors, parse RuntimeErrors, timeouts) must preserve
    the values read so far plus whatever process state can still be observed.
    """
    if sample:
        for key, value in sample.items():
            candidate.setdefault(key, value)
    candidate["failure"] = f"{type(error).__name__}: {error}"
    candidate["reviaPid"] = getattr(proc, "pid", None)
    try:
        candidate["pollAtFailure"] = proc.poll()
    except Exception as poll_error:  # noqa: BLE001 - record, never mask
        candidate["pollAtFailure"] = "error"
        candidate["pollAtFailureError"] = f"{type(poll_error).__name__}: {poll_error}"
    candidate["servicesAtFailure"] = services
    candidate["capturedAt"] = datetime.datetime.now().astimezone().isoformat()
    partials.append(candidate)
    evidence.record({"kind": "sample-failure", **candidate})
    return candidate


def binary_fingerprint(binary):
    return hashlib.sha256(binary.read_bytes()).hexdigest()


def binary_unchanged(binary, expected):
    try:
        return binary_fingerprint(binary) == expected
    except OSError:
        return False


def main():
    parser = argparse.ArgumentParser(description="Revia macOS 长期运行基线；请保持受测窗口可见")
    parser.add_argument("--seconds", type=int)
    parser.add_argument("--minutes", type=int, default=30)
    parser.add_argument("--app", default="src-tauri/target/release/bundle/macos/Revia.app")
    parser.add_argument("--repo", default="/tmp/revia-m1-fixtures/diffs")
    parser.add_argument("--output", default=f"/tmp/revia-soak-{datetime.datetime.now():%Y%m%d-%H%M%S}.json")
    parser.add_argument("--interval", type=int, default=10)
    parser.add_argument("--baseline-minimal-logging", action="store_true",
                        help="正式持续时长基线只保留关键日志，同时观察 IPC；不启用固定循环或静置")
    parser.add_argument("--diagnostic-logging", choices=("verbose", "minimal"),
                        help="显式启用 IPC/内存短诊断；结果不算长期运行验收")
    parser.add_argument("--idle-seconds", type=int, default=90)
    parser.add_argument("--keep-on-top", action="store_true",
                        help="受测窗口保持在最前并出现在所有桌面空间（仅诊断用，避免被遮挡中断）")
    parser.add_argument("--focus", choices=tuple(FOCUS_COUNTERS),
                        help="诊断只重复一类操作，约每 90 秒压力取样一次（需同时指定 --diagnostic-logging）")
    parser.add_argument("--diagnostic-cycles", type=int,
                        help="诊断固定完整循环数；seconds/minutes 此时只作为主动操作安全上限")
    args = parser.parse_args()
    seconds = args.seconds if args.seconds is not None else args.minutes * 60
    diagnostic = ({"loggingMode": args.diagnostic_logging, "idleSeconds": args.idle_seconds,
                   "cycles": args.diagnostic_cycles, "focus": args.focus} if args.diagnostic_logging else None)
    if args.baseline_minimal_logging and diagnostic:
        parser.error("正式基线和短诊断模式不能同时选择")
    minimal_logging = args.baseline_minimal_logging or args.diagnostic_logging == "minimal"
    if args.idle_seconds < 0 or (args.diagnostic_cycles is not None and args.diagnostic_cycles <= 0):
        parser.error("静置时长不能为负，固定循环数必须为正")
    if not diagnostic and (args.diagnostic_cycles is not None or args.idle_seconds != 90 or args.focus):
        parser.error("循环数、静置和单项参数仅适用于显式诊断模式")
    if args.focus and args.diagnostic_cycles is not None:
        parser.error("单项诊断按时长运行，不能指定固定循环数")
    app, repo, output = Path(args.app).resolve(), Path(args.repo).resolve(), Path(args.output).resolve()
    binary = app / "Contents/MacOS/revia"
    frontend_path, log_path, samples_path, phase_samples_path = (Path(str(output) + suffix) for suffix in (
        ".frontend.json", ".log", ".samples.jsonl", ".phase-samples.jsonl"))
    if platform.system() != "Darwin" or seconds <= 0 or args.interval <= 0:
        parser.error("此驱动仅适用于 macOS；时长与采样间隔必须为正")
    if not binary.is_file() or not repo.is_dir():
        parser.error("应用或 fixture 不存在，请先构建当前源码并准备 fixture")
    if any(p.exists() for p in (output, frontend_path, frontend_path.with_suffix(".log"), log_path, samples_path, phase_samples_path)):
        parser.error("输出已存在，请指定新的 --output，保留旧证据")
    output.parent.mkdir(parents=True, exist_ok=True)
    env = {key: value for key, value in os.environ.items() if not key.startswith("REVIA_")}
    data_dir = tempfile.mkdtemp(prefix="revia-soak-data-")
    env.update(REVIA_SOAK_SECONDS=str(seconds), REVIA_MEASURE_DATA_DIR=data_dir,
               REVIA_MEASURE_REPO=str(repo), REVIA_MEASURE_LOG=str(frontend_path))
    if args.baseline_minimal_logging:
        env["REVIA_SOAK_MINIMAL_LOGGING"] = "1"
    if args.keep_on_top:
        env["REVIA_SOAK_ON_TOP"] = "1"
    if diagnostic:
        env.update(REVIA_SOAK_DIAG_LOGS=diagnostic["loggingMode"], REVIA_SOAK_IDLE_SECONDS=str(args.idle_seconds))
        if args.diagnostic_cycles is not None:
            env["REVIA_SOAK_DIAG_CYCLES"] = str(args.diagnostic_cycles)
        if args.focus:
            env["REVIA_SOAK_DIAG_FOCUS"] = args.focus
    binary_sha256 = binary_fingerprint(binary)
    started = time.monotonic()
    started_at = datetime.datetime.now().astimezone().isoformat()
    samples, phase_samples, errors, frontend = [], [], [], {}
    checkpoints = CheckpointLog(frontend_path.with_suffix(".log"))
    progress_log = ProgressLog(frontend_path.with_suffix(".log"))
    evidence_path = Path(str(output) + ".evidence.jsonl")
    evidence = CommandEvidence(evidence_path)
    cleanup_evidence = {}
    partial_candidates = []
    initial_services = None
    with log_path.open("x") as log, samples_path.open("x") as journal, phase_samples_path.open("x") as phase_journal:
        proc = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=subprocess.STDOUT)
        load = f"固定 {args.diagnostic_cycles} 循环（{seconds}s 安全上限）" if args.diagnostic_cycles else f"连续操作 {seconds} 秒"
        print(f"Revia PID={proc.pid}，等待可见窗口后{load}；诊断={diagnostic}；报告 {output}", flush=True)
        next_sample = started + 2
        try:
            while proc.poll() is None:
                checkpoints.poll()
                record_phase_samples(checkpoints, proc, initial_services, started, phase_samples, phase_journal, evidence)
                now = time.monotonic()
                # Allow 120s visibility setup, about 200s phase/pressure holds and an exit margin;
                # single-operation diagnostics may also pause up to 10 minutes per hidden-window interruption.
                if now - started > seconds + 480 + (args.idle_seconds if diagnostic else 0) + (1800 if args.focus else 0):
                    raise TimeoutError("等待可见窗口或操作硬超时")
                if now >= next_sample:
                    sampling_started_at = datetime.datetime.now().astimezone().isoformat()
                    candidate = {"elapsedSec": round(now - started, 2), "samplingStartedAt": sampling_started_at}
                    try:
                        sample = sample_memory(proc.pid, evidence)
                        candidate.update(sample)
                    except Exception as error:  # noqa: BLE001 - every failure keeps its partial data
                        preserve_partial_candidate(candidate, sample, error, proc, None,
                                                   evidence, partial_candidates)
                        if proc.poll() is not None:
                            break
                        if now - started < 15:
                            next_sample = now + 1
                            continue
                        raise RuntimeError(f"内存采样失败：{error}") from error
                    services = {role: sample[ROLES[role] + "Pids"] for role in ROLES}
                    if initial_services is not None and services != initial_services:
                        raise RuntimeError("WebKit 进程改变，本次不算同一 WebView 连续运行")
                    initial_services = services
                    if diagnostic or len(samples) % 3 == 0:
                        try:
                            sample["rustFootprintMb"] = physical_footprint(proc.pid, evidence)
                            if diagnostic:
                                sample["webContentVmmap"] = [webcontent_memory(child, evidence) for child in services["WebContent"]]
                                sample["webContentFootprintMb"] = sum(item["footprintMb"] for item in sample["webContentVmmap"])
                                sample["webContentMallocAllocatedMb"] = sum(item["webkitMallocAllocatedMb"] for item in sample["webContentVmmap"])
                                sample["networkingFootprintMb"] = sum(physical_footprint(child, evidence) for child in services["Networking"])
                            else:
                                sample["webContentFootprintMb"] = sum(physical_footprint(child, evidence) for child in services["WebContent"])
                        except Exception as error:  # noqa: BLE001 - every failure keeps its partial data
                            # Covers subprocess errors AND parse/format RuntimeErrors, so a
                            # vmmap that exits 0 with unusable output cannot discard what was read.
                            preserve_partial_candidate(candidate, sample, error, proc, services,
                                                       evidence, partial_candidates)
                            if proc.poll() is not None:
                                break
                            raise
                    sample.update(elapsedSec=round(now - started, 2), timestamp=datetime.datetime.now().astimezone().isoformat(),
                                  samplingStartedAt=sampling_started_at)
                    samples.append(sample)
                    journal.write(json.dumps(sample) + "\n")
                    journal.flush()
                    print(f"[{sample['elapsedSec']:7.1f}s] Rust {sample['rustRssMb']:.1f} MB / WebContent {sample['webContentRssMb']:.1f} MB", flush=True)
                    next_sample = now + args.interval
                time.sleep(0.2)
        except (Exception, KeyboardInterrupt) as error:
            errors.append(f"{type(error).__name__}: {error}")
        finally:
            # Every cleanup step is recorded, including exceptions from terminate/kill/wait.
            try:
                terminate_and_record(proc, evidence, cleanup_evidence, initial_services)
            except Exception as error:  # noqa: BLE001 - cleanup evidence must never vanish
                cleanup_evidence["cleanupError"] = f"{type(error).__name__}: {error}"
                try:
                    evidence.record({"kind": "cleanup", **cleanup_evidence})
                except Exception as record_error:  # noqa: BLE001 - last resort
                    errors.append(f"清理证据写入失败：{type(record_error).__name__}: {record_error}")
            try:
                checkpoints.poll()
                record_phase_samples(checkpoints, proc, initial_services, started, phase_samples, phase_journal, evidence)
            except (OSError, RuntimeError) as error:
                errors.append(f"最终阶段日志不可核验：{error}")
    if frontend_path.exists():
        try:
            frontend = json.loads(frontend_path.read_text())
        except (OSError, ValueError) as error:
            errors.append(f"前端报告无效：{error}")
    # Incremental frontend progress explains an interrupted run. It is diagnostics only:
    # it never satisfies completion, which still requires the real final report below.
    progress_anchor, progress_checkpoints, progress_problems, progress_tail = progress_log.read(
        expected_output=str(frontend_path), expected_target=seconds)
    finalize_phase_samples(phase_samples, checkpoints, frontend, progress_checkpoints)
    # The live journal is explicitly provisional; publish the same final validity as the report.
    phase_samples_path.write_text("".join(json.dumps(sample) + "\n" for sample in phase_samples))
    errors.extend(completion_errors(frontend, samples, seconds, proc.returncode, diagnostic, minimal_logging))
    # Incremental progress integrity is part of the success gate: corrupt, duplicated,
    # cross-run, unanchored or unclosed progress cannot accompany a passing run.
    errors.extend(f"前端进度记录问题：{problem}" for problem in progress_problems)
    errors.extend(f"前端进度与最终报告不一致：{problem}"
                  for problem in progress_consistency_errors(frontend, progress_checkpoints))
    if not binary_unchanged(binary, binary_sha256):
        errors.append("受测二进制在运行期间被替换或移除，构建证据无效")
    report = {
        "test": "ipc_memory_diagnostic" if diagnostic else "long_running_baseline", "runCompleted": not errors,
        "acceptance": ("diagnostic only; not baseline acceptance" if diagnostic
                       else "pending desktop evidence, performance regression and memory trend review"),
        "diagnostic": diagnostic, "minimalSoakLogging": minimal_logging,
        "startedAt": started_at, "endedAt": datetime.datetime.now().astimezone().isoformat(),
        "targetDurationSeconds": seconds, "actualDurationSeconds": round(time.monotonic() - started, 2),
        "reviaPid": proc.pid, "webkitAttribution": "launchctl print pid/<exact child PID>: services table",
        "binarySha256": binary_sha256,
        "platform": platform.platform(), "isolatedDataDir": data_dir, "repo": str(repo),
        "errors": errors, "frontend": frontend, "memoryTimeSeries": samples,
        "partialCandidates": partial_candidates,
        "commandEvidence": str(evidence_path),
        "cleanup": cleanup_evidence,
        "progressEvidence": {"anchor": progress_anchor, "closedCheckpoints": progress_checkpoints,
                             "problems": progress_problems, "unterminatedTail": progress_tail,
                             "note": "中断诊断用；不构成最终前端报告或验收依据"},
        "memoryAnalysis": (None if diagnostic else analyze_memory(samples, frontend.get("setupSeconds", 0))),
        "phaseMemoryTimeSeries": phase_samples,
        # Diagnostic runs report the same Q98 numbers for comparison; only baselines count for acceptance.
        "q98MemoryVerdict": analyze_q98(samples, phase_samples, frontend.get("setupSeconds", 0)),
        # A verdict computed from progress-derived phase samples is diagnostic only.
        "q98MemoryVerdictAuthoritative": bool(frontend.get("ok") is True)
        and isinstance(frontend.get("phaseCheckpoints"), list) and bool(frontend.get("phaseCheckpoints")),
        "phaseMemorySummary": {
            "sampleCount": len(phase_samples), "validSampleCount": sum(s["valid"] for s in phase_samples),
            "invalidSampleCount": sum(not s["valid"] for s in phase_samples),
            "allocationSource": "vmmap MALLOC ZONE / WebKit Malloc_* / BYTES ALLOCATED",
            "acceptance": ("pending manual same-phase sample sufficiency and trend review"
                           if any(s["valid"] for s in phase_samples) else "missing valid phase evidence; not accepted"),
        },
    }
    output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(f"运行{'完成，待验收' if not errors else '失败'}：{errors}；报告 {output}", flush=True)
    if report["q98MemoryVerdict"] is not None:
        verdict = report["q98MemoryVerdict"]
        if report["q98MemoryVerdictAuthoritative"]:
            print(f"Q98 内存判定：{'通过' if verdict['passed'] else '不通过'}；{verdict['errors']}", flush=True)
        else:
            print(f"Q98 内存判定（非权威：缺最终前端报告，仅诊断）：{verdict['errors']}", flush=True)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
