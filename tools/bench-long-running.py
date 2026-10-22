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
FOCUS_COUNTERS = {"idle": (), "ipc": ("totalIpcRequests",), "open": ("totalFileSwitches",),
                  "scroll": ("totalScrolls",), "jump": ("totalJumps",), "view": ("totalViewSwitches",)}
FULL_LOAD_COUNTERS = ("totalFileSwitches", "totalViewSwitches", "totalScrolls", "totalJumps")


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


def sample_memory(pid):
    domain = subprocess.check_output(["launchctl", "print", f"pid/{pid}"], text=True, timeout=5)
    services = webkit_services(domain)
    pids = [pid] + [child for group in services.values() for child in group]
    output = subprocess.check_output(["ps", "-o", "pid=,rss=", "-p", ",".join(map(str, pids))], text=True, timeout=5)
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


def physical_footprint(pid):
    output = subprocess.check_output(["vmmap", "-summary", str(pid)], text=True, timeout=10)
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


def webcontent_memory(pid):
    output = subprocess.check_output(["vmmap", "-summary", str(pid)], text=True, timeout=10)
    return {"pid": pid, "footprintMb": parse_footprint(output),
            "webkitMallocAllocatedMb": parse_webkit_allocated(output)}


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


def sample_phase_memory(proc, checkpoints, checkpoint, expected_services, started):
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
            subprocess.run(["notifyutil", "-p", "org.WebKit.lowMemory"], check=True, timeout=5)
            posted = time.monotonic()
            sample["lowMemoryPostedElapsedSec"] = round(posted - started, 3)
            time.sleep(max(0.0, posted + PRESSURE_SETTLE_SECONDS - time.monotonic()))
            sample["timestamp"] = datetime.datetime.now().astimezone().isoformat()
            checkpoints.poll()
            errors.extend(checkpoints.invalid_reasons(checkpoint))
            if proc.poll() is not None:
                errors.append("低内存通知后应用已退出")
        if not errors:
            sample.update(sample_memory(proc.pid))
            services = {role: sample[ROLES[role] + "Pids"] for role in ROLES}
            if expected_services is not None and services != expected_services:
                raise RuntimeError("WebKit 进程与常规采样归属不一致")
            sample["rustFootprintMb"] = physical_footprint(proc.pid)
            sample["webContentVmmap"] = [webcontent_memory(pid) for pid in services["WebContent"]]
            sample["webContentFootprintMb"] = sum(item["footprintMb"] for item in sample["webContentVmmap"])
            sample["webContentMallocAllocatedMb"] = sum(item["webkitMallocAllocatedMb"] for item in sample["webContentVmmap"])
            after = sample_memory(proc.pid)
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


def finalize_phase_samples(samples, checkpoints, frontend):
    def instant(value):
        result = datetime.datetime.fromisoformat(value.replace("Z", "+00:00"))
        if result.tzinfo is None:
            raise ValueError("阶段时间缺少时区")
        return result

    for sample in samples:
        errors = sample["errors"]
        sample["checkpointEndConfirmed"] = checkpoints.has_ended(sample["checkpointId"])
        if not sample["checkpointEndConfirmed"]:
            errors.append("缺少匹配的 checkpoint END，不能确认静置窗口闭合")
        reported = frontend.get("phaseCheckpoints")
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


def record_phase_samples(checkpoints, proc, services, started, samples, journal):
    while checkpoints.pending:
        sample = sample_phase_memory(proc, checkpoints, checkpoints.pending.popleft(), services, started)
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
    pressure = [s for s in phase_samples if s.get("pressure") and s["valid"] and s["samplingStartedElapsedSec"] >= cutoff]
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
    initial_services = None
    with log_path.open("x") as log, samples_path.open("x") as journal, phase_samples_path.open("x") as phase_journal:
        proc = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=subprocess.STDOUT)
        load = f"固定 {args.diagnostic_cycles} 循环（{seconds}s 安全上限）" if args.diagnostic_cycles else f"连续操作 {seconds} 秒"
        print(f"Revia PID={proc.pid}，等待可见窗口后{load}；诊断={diagnostic}；报告 {output}", flush=True)
        next_sample = started + 2
        try:
            while proc.poll() is None:
                checkpoints.poll()
                record_phase_samples(checkpoints, proc, initial_services, started, phase_samples, phase_journal)
                now = time.monotonic()
                # Allow 120s visibility setup, about 200s phase/pressure holds and an exit margin;
                # single-operation diagnostics may also pause up to 10 minutes per hidden-window interruption.
                if now - started > seconds + 480 + (args.idle_seconds if diagnostic else 0) + (1800 if args.focus else 0):
                    raise TimeoutError("等待可见窗口或操作硬超时")
                if now >= next_sample:
                    sampling_started_at = datetime.datetime.now().astimezone().isoformat()
                    try:
                        sample = sample_memory(proc.pid)
                    except (subprocess.SubprocessError, RuntimeError) as error:
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
                            sample["rustFootprintMb"] = physical_footprint(proc.pid)
                            if diagnostic:
                                sample["webContentVmmap"] = [webcontent_memory(child) for child in services["WebContent"]]
                                sample["webContentFootprintMb"] = sum(item["footprintMb"] for item in sample["webContentVmmap"])
                                sample["webContentMallocAllocatedMb"] = sum(item["webkitMallocAllocatedMb"] for item in sample["webContentVmmap"])
                                sample["networkingFootprintMb"] = sum(physical_footprint(child) for child in services["Networking"])
                            else:
                                sample["webContentFootprintMb"] = sum(physical_footprint(child) for child in services["WebContent"])
                        except subprocess.SubprocessError:
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
            if proc.poll() is None:
                proc.terminate()
                try:
                    proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
            try:
                checkpoints.poll()
                record_phase_samples(checkpoints, proc, initial_services, started, phase_samples, phase_journal)
            except (OSError, RuntimeError) as error:
                errors.append(f"最终阶段日志不可核验：{error}")
    if frontend_path.exists():
        try:
            frontend = json.loads(frontend_path.read_text())
        except (OSError, ValueError) as error:
            errors.append(f"前端报告无效：{error}")
    finalize_phase_samples(phase_samples, checkpoints, frontend)
    # The live journal is explicitly provisional; publish the same final validity as the report.
    phase_samples_path.write_text("".join(json.dumps(sample) + "\n" for sample in phase_samples))
    errors.extend(completion_errors(frontend, samples, seconds, proc.returncode, diagnostic, minimal_logging))
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
        "memoryAnalysis": (None if diagnostic else analyze_memory(samples, frontend.get("setupSeconds", 0))),
        "phaseMemoryTimeSeries": phase_samples,
        # Diagnostic runs report the same Q98 numbers for comparison; only baselines count for acceptance.
        "q98MemoryVerdict": analyze_q98(samples, phase_samples, frontend.get("setupSeconds", 0)),
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
        print(f"Q98 内存判定：{'通过' if verdict['passed'] else '不通过'}；{verdict['errors']}", flush=True)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
