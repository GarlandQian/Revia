#!/usr/bin/env python3
"""Read explicit Revia measurement JSON files; print raw metrics, not verdicts."""

import argparse
import hashlib
import json
from pathlib import Path
import sys


MISSING = object()
MISSING_TEXT = "\u7f3a\u5931"
COMBO_FIELDS = (
    "id", "label", "startedAt", "view", "path", "firstScreen", "jump",
    "jumpDom", "jumpDomError", "scroll", "highlight", "visibility", "valid",
)


def lookup(value, path):
    for key in path.split("."):
        if not isinstance(value, dict) or key not in value:
            return MISSING
        value = value[key]
    return value


def reject_non_json_number(value):
    raise ValueError("Non-JSON numeric literal: " + value)


def extract(path):
    result = {"source": str(path.resolve())}
    try:
        raw = path.read_bytes()
        result["sourceBytes"] = len(raw)
        result["sourceSha256"] = hashlib.sha256(raw).hexdigest()
        data = json.loads(raw, parse_constant=reject_non_json_number)
        if not isinstance(data, dict):
            raise ValueError("Expected a top-level JSON object")
        result["metadata"] = {key: value for key, value in data.items() if key != "combos"}
        combos = data.get("combos", [])
        if not isinstance(combos, list):
            raise ValueError("Expected combos to be an array")
        result["measurements"] = []
        for index, combo in enumerate(combos):
            if not isinstance(combo, dict):
                raise ValueError("Expected combos[%d] to be an object" % index)
            result["measurements"].append({
                "comboIndex": index,
                **{key: combo[key] for key in COMBO_FIELDS if key in combo},
            })
    except (OSError, ValueError, UnicodeError) as error:
        result["readError"] = "%s: %s" % (type(error).__name__, error)
    return result


def cell(value):
    if value is MISSING:
        return MISSING_TEXT
    if isinstance(value, str):
        text = value
    else:
        text = json.dumps(value, ensure_ascii=False, separators=(",", ":"))
    return text.replace("|", "\\|").replace("\r", "\\r").replace("\n", "<br>")


def table(title, headers, rows):
    lines = ["### " + title, ""]
    lines.append("| " + " | ".join(headers) + " |")
    lines.append("| " + " | ".join("---" for _ in headers) + " |")
    lines.extend("| " + " | ".join(cell(value) for value in row) + " |" for row in rows)
    if not rows:
        lines.append("| " + " | ".join(["\u65e0\u6837\u672c"] + ["-"] * (len(headers) - 1)) + " |")
    return "\n".join(lines)


