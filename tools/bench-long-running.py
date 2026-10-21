#!/usr/bin/env python3
"""macOS release soak driver. A completed run still requires desktop/trend review."""

import argparse
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


def analyze_memory(samples, start, warmup=180):
    steady = [s for s in samples if s["elapsedSec"] >= start + warmup]
    result = {"warmupSeconds": warmup, "steadySampleCount": len(steady), "series": {}}
    for key in ("rustRssMb", "webContentRssMb", "networkingRssMb", "gpuRssMb", "totalRssMb"):
        if len(steady) < 2:
            continue
        xs, ys = [s["elapsedSec"] / 60 for s in steady], [s[key] for s in steady]
        mx, my = sum(xs) / len(xs), sum(ys) / len(ys)
        denominator = sum((x - mx) ** 2 for x in xs)
        bins = []
        for begin in range(0, len(steady), max(1, len(steady) // 6)):
            group = steady[begin:begin + max(1, len(steady) // 6)]
            bins.append({"fromSec": group[0]["elapsedSec"], "toSec": group[-1]["elapsedSec"],
                         "meanMb": round(sum(s[key] for s in group) / len(group), 2)})
        result["series"][key] = {"firstMb": ys[0], "lastMb": ys[-1], "minMb": min(ys), "maxMb": max(ys),
            "regressionMbPerMinute": round(sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / denominator, 4) if denominator else None,
            "windows": bins}
    return result


def completion_errors(frontend, samples, seconds, returncode):
    errors = []
    if returncode != 0 or frontend.get("ok") is not True:
        errors.append("应用未成功完成：" + str(frontend.get("error", returncode)))
    if frontend.get("actualDurationSeconds", 0) < seconds:
        errors.append("前端连续操作时长不足")
    if not samples or any(not s.get("webContentPids") for s in samples):
        errors.append("缺少已归属的 WebContent 内存数据")
    if any(frontend.get(key, 0) <= 0 for key in ("totalFileSwitches", "totalViewSwitches", "totalScrolls", "totalJumps")):
        errors.append("缺少必需的操作负载")
    return errors


def main():
    parser = argparse.ArgumentParser(description="Revia macOS 长期运行基线；请保持受测窗口可见")
    parser.add_argument("--seconds", type=int)
    parser.add_argument("--minutes", type=int, default=30)
    parser.add_argument("--app", default="src-tauri/target/release/bundle/macos/Revia.app")
    parser.add_argument("--repo", default="/tmp/revia-m1-fixtures/diffs")
    parser.add_argument("--output", default=f"/tmp/revia-soak-{datetime.datetime.now():%Y%m%d-%H%M%S}.json")
    parser.add_argument("--interval", type=int, default=10)
    args = parser.parse_args()
    seconds = args.seconds if args.seconds is not None else args.minutes * 60
    app, repo, output = Path(args.app).resolve(), Path(args.repo).resolve(), Path(args.output).resolve()
    binary = app / "Contents/MacOS/revia"
    frontend_path, log_path, samples_path = (Path(str(output) + suffix) for suffix in (".frontend.json", ".log", ".samples.jsonl"))
    if platform.system() != "Darwin" or seconds <= 0 or args.interval <= 0:
        parser.error("此驱动仅适用于 macOS；时长与采样间隔必须为正")
    if not binary.is_file() or not repo.is_dir():
        parser.error("应用或 fixture 不存在，请先构建当前源码并准备 fixture")
    if any(p.exists() for p in (output, frontend_path, frontend_path.with_suffix(".log"), log_path, samples_path)):
        parser.error("输出已存在，请指定新的 --output，保留旧证据")
    output.parent.mkdir(parents=True, exist_ok=True)
    env = {key: value for key, value in os.environ.items() if not key.startswith("REVIA_")}
    data_dir = tempfile.mkdtemp(prefix="revia-soak-data-")
    env.update(REVIA_SOAK_SECONDS=str(seconds), REVIA_SOAK_DATA_DIR=data_dir,
               REVIA_MEASURE_REPO=str(repo), REVIA_MEASURE_LOG=str(frontend_path))
    started = time.monotonic()
    started_at = datetime.datetime.now().astimezone().isoformat()
    samples, errors, frontend = [], [], {}
    initial_services = None
    with log_path.open("x") as log, samples_path.open("x") as journal:
        proc = subprocess.Popen([str(binary)], env=env, stdout=log, stderr=subprocess.STDOUT)
        print(f"Revia PID={proc.pid}，等待可见窗口后连续操作 {seconds} 秒；报告 {output}", flush=True)
        next_sample = started + 2
        try:
            while proc.poll() is None:
                now = time.monotonic()
                if now - started > seconds + 240:
                    raise TimeoutError("等待可见窗口或操作硬超时")
                if now >= next_sample:
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
                    sample.update(elapsedSec=round(now - started, 2), timestamp=datetime.datetime.now().astimezone().isoformat())
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
    if frontend_path.exists():
        try:
            frontend = json.loads(frontend_path.read_text())
        except (OSError, ValueError) as error:
            errors.append(f"前端报告无效：{error}")
    errors.extend(completion_errors(frontend, samples, seconds, proc.returncode))
    report = {
        "test": "long_running_baseline", "runCompleted": not errors,
        "acceptance": "pending desktop evidence, performance regression and memory trend review",
        "startedAt": started_at, "endedAt": datetime.datetime.now().astimezone().isoformat(),
        "targetDurationSeconds": seconds, "actualDurationSeconds": round(time.monotonic() - started, 2),
        "reviaPid": proc.pid, "webkitAttribution": "launchctl print pid/<exact child PID>: services table",
        "binarySha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
        "platform": platform.platform(), "isolatedDataDir": data_dir, "repo": str(repo),
        "errors": errors, "frontend": frontend, "memoryTimeSeries": samples,
        "memoryAnalysis": analyze_memory(samples, frontend.get("setupSeconds", 0)),
    }
    output.write_text(json.dumps(report, indent=2, ensure_ascii=False) + "\n")
    print(f"运行{'完成，待验收' if not errors else '失败'}：{errors}；报告 {output}", flush=True)
    return 1 if errors else 0


if __name__ == "__main__":
    sys.exit(main())
