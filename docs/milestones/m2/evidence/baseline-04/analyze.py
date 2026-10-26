#!/usr/bin/env python3
"""Read one finalized soak report; never sample, launch, or assign acceptance."""

import argparse
from collections import Counter, defaultdict
import datetime
import hashlib
import json
import math
from pathlib import Path
import statistics
import sys


WARMUP_SECONDS = 180
METRICS = (
    "rustRssMb", "rustFootprintMb", "webContentRssMb", "webContentFootprintMb",
    "webContentMallocAllocatedMb", "networkingRssMb", "networkingFootprintMb",
    "gpuRssMb", "totalRssMb",
)
ROLES = {"WebContent": "webContentPids", "Networking": "networkingPids", "GPU": "gpuPids"}
COUNTERS = ("started", "headersSettled", "failed", "synchronousThrows", "active", "maxActive")


def numeric(value):
    return isinstance(value, (int, float)) and not isinstance(value, bool) and math.isfinite(value)


def instant(value):
    parsed = datetime.datetime.fromisoformat(value.replace("Z", "+00:00"))
    if parsed.tzinfo is None:
        raise ValueError("timestamp lacks timezone")
    return parsed.timestamp()


def rounded(value):
    return round(value, 6) if numeric(value) else None


def numeric_range(values):
    selected = [value for value in values if numeric(value)]
    return {"count": len(selected), "first": selected[0] if selected else None,
            "last": selected[-1] if selected else None,
            "min": min(selected) if selected else None, "max": max(selected) if selected else None}


def scalar_stats(rows, key):
    selected = [row for row in rows if numeric(row.get(key)) and numeric(row.get("startWallSec"))]
    selected.sort(key=lambda row: (row["startWallSec"], row["sourceIndex"]))
    missing = len(rows) - len(selected)
    if not selected:
        return {"sampleCount": 0, "missingMetricCount": missing}
    xs = [row["startWallSec"] / 60 for row in selected]
    ys = [row[key] for row in selected]
    mx, my = statistics.mean(xs), statistics.mean(ys)
    denominator = sum((x - mx) ** 2 for x in xs)
    changes = [right - left for left, right in zip(ys, ys[1:])]
    return {
        "sampleCount": len(selected), "missingMetricCount": missing,
        "firstSourceIndex": selected[0]["sourceIndex"], "lastSourceIndex": selected[-1]["sourceIndex"],
        "firstWallSec": rounded(selected[0]["startWallSec"]),
        "lastWallSec": rounded(selected[-1]["startWallSec"]),
        "firstMiB": ys[0], "lastMiB": ys[-1], "minMiB": min(ys), "maxMiB": max(ys),
        "meanMiB": rounded(my), "medianMiB": rounded(statistics.median(ys)),
        "netChangeMiB": rounded(ys[-1] - ys[0]),
        "slopeMiBPerMinute": rounded(sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / denominator)
        if denominator else None,
        "maximumAdjacentDeltaMiB": rounded(max(changes)) if changes else None,
        "minimumAdjacentDeltaMiB": rounded(min(changes)) if changes else None,
    }


def bins(rows, key, operation_seconds, post_warmup=False):
    result = []
    for left in range(0, max(1, math.ceil(operation_seconds / 300)) * 300, 300):
        right = left + 300
        effective_left = max(left, WARMUP_SECONDS) if post_warmup else left
        if effective_left >= right or effective_left >= operation_seconds:
            continue
        group = [row for row in rows if effective_left <= row["startWallSec"] < right]
        result.append({"fromWallMinute": effective_left / 60, "toWallMinute": right / 60,
                       "operationEndWallMinute": rounded(min(right, operation_seconds) / 60),
                       **scalar_stats(group, key)})
    return result


def classify(start, end, operation_seconds):
    if start < 0:
        return "before_operations" if end < 0 else "straddles_operation_start"
    if end > operation_seconds:
        return "after_operations" if start > operation_seconds else "straddles_operation_end"
    if start < WARMUP_SECONDS:
        return "warmup" if end < WARMUP_SECONDS else "straddles_180_seconds"
    return "post_180_seconds"