def render_tables(report):
    samples = []
    source_rows = []
    visibility_rows = []
    for file_index, item in enumerate(report["files"], 1):
        source_id = "F%d" % file_index
        metadata = item.get("metadata", {})
        source_rows.append([
            source_id, item["source"], lookup(metadata, "ok"), lookup(metadata, "valid"),
            lookup(metadata, "error"), item.get("readError", MISSING),
            len(item.get("measurements", [])), item.get("sourceSha256", MISSING),
        ])
        visibility_rows.append(visibility_row(source_id, "\u6574\u4efd\u8bb0\u5f55", lookup(metadata, "visibility")))
        for combo in item.get("measurements", []):
            sample_id = "%s/C%d" % (source_id, combo["comboIndex"])
            samples.append((sample_id, combo))
            visibility_rows.append(visibility_row(sample_id, "\u5355\u7ec4\u6d4b\u91cf", lookup(combo, "visibility")))

    sections = [
        "# Revia D06 \u539f\u59cb\u6d4b\u91cf\u63d0\u53d6",
        "\u4ec5\u63d0\u53d6\u5df2\u8bb0\u5f55\u7684\u503c\uff0c\u4e0d\u4ea7\u751f\u901a\u8fc7\u6216\u4e0d\u901a\u8fc7\u7ed3\u8bba\u3002\u8017\u65f6\u5355\u4f4d\u4e3a ms\uff0c\u9664\u975e\u5217\u540d\u53e6\u6709\u6807\u6ce8\u3002",
        "\u7f3a\u5931\u5b57\u6bb5\u663e\u793a\u201c\u7f3a\u5931\u201d\uff1bnull\u30010\u3001false\u4fdd\u7559\u539f\u610f\u3002valid\u4e0d\u4ece\u5176\u4ed6\u5b57\u6bb5\u63a8\u5bfc\u3002",
        "focusLosses \u4fdd\u7559\u539f\u503c\uff1b\u5f53\u524d\u63a2\u9488\u662f 0/1 \u5931\u7126\u6807\u5fd7\uff0c\u4e0d\u662f\u7d2f\u8ba1\u5931\u7126\u6b21\u6570\u3002",
        "appear/warm \u662f M1 \u539f\u59cb\u603b\u8ba1\u65f6\uff0c\u4e0d\u7b49\u540c\u4e8e DOM \u5c31\u7eea\u6216\u786c\u4ef6\u4e0a\u5c4f\uff1bcolorRestoreMs \u4e0d\u7b49\u540c\u4e8e colorDomMs\u3002",
        "droppedFrames \u662f\u539f\u6d4b\u91cf\u4e2d\u5927\u4e8e 20 ms \u7684\u5e27\u95f4\u9694\u6570\uff0c\u4e0d\u662f\u63a8\u7b97\u7684\u786c\u4ef6\u4e22\u5e27\u6570\u3002",
        table("\u6765\u6e90\u4e0e\u539f\u59cb\u72b6\u6001", [
            "ID", "\u6587\u4ef6", "ok", "valid", "error", "\u8bfb\u53d6\u9519\u8bef", "\u7ec4\u6570", "SHA-256",
        ], source_rows),
        table("\u9996\u5c4f\u539f\u59cb\u8ba1\u65f6", [
            "ID", "\u6807\u7b7e", "\u5f00\u59cb\u65f6\u95f4", "\u663e\u793a\u884c\u6570", "\u6587\u4ef6\u6570", "fromListChangesMs",
            "fromOpenWorktreeMs", "listChangesMs", "firstRowsMs", "renderMs", "rafPainted", "\u89c6\u53e3\u9ad8\u5ea6", "DPR",
        ], [
            [sid, lookup(c, "label"), lookup(c, "startedAt")] +
            [lookup(c, "firstScreen." + key) for key in (
                "rowCount", "fileCount", "fromListChangesMs", "fromOpenWorktreeMs", "listChangesMs",
                "firstRowsMs", "renderMs", "rafPainted", "viewportHeight", "devicePixelRatio",
            )] for sid, c in samples
        ]),
        table("M1 \u539f\u59cb\u8df3\u8f6c\u8ba1\u65f6", [
            "ID", "fromRow", "toRow", "\u76ee\u6807\u5df2\u7f13\u5b58", "\u76ee\u6807\u5df2\u8986\u76d6", "appearMs", "fetchMs", "withinOneFrame",
            "\u8fd4\u56de\u5df2\u7f13\u5b58", "\u8fd4\u56de\u5df2\u8986\u76d6", "warmAppearMs", "warmWithinOneFrame", "colorRestoreMs",
        ], [
            [sid] + [lookup(c, "jump." + key) for key in (
                "fromRow", "toRow", "targetWasCached", "targetCovered", "appearMs", "fetchMs", "withinOneFrame",
                "warmTargetWasCached", "warmCovered", "warmAppearMs", "warmWithinOneFrame", "colorRestoreMs",
            )] for sid, c in samples
        ]),
    ]

    jump_rows = []
    dom_rows = []
    validity_rows = []
    preparation_rows = []
    for sid, combo in samples:
        preparation_rows.append([sid] + [lookup(combo, key) for key in (
            "jumpDom.sourceReady", "jumpDom.sourcePreparation", "jumpDom.preparationError", "jumpDomError", "valid",
        )])
        for direction, label in (("outward", "\u53bb\u7a0b"), ("return", "\u8fd4\u56de")):
            jump = lookup(combo, "jumpDom." + direction)
            jump_rows.append([sid, label] + [lookup(jump, key) for key in (
                "fromRow", "toRow", "targetWasCached", "missingRowsBefore", "colorsCachedBefore",
                "scrollCallMs", "immediateDomMs", "textDomMs", "textDomSource", "colorDomMs",
                "firstFrame.callbackMs", "firstFrame.checkedMs", "firstFrame.rafTimestampMs",
            )])
            for stage, name in (("immediate", "\u540c\u6b65 DOM"), ("firstFrame.dom", "\u9996 rAF DOM"), ("final", "\u6700\u7ec8 DOM")):
                dom = lookup(jump, stage)
                dom_rows.append([sid, label, name] + [lookup(dom, key) for key in (
                    "matchedRows", "requiredRows", "textMatched", "placeholderRows", "placeholderExposed",
                    "canvasReady", "canvasAligned", "colorsCached", "coloredCells", "expectedColoredCells", "colorsMatched",
                )])
            validity_rows.append([sid, label] + [lookup(jump, key) for key in (
                "valid", "completed", "timedOut", "timeoutMs", "frameReady", "textWithinBudget", "frameBudgetMs", "textBudgetMs",
                "hidden", "hiddenTransitions", "firstFrame.hidden", "placeholderStyle.ready",
            )])

    sections.extend([
        table("DOM \u8df3\u8f6c\u51c6\u5907", [
            "ID", "sourceReady", "sourcePreparation", "preparationError", "jumpDomError", "\u6d4b\u91cf valid",
        ], preparation_rows),
        table("DOM \u8df3\u8f6c\u7f13\u5b58\u4e0e\u8ba1\u65f6", [
            "ID", "\u65b9\u5411", "fromRow", "toRow", "\u76ee\u6807\u5df2\u7f13\u5b58", "\u4e8b\u524d\u7f3a\u884c", "\u4e8b\u524d\u989c\u8272\u7f13\u5b58",
            "scrollCallMs", "immediateDomMs", "textDomMs", "textDomSource", "colorDomMs",
            "\u9996 rAF callbackMs", "\u9996 rAF checkedMs", "\u9996 rAF timestampMs",
        ], jump_rows),
        table("\u540c\u6b65\u3001\u9996 rAF \u4e0e\u6700\u7ec8 DOM \u539f\u59cb\u6838\u5bf9", [
            "ID", "\u65b9\u5411", "\u9636\u6bb5", "\u5339\u914d\u884c", "\u9700\u6c42\u884c", "\u6587\u5b57\u5339\u914d", "\u5360\u4f4d\u884c", "\u5360\u4f4d\u66b4\u9732",
            "\u753b\u5e03\u5c31\u7eea", "\u753b\u5e03\u5bf9\u9f50", "\u989c\u8272\u7f13\u5b58", "\u5df2\u7740\u8272\u5355\u5143", "\u9884\u671f\u7740\u8272\u5355\u5143", "\u989c\u8272\u5339\u914d",
        ], dom_rows),
        table("DOM \u6709\u6548\u6027\u76f8\u5173\u539f\u59cb\u5b57\u6bb5", [
            "ID", "\u65b9\u5411", "valid", "completed", "timedOut", "timeoutMs", "frameReady", "textWithinBudget",
            "frameBudgetMs", "textBudgetMs", "hidden", "hiddenTransitions", "\u9996 rAF hidden", "\u5360\u4f4d\u6837\u5f0f ready",
        ], validity_rows),
        table("\u6eda\u52a8\u5e27\u4e0e\u8986\u76d6", [
            "ID", "mode", "\u8bf7\u6c42\u5c4f\u6570", "\u5b9e\u9645\u5c4f\u6570", "frames", "rAF \u5e27", "\u515c\u5e95\u5e27", "\u672a\u8986\u76d6\u5e27",
            "\u672a\u8986\u76d6 max", "\u672a\u8986\u76d6 p95", "\u5e27\u95f4\u9694 max", "\u5e27\u95f4\u9694 p95", "\u5e27\u95f4\u9694 p50",
            "\u6162\u5e27 >20ms", "\u8ddd\u79bb px", "\u8017\u65f6 s",
        ], [
            [sid] + [lookup(c, "scroll." + key) for key in (
                "mode", "requestedScreens", "screens", "frames", "rafFrames", "fallbackFrames", "uncoveredFrames",
                "uncoveredMax", "uncoveredP95", "frameMaxMs", "frameP95Ms", "frameP50Ms", "droppedFrames", "scrollDistancePx", "seconds",
            )] for sid, c in samples
        ]),
        table("\u53ef\u89c1\u6027\u539f\u59cb\u8bb0\u5f55", [
            "ID", "\u8303\u56f4", "state", "hidden", "focused", "focusLosses", "hiddenTransitions", "maxFrameGapMs", "rafPerSecond",
            "\u89c6\u53e3\u5bbd\u5ea6", "\u89c6\u53e3\u9ad8\u5ea6", "DPR", "valid",
        ], visibility_rows),
    ])
    return "\n\n".join(sections)


def visibility_row(sid, scope, visibility):
    return [sid, scope] + [lookup(visibility, key) for key in (
        "state", "hidden", "focused", "focusLosses", "hiddenTransitions", "maxFrameGapMs", "rafPerSecond",
        "viewportWidth", "viewportHeight", "devicePixelRatio", "valid",
    )]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("files", nargs="+", type=Path, help="Explicit existing JSON paths; never waits or retries")
    parser.add_argument("--format", choices=("json", "table", "both"), default="both")
    args = parser.parse_args()
    report = {
        "schemaVersion": 1,
        "policy": "raw values only; no acceptance verdict; absent fields stay absent; null is not zero",
        "files": [extract(path) for path in args.files],
    }
    if args.format in ("json", "both"):
        print(json.dumps(report, ensure_ascii=False, indent=2, allow_nan=False))
    if args.format == "both":
        print("\n---\n")
    if args.format in ("table", "both"):
        print(render_tables(report))
    return int(any("readError" in item for item in report["files"]))


if __name__ == "__main__":
    sys.exit(main())
