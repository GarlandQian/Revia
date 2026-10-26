import json
import statistics
import sys
from pathlib import Path


def describe(samples, key, time_key, start):
    rows = [s for s in samples if key in s and s[time_key] >= start + 180]
    if len(rows) < 2:
        return {"sampleCount": len(rows)}
    xs = [(s[time_key] - start) / 60 for s in rows]
    ys = [s[key] for s in rows]
    mx, my = statistics.mean(xs), statistics.mean(ys)
    slope = sum((x - mx) * (y - my) for x, y in zip(xs, ys)) / sum((x - mx) ** 2 for x in xs)
    bins = []
    for left in range(0, 35, 5):
        values = [s[key] for s in samples if key in s and left * 60 <= s[time_key] - start < (left + 5) * 60]
        bins.append({"fromMinute": left, "toMinute": left + 5, "sampleCount": len(values),
                     "meanMiB": round(statistics.mean(values), 3) if values else None,
                     "minMiB": min(values) if values else None, "maxMiB": max(values) if values else None})
    return {"sampleCount": len(rows), "firstMiB": ys[0], "lastMiB": ys[-1], "minMiB": min(ys),
            "maxMiB": max(ys), "slopeMiBPerMinute": round(slope, 4), "fiveMinuteWallClockBins": bins}


def analyze(path):
    report = json.loads(path.read_text())
    frontend = report["frontend"]
    start = frontend["setupSeconds"]
    result = {"source": path.name, "runCompleted": report["runCompleted"], "errors": report["errors"],
              "startedAt": report["startedAt"], "endedAt": report["endedAt"],
              "activeSeconds": frontend.get("actualDurationSeconds"),
              "operationWallSeconds": frontend.get("wallDurationSeconds"),
              "checkpointPauseSeconds": frontend.get("checkpointPauseSeconds"),
              "reviaPid": report["reviaPid"], "visibility": frontend["finalVisibility"],
              "phaseSummary": report.get("phaseMemorySummary"), "regular": {}, "phase": {}}
    for key in ["rustRssMb", "rustFootprintMb", "webContentRssMb", "webContentFootprintMb", "networkingRssMb", "gpuRssMb"]:
        result["regular"][key] = describe(report["memoryTimeSeries"], key, "elapsedSec", start)
    valid_phase = [s for s in report.get("phaseMemoryTimeSeries", []) if s["valid"]]
    for key in ["rustFootprintMb", "webContentFootprintMb", "webContentMallocAllocatedMb", "webContentRssMb"]:
        result["phase"][key] = describe(valid_phase, key, "samplingStartedElapsedSec", start)
    return result


if __name__ == "__main__":
    results = {"method": {"unit": "MiB", "warmupSeconds": 180,
                         "timeAxis": "wall seconds since driver elapsedSec minus frontend setupSeconds; not active time",
                         "phasePolicy": "all finalized valid checkpoints; invalid samples remain in source report",
                         "bins": "fixed [left,right) five-minute wall-clock buckets, including warmup and tail"},
               "runs": [analyze(Path(path)) for path in sys.argv[1:]]}
    print(json.dumps(results, indent=2, ensure_ascii=False))