def normalize_memory(samples, origin, operation_seconds, phase=False):
    rows = []
    for index, source in enumerate(samples):
        row = {"sourceIndex": index, **{key: source[key] for key in METRICS if key in source}}
        for key in (*ROLES.values(), "serviceDomain", "checkpointId", "valid", "validation", "errors"):
            if key in source:
                row[key] = source[key]
        try:
            start = instant(source["timestamp"] if phase else source["samplingStartedAt"]) - origin
            end = instant(source["samplingEndedAt"] if phase else source["timestamp"]) - origin
            if end < start:
                raise ValueError("sample ends before it starts")
            row.update(startWallSec=start, endWallSec=end, samplingSeconds=rounded(end - start),
                       intervalClass=classify(start, end, operation_seconds))
        except (KeyError, TypeError, ValueError, AttributeError) as error:
            row.update(startWallSec=None, endWallSec=None, intervalClass="invalid_time", timeError=str(error))
        rows.append(row)
    return rows


def summarize_memory(rows, operation_seconds):
    operation = [row for row in rows if numeric(row["startWallSec"]) and row["startWallSec"] >= 0
                 and row["endWallSec"] <= operation_seconds]
    steady = [row for row in operation if row["startWallSec"] >= WARMUP_SECONDS]
    starts = [row["startWallSec"] for row in operation]
    gaps = [right - left for left, right in zip(starts, starts[1:])]
    return {
        "sampleCount": len(rows), "operationIntervalCount": len(operation), "post180Count": len(steady),
        "intervalClasses": dict(Counter(row["intervalClass"] for row in rows)),
        "excludedFromPost180": [{"sourceIndex": row["sourceIndex"], "reason": row["intervalClass"]}
                                for row in rows if row not in steady],
        "sampleStartWallSec": numeric_range(starts), "adjacentSampleStartGapSeconds": numeric_range(gaps),
        "outOfOrderSampleIndices": [operation[index + 1]["sourceIndex"] for index, gap in enumerate(gaps) if gap <= 0],
        "metrics": {key: {"wholeOperation": scalar_stats(operation, key), "post180": scalar_stats(steady, key),
                          "fiveMinuteWallClockBinsIncludingWarmup": bins(operation, key, operation_seconds),
                          "post180FixedWallClockBins": bins(steady, key, operation_seconds, True)} for key in METRICS},
    }


def pid_signature(sample):
    return {role: sorted(sample.get(key, [])) for role, key in ROLES.items()}


def pid_audit(report, regular, phase):
    expected_domain = f"pid/{report.get('reviaPid')}"
    signatures = Counter()
    issues = []
    for kind, samples in (("regular", regular), ("phase", phase)):
        for index, sample in enumerate(samples):
            signature = pid_signature(sample)
            signatures[json.dumps(signature, sort_keys=True)] += 1
            for role, pids in signature.items():
                if not pids or any(not isinstance(pid, int) or isinstance(pid, bool) or pid <= 0 for pid in pids):
                    issues.append({"series": kind, "index": index, "issue": f"missing/invalid {role} PIDs"})
            if sample.get("serviceDomain") != expected_domain:
                issues.append({"series": kind, "index": index, "issue": "serviceDomain differs from exact Revia PID"})
            after = sample.get("servicesAfterSampling")
            if after is not None and {role: sorted(after.get(role, [])) for role in ROLES} != signature:
                issues.append({"series": kind, "index": index, "issue": "PID membership changed within sample"})
    return {"expectedServiceDomain": expected_domain, "attribution": report.get("webkitAttribution"),
            "observedServiceSets": [{"pids": json.loads(key), "sampleCount": count} for key, count in signatures.items()],
            "sameNonemptyServiceSetInAllRecordedSamples": len(signatures) == 1 and not issues,
            "issues": issues,
            "limitation": "Uses archived per-sample services evidence; does not independently observe process lifetime."}


def phase_audit(report, origin, operation_seconds):
    frontend = report["frontend"]
    checkpoints = frontend.get("phaseCheckpoints", [])
    samples = report.get("phaseMemoryTimeSeries", [])
    rows = normalize_memory(samples, origin, operation_seconds, True)
    counts = Counter(sample.get("checkpointId") for sample in samples)
    audit = []
    usable = []
    for index, (sample, row) in enumerate(zip(samples, rows)):
        reasons = []
        notes = []
        checkpoint_id = sample.get("checkpointId")
        matches = [entry for entry in checkpoints if entry.get("cycle") == checkpoint_id]
        if sample.get("valid") is not True or sample.get("errors"):
            reasons.append("driver marked sample invalid or reported errors")
        if sample.get("validation") != "final" or sample.get("checkpointEndConfirmed") is not True:
            reasons.append("checkpoint closure is not finalized")
        if sample.get("samplingChecksPassed") is not True:
            reasons.append("live sampling checks did not pass")
        if counts[checkpoint_id] != 1:
            reasons.append("duplicate checkpoint sample ID")
        if len(matches) != 1 or matches[0].get("file") != "src/staged.ts" or matches[0].get("view") != "split":
            reasons.append("missing unique frontend staged/split interval")
        else:
            try:
                if not (instant(matches[0]["startedAt"]) <= instant(sample["timestamp"])
                        <= instant(sample["samplingEndedAt"]) <= instant(matches[0]["endedAt"])):
                    reasons.append("sampling interval is outside frontend checkpoint interval")
            except (KeyError, TypeError, ValueError, AttributeError):
                reasons.append("checkpoint timestamps cannot be compared")
        if row["intervalClass"] in ("invalid_time", "before_operations", "straddles_operation_start",
                                    "after_operations", "straddles_operation_end"):
            reasons.append("phase interval is not wholly within operations")
        if numeric(sample.get("samplingEndedElapsedSec")) and numeric(sample.get("checkpointObservedElapsedSec")):
            rounded_hold = sample["samplingEndedElapsedSec"] - sample["checkpointObservedElapsedSec"]
            # Both archived fields are rounded to 1ms; do not reverse the driver's precise boundary check.
            if rounded_hold > 3.501:
                reasons.append("sampling extended beyond driver 3500ms hold window")
            elif rounded_hold >= 3.499:
                notes.append("rounded hold duration is within 1ms of the 3500ms boundary; retain driver validity")
        else:
            reasons.append("missing phase monotonic interval")
        if sample.get("servicesAfterSampling") != pid_signature(sample):
            reasons.append("missing/mismatched after-sampling PID membership")
        row["independentlyUsable"] = not reasons
        audit.append({"sourceIndex": index, "checkpointId": checkpoint_id, "reportedValid": sample.get("valid"),
                      "independentlyUsable": not reasons, "driverErrors": sample.get("errors", []),
                      "auditIssues": reasons, "notes": notes})
        if not reasons:
            usable.append(row)
    return {"reportedSummary": report.get("phaseMemorySummary"), "frontendCheckpointCount": len(checkpoints),
            "reportedSampleCount": len(samples), "reportedValidCount": sum(sample.get("valid") is True for sample in samples),
            "independentlyUsableCount": len(usable),
            "frontendCheckpointIdsWithoutSample": [entry.get("cycle") for entry in checkpoints
                                                   if entry.get("cycle") not in counts],
            "audit": audit, "allSamples": rows, "usableSummary": summarize_memory(usable, operation_seconds)}


def visibility_audit(frontend):
    entries = [(f"heartbeat[{index}]", sample.get("visibility")) for index, sample in enumerate(frontend.get("samples", []))]
    no_heartbeats = not entries
    entries.append(("final", frontend.get("finalVisibility")))
    issues = []
    if no_heartbeats:
        issues.append({"at": "heartbeats", "issue": "no periodic visibility evidence retained"})
    for label, value in entries:
        if not isinstance(value, dict):
            issues.append({"at": label, "issue": "missing visibility observation"})
            continue
        if (value.get("hidden") is not False or not numeric(value.get("rafPerSecond"))
                or value["rafPerSecond"] <= 5 or value.get("hiddenTransitions") != 0
                or not numeric(value.get("maxFrameGapMs")) or value["maxFrameGapMs"] > 2000):
            issues.append({"at": label, "issue": "existing visibility/compositor guard not satisfied", "value": value})
    observed = [value for _, value in entries if isinstance(value, dict)]
    return {"observations": len(entries), "readinessCountBeforeMonitorReset": len(frontend.get("readiness", [])),
            "rafPerSecond": numeric_range([value.get("rafPerSecond") for value in observed]),
            "maxFrameGapMs": numeric_range([value.get("maxFrameGapMs") for value in observed]),
            "hiddenTransitions": numeric_range([value.get("hiddenTransitions") for value in observed]),
            "unfocusedObservations": sum(value.get("focused") is False for value in observed),
            "final": frontend.get("finalVisibility"), "issues": issues,
            "limitation": "Uses existing >5 rAF/s and <=2000ms gap guards; unfocused is not hidden; not a 60fps claim."}


def ipc_audit(frontend):
    snapshots = []
    required_observation_count = 0
    missing_required = []

    def add(label, value, required=False, **context):
        nonlocal required_observation_count
        if required:
            required_observation_count += 1
        if isinstance(value, dict):
            snapshots.append({"source": label, "context": context, "snapshot": value})
        elif required:
            missing_required.append(label)

    add("initial", frontend.get("ipcInitial"), required=True)
    for index, sample in enumerate(frontend.get("samples", [])):
        add(f"heartbeat[{index}]", sample.get("ipc"), required=True, **{key: sample.get(key) for key in
            ("elapsedSec", "cycle", "file", "view", "loadedChunks", "highlightLines")})
    for index, checkpoint in enumerate(frontend.get("phaseCheckpoints", [])):
        for boundary in ("ipcBefore", "ipcAfter"):
            add(f"checkpoint[{index}].{boundary}", checkpoint.get(boundary), required=True, **{key: checkpoint.get(key) for key in
                ("cycle", "file", "view", "startedAt", "endedAt")})
    add("afterOperations", frontend.get("ipcAfterOperations"))
    idle = frontend.get("idle", {})
    if isinstance(idle, dict):
        add("idle.initial", idle.get("initial"))
        for index, sample in enumerate(idle.get("samples", [])):
            add(f"idle[{index}]", sample.get("ipc"), elapsedSec=sample.get("elapsedSec"))
    add("final", frontend.get("ipcFinal"), required=True)
    snapshots.sort(key=lambda item: item["snapshot"].get("observedAtMs", math.inf)
                   if numeric(item["snapshot"].get("observedAtMs")) else math.inf)
    initial_at = frontend.get("ipcInitial", {}).get("observedAtMs")
    issues = [{"at": label, "issue": "required IPC observation missing or not an object"} for label in missing_required]
    previous = {}
    grouped_dom = defaultdict(list)
    observations = []
    for item in snapshots:
        label, snapshot, context = item["source"], item["snapshot"], item["context"]
        fetch = snapshot.get("fetch", {})
        total = fetch.get("total", {})
        callback = snapshot.get("callbacks", {})
        if snapshot.get("fetchHookStillInstalled") is not True or snapshot.get("stopped") is not False:
            issues.append({"at": label, "issue": "fetch hook absent or stopped during observation"})
        if callback.get("available") is not True:
            issues.append({"at": label, "issue": "callback registry unavailable"})
        counters = {"total": total, **{f"command:{key}": value for key, value in fetch.get("commands", {}).items()}}
        for name, counter in counters.items():
            if any(not numeric(counter.get(key)) or counter[key] < 0 for key in COUNTERS):
                issues.append({"at": label, "counter": name, "issue": "missing/invalid fetch counter"})
                continue
            if counter["started"] != counter["headersSettled"] + counter["failed"] + counter["active"]:
                issues.append({"at": label, "counter": name, "issue": "fetch counter identity does not close"})
            if counter["synchronousThrows"] > counter["failed"] or counter["active"] > counter["maxActive"]:
                issues.append({"at": label, "counter": name, "issue": "inconsistent fetch maximum/failure counts"})
            before = previous.get(name)
            if before and any(counter[key] < before[key] for key in COUNTERS if key != "active"):
                issues.append({"at": label, "counter": name, "issue": "cumulative counter decreased"})
            previous[name] = counter
        observed_at = snapshot.get("observedAtMs")
        row = {"source": label, "context": context,
               "secondsSinceInitialIpcSnapshot": rounded((observed_at - initial_at) / 1000)
               if numeric(observed_at) and numeric(initial_at) else None,
               "callbacks": callback, "fetchTotal": total, "dom": snapshot.get("dom"),
               "resourceTimingEntries": snapshot.get("resourceTimingEntries"),
               "observationErrors": snapshot.get("observationErrors"),
               "commandOverflowRequests": fetch.get("commandOverflowRequests")}
        observations.append(row)
        if context.get("file") is not None and context.get("view") is not None:
            group = ("checkpoint" if label.startswith("checkpoint") else "heartbeat", context["file"], context["view"])
            grouped_dom[group].append(row)
    callbacks = [row["callbacks"] for row in observations]
    initial, final = frontend.get("ipcInitial"), frontend.get("ipcFinal")
    return {
        "present": bool(snapshots), "observationsCount": len(observations), "initial": initial, "final": final,
        "expectedRequiredObservationCount": required_observation_count, "missingObservationLabels": missing_required,
        "counterScope": "diagnostic hook installation through final snapshot before stop/writeMeasurement; formal baseline has no ipcAfterOperations and final counters include completion logging",
        "callbackSummary": {key: numeric_range([value.get(key) for value in callbacks]) for key in
            ("registrySize", "baselineStillRegistered", "newIdCount", "nonBaselineRegistryCount",
             "oldestObservedAgeMs", "baselineOverflow", "observationOverflow")},
        "unavailableCallbackObservations": sum(value.get("available") is not True for value in callbacks),
        "fetchActiveAtObservation": numeric_range([row["fetchTotal"].get("active") for row in observations]),
        "fetchMaxActiveReported": numeric_range([row["fetchTotal"].get("maxActive") for row in observations]),
        "observationErrors": numeric_range([row["observationErrors"] for row in observations]),
        "resourceTimingEntries": numeric_range([row["resourceTimingEntries"] for row in observations]),
        "sameContextDom": [{"kind": group[0], "file": group[1], "view": group[2], "observations": len(rows),
                            "elements": numeric_range([(row["dom"] or {}).get("elements") for row in rows]),
                            "diffRows": numeric_range([(row["dom"] or {}).get("diffRows") for row in rows])}
                           for group, rows in grouped_dom.items()],
        "issues": issues, "allObservations": observations,
        "limitations": ["fetch settlement is response headers, not body/callback completion; fallback is not counted",
                        "callback observations are snapshots, not continuous pending-request traces; counts include listeners",
                        "ID age is an observed lower bound; ID cap/overflow must be considered",
                        "ResourceTiming=0 is not proof that WebKit retains no resources; DOM counts are context-dependent"],
    }


def legacy_summary(samples, setup_seconds, time_key):
    rows = []
    for index, sample in enumerate(samples):
        if numeric(sample.get(time_key)):
            rows.append({"sourceIndex": index, "startWallSec": sample[time_key] - setup_seconds,
                         **{key: sample[key] for key in METRICS if key in sample}})
    rows = [row for row in rows if row["startWallSec"] >= WARMUP_SECONDS]
    return {key: scalar_stats(rows, key) for key in METRICS}


def analyze(path, expected_sha=None, expected_pid=None):
    content = path.read_bytes()
    report = json.loads(content)
    frontend = report["frontend"]
    origin, end = instant(frontend["operationsStartedAt"]), instant(frontend["operationsEndedAt"])
    duration = end - origin
    if duration <= 0:
        raise ValueError("operation interval is empty or reversed; finalized report required")
    regular, phase = report.get("memoryTimeSeries", []), report.get("phaseMemoryTimeSeries", [])
    rows = normalize_memory(regular, origin, duration)
    active, pause, wall = (frontend.get(key) for key in
                           ("actualDurationSeconds", "checkpointPauseSeconds", "wallDurationSeconds"))
    target = report.get("targetDurationSeconds")
    setup = frontend.get("setupSeconds")
    driver_origin = instant(report["startedAt"])
    sha = report.get("binarySha256")
    issues = []
    if report.get("runCompleted") is not True or frontend.get("ok") is not True or report.get("errors"):
        issues.append("driver/frontend did not report clean completion")
    if report.get("test") != "long_running_baseline" or report.get("diagnostic") is not None or frontend.get("diagnostic") is not None:
        issues.append("report is not a non-diagnostic baseline")
    if report.get("minimalSoakLogging") is not True or frontend.get("minimalSoakLogging") is not True:
        issues.append("minimal baseline logging mode not confirmed")
    if "idle" in frontend:
        issues.append("unexpected terminal diagnostic idle phase")
    if not numeric(active) or not numeric(target) or active < target:
        issues.append("active operation duration does not meet reported target")
    if frontend.get("targetDurationSeconds") != target:
        issues.append("frontend and driver target durations differ")
    if frontend.get("samplesCount") != len(frontend.get("samples", [])):
        issues.append("frontend reported heartbeat count does not match retained samples")
    reported_warmup = (report.get("memoryAnalysis") or {}).get("warmupSeconds")
    if reported_warmup != WARMUP_SECONDS:
        issues.append("driver memory-analysis warmup is missing or differs from 180 seconds")
    if expected_sha is not None and sha != expected_sha:
        issues.append("binary hash does not match supplied expected hash")
    if expected_pid is not None and report.get("reviaPid") != expected_pid:
        issues.append("Revia PID does not match supplied expected PID")
    counts = {key: frontend.get(key) for key in
              ("totalCycles", "totalFileSwitches", "totalViewSwitches", "totalScrolls", "totalJumps")}
    if any(not numeric(value) or value <= 0 for value in counts.values()):
        issues.append("required operation counts missing or zero")
    if not regular:
        issues.append("no regular memory samples")
    pid_result, phase_result = pid_audit(report, regular, phase), phase_audit(report, origin, duration)
    visibility_result, ipc_result = visibility_audit(frontend), ipc_audit(frontend)
    if not isinstance(frontend.get("ipcFinal"), dict):
        issues.append("missing final IPC snapshot")
    return {
        "method": {"unit": "MiB", "warmupSeconds": WARMUP_SECONDS,
                   "timeAxis": "wall seconds since frontend operationsStartedAt, using timezone-bearing ISO timestamps",
                   "metricTiming": "regression/bins use sampling start; metrics are acquired within the retained start/end bracket, not simultaneously",
                   "post180Selection": "all complete sampling intervals within operations with sampling start >=180; boundary exclusions are explicit",
                   "phasePolicy": "all finalized valid checkpoints with independently checked closure; all invalid samples remain in output",
                   "activeTime": "frontend active duration excludes all checkpoint preparation/hold pauses; per-sample active time cannot be reconstructed exactly",
                   "legacyComparison": "separate old driver elapsedSec minus frontend setupSeconds summary, not silently substituted for timestamp alignment"},
        "source": {"path": str(path.resolve()), "reportSha256": hashlib.sha256(content).hexdigest(),
                   "binarySha256": sha, "expectedBinarySha256": expected_sha, "expectedReviaPid": expected_pid,
                   "reviaPid": report.get("reviaPid"), "platform": report.get("platform"), "repo": report.get("repo"),
                   "startedAt": report.get("startedAt"), "endedAt": report.get("endedAt")},
        "execution": {"runCompleted": report.get("runCompleted"), "frontendOk": frontend.get("ok"),
                      "driverErrors": report.get("errors"), "frontendError": frontend.get("error"),
                      "driverAcceptance": report.get("acceptance"), "targetActiveSeconds": target,
                      "activeSeconds": active, "operationWallSeconds": wall, "checkpointPauseSeconds": pause,
                      "driverLifetimeSeconds": report.get("actualDurationSeconds"),
                      "driverReportedWarmupSeconds": reported_warmup,
                      "operationTimestampSeconds": rounded(duration),
                      "wallMinusActiveMinusPauseSeconds": rounded(wall - active - pause)
                      if all(numeric(value) for value in (wall, active, pause)) else None,
                      "isoDurationMinusReportedWallSeconds": rounded(duration - wall) if numeric(wall) else None,
                      "thirtyActiveMinutesRecorded": numeric(active) and active >= 1800,
                      "setupSeconds": setup, "counts": counts, "frontendFiles": frontend.get("files"),
                      "frontendReportedSamplesCount": frontend.get("samplesCount"),
                      "actualFrontendSamplesCount": len(frontend.get("samples", [])), "issues": issues},
        "pids": pid_result, "visibility": visibility_result,
        "regularMemory": {**summarize_memory(rows, duration), "allSamples": rows},
        "phaseMemory": phase_result, "ipc": ipc_result,
        "legacyTimeAxis": {"newOriginMinusOldOriginSeconds": rounded(origin - driver_origin - setup) if numeric(setup) else None,
                           "regularPost180": legacy_summary(regular, setup, "elapsedSec") if numeric(setup) else None,
                           "reportedValidPhasePost180": legacy_summary([row for row in phase if row.get("valid") is True],
                                                                        setup, "samplingStartedElapsedSec") if numeric(setup) else None},
        "acceptance": {"status": "not_automatically_decided",
                       "reason": "Completion, integrity, visibility, full memory trajectories, same-phase evidence and presentation checks require joint review; no slope or endpoint alone assigns pass/fail.",
                       "scope": "No claim about a leak/GC/allocator root cause; regular allocated/networking footprint may be absent by design and are never replaced with zero."},
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", type=Path, help="final completed JSON report, not live JSONL")
    parser.add_argument("--expected-sha256")
    parser.add_argument("--expected-pid", type=int)
    args = parser.parse_args()
    try:
        result = analyze(args.report, args.expected_sha256, args.expected_pid)
    except (OSError, ValueError, KeyError, TypeError, AttributeError) as error:
        print(json.dumps({"source": str(args.report), "analysisError": f"{type(error).__name__}: {error}",
                          "acceptance": {"status": "not_automatically_decided"}}, ensure_ascii=False, indent=2))
        return 2
    print(json.dumps(result, ensure_ascii=False, indent=2, allow_nan=False))
    return 0


if __name__ == "__main__":
    sys.exit(main())
