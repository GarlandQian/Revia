/**
 * M0 原型界面：左侧改动文件列表 + 右侧只读 diff 视图 + 底部测量面板。
 *
 * 这不是 Revia 的正式界面，只用于 diff 渲染与运行环境的测量。
 */

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { DiffView, ROW_HEIGHT, type DiffViewport } from "./diff/DiffView";
import { buildFileModel, findRowForAnchor, lineNumberOf, rowAt, type FileModel } from "./diff/model";
import { statusLabel, type DiffResponse, type FileChange, type FileVersionResponse, type HighlightedDocument, type HighlightMode, type ViewMode } from "./diff/types";
import { createHighlighter, highlightVersions, isCollapsedByDefault, languageForPath, LARGE_FILE_LINES } from "./highlight";
import {
  formatMs,
  measureScrollFrames,
  measureScrollSteps,
  nextPaint,
  nowStamp,
  probeRaf,
  visibilitySample,
  type FirstScreenSample,
  type HighlightSample,
  type Measurement,
  type ScrollSample,
  type SwitchSample,
} from "./bench/measure";

const DEFAULT_REPO = "/tmp/revia-m0-fixtures/diffs";

/** 把关键步骤写进后端日志文件，便于在没有窗口的情况下排查自动测量。 */
function debugLog(message: string) {
  void invoke("debug_log", { message: `${new Date().toISOString()} ${message}` }).catch(() => {});
}
/** 自动测量的目标文件：1 万行量级的 TypeScript 改动。 */
const AUTO_PATH = "src/generated.ts";
/** syntect 分块高亮的块大小（行）。 */
const SYN_TECT_CHUNK_LINES = 2000;
/** 视图切换时保持位置的策略。 */
type HighlighterEngine = "row-map" | "scroll-top";

export default function App() {
  const [repo, setRepo] = useState(DEFAULT_REPO);
  const [staged, setStaged] = useState(false);
  const [view, setView] = useState<ViewMode>("split");
  const [highlight, setHighlight] = useState<HighlightMode>("none");
  const [diff, setDiff] = useState<DiffResponse | null>(null);
  const [selectedPath, setSelectedPath] = useState<string | null>(null);
  const [expanded, setExpanded] = useState<Record<string, boolean>>({});
  const [highlights, setHighlights] = useState<HighlightedDocument | null>(null);

  const [highlightError, setHighlightError] = useState<string | null>(null);
  const [envLine, setEnvLine] = useState<string>("");
  const [measurements, setMeasurements] = useState<Measurement[]>([]);
  const [busy, setBusy] = useState(false);
  const [status, setStatus] = useState<string>("就绪");
  const viewportRef = useRef<DiffViewport | null>(null);
  const highlightAbort = useRef<AbortController | null>(null);
  const pendingAnchor = useRef<{ line: number | null; rowIndex: number; mode: ViewMode } | null>(null);
  const autoRan = useRef(false);

  const loadDiff = useCallback(
    async (options: { repo: string; staged: boolean; path: string | null; highlightMode: HighlightMode; fresh: boolean }) => {
      const started = performance.now();
      setBusy(true);
      setStatus("读取 diff…");
      if (options.fresh) {
        setHighlights(null);
        setHighlightError(null);
      }
      debugLog(`loadDiff：invoke 前 stage=1`);
      const response = await invoke<DiffResponse>("get_diff", {
        request: { worktree: options.repo, path: options.path, staged: options.staged },
      });
      debugLog(`loadDiff：invoke 返回，${response.files.length} 个文件，parse ${response.parseMs} ms`);
      const dataAt = performance.now();
      const ipcMs = dataAt - started;
      const payloadChars = JSON.stringify(response).length;

      setDiff(response);
      // 同一 tick 内设置选中文件；随后等待提交完成，再读布局。
      setSelectedPath(options.path ?? response.files[0]?.path ?? null);

      // 等到 React 提交后再测：读取视口尺寸会强制同步布局，
      // 能真实反映“第一屏内容已经进入 DOM 并完成排版”的时刻。
      const paint = await nextPaint();
      const layoutAt = performance.now();
      const viewport = globalThis.document.querySelector<HTMLElement>(".diff-scroll");
      const viewportHeight = viewport?.clientHeight ?? null;
      const rowCount = response.files.reduce((total, file) => total + buildFileModel(file).rows.length, 0);
      const hunkCount = response.files.reduce((total, file) => total + file.hunks.length, 0);
      const firstScreen: FirstScreenSample = {
        parseMs: response.parseMs,
        ipcMs,
        renderMs: layoutAt - dataAt,
        firstScreenMs: layoutAt - started,
        rafPainted: paint.rafPainted,
        rowCount,
        fileCount: response.files.length,
        hunkCount,
        payloadChars,
        viewportHeight,
        devicePixelRatio: window.devicePixelRatio,
      };
      // 记录每个文件的默认显示决策，作为“lock/二进制/超大文件默认收起”的实测证据。
      for (const file of response.files) {
        const lines = file.hunks.reduce((total, hunk) => total + hunk.lines.length, 0);
        const collapsed = isCollapsedByDefault(file.path, lines, file.binary);
        debugLog(
          `文件 ${file.path}：hunk ${file.hunks.length} / 行 ${lines} / 二进制 ${file.binary} / 默认${collapsed ? "收起" : "展开"}`,
        );
      }
      setStatus(`已加载 ${response.files.length} 个文件、${hunkCount} 个 hunk`);

      return { response, firstScreen, ipcMs };
    },
    [],
  );

  const selectedFile = useMemo(
    () => diff?.files.find((file) => file.path === selectedPath) ?? null,
    [diff, selectedPath],
  );
  const selectedModel = useMemo(() => (selectedFile ? buildFileModel(selectedFile) : null), [selectedFile]);

  const applyHighlight = useCallback(
    async (
      response: DiffResponse,
      path: string | null,
      mode: HighlightMode,
      options: { force?: boolean } = {},
    ): Promise<HighlightSample | null> => {
      if (mode === "none" || !path) {
        setHighlights(null);
        return null;
      }
      const file = response.files.find((candidate) => candidate.path === path);
      if (!file) return null;
      const collapsed = isCollapsedByDefault(
        path,
        file.hunks.reduce((total, hunk) => total + hunk.lines.length, 0),
        file.binary,
      );
      // 默认收起只影响初次显示；测量等同于用户点了“展开”。
      if (collapsed && !options.force) {
        setHighlights(null);
        return null;
      }
      highlightAbort.current?.abort();
      const controller = new AbortController();
      highlightAbort.current = controller;
      setStatus(`${mode} 高亮中…`);
      try {
        debugLog(`高亮开始：${path} / ${mode}`);
        const versions = await invoke<FileVersionResponse>("get_file_versions", {
          request: { worktree: response.worktree, path, checkIntroduced: true },
        });
        debugLog(
          `文件版本：head ${versions.head?.length ?? 0} 字节 / working ${versions.working?.length ?? 0} / staged ${versions.staged?.length ?? 0}`,
        );
        const language = languageForPath(path);
        const highlighter = createHighlighter(mode);

        // 与 Rust 侧一致：末尾换行不额外产生一行。
        const countLines = (text: string | null | undefined) =>
          text ? text.split("\n").length - (text.endsWith("\n") ? 1 : 0) : 0;
        const oldContent = versions.head;
        const newContent = versions.working ?? versions.staged ?? null;
        const oldLines = countLines(oldContent);
        const newLines = countLines(newContent);

        debugLog(`${path} / ${mode} 的文件版本已就绪，开始高亮准备`);
        // 可见区域优先：先让当前视图有颜色，再在后台补齐其余部分。
        const viewport = globalThis.document.querySelector<HTMLElement>(".diff-scroll");
        const visibleRows = viewport ? Math.ceil(viewport.clientHeight / ROW_HEIGHT) : 60;
        const firstRow = viewport ? Math.floor(viewport.scrollTop / ROW_HEIGHT) : 0;
        const visibleFrom = Math.max(0, firstRow - 40);
        const visibleTo = firstRow + visibleRows + 40;

        debugLog(`分块高亮：old ${oldLines} 行 / new ${newLines} 行，可见 ${visibleFrom}-${visibleTo}`);
        const { document: highlighted, oldMs, newMs, chunkTimings } = await highlightVersions(
          highlighter,
          {
            old: oldContent ? { path, language, content: oldContent, signal: controller.signal } : null,
            new: newContent ? { path, language, content: newContent, signal: controller.signal } : null,
          },
          {
            visibleFrom,
            visibleTo,
            chunkSize: SYN_TECT_CHUNK_LINES,
            totalLines: { old: oldLines, new: newLines },
            signal: controller.signal,
          },
        );

        let tokenCount = 0;
        for (const tokens of highlighted.old.values()) tokenCount += tokens.length;
        for (const tokens of highlighted.new.values()) tokenCount += tokens.length;
        const rerenderStart = performance.now();
        setHighlights(highlighted);
        // 强制同步布局，得到“带高亮的可见区域已提交”的耗时。
        void globalThis.document.querySelector<HTMLElement>(".diff-scroll")?.scrollHeight;
        const rerenderMs = performance.now() - rerenderStart;
        setHighlightError(null);
        setStatus(`${mode} 高亮完成（${highlighted.highlightedLines}/${highlighted.totalLines} 行）`);
        return {
          engine: highlighter.mode,
          totalMs: oldMs + newMs,
          oldMs,
          newMs,
          engineMs: highlighted.engineMs,
          syntax: highlighted.syntax,
          theme: highlighted.theme,
          tokenLines: highlighted.old.size + highlighted.new.size,
          tokenCount,
          rerenderMs,
          complete: highlighted.complete,
          highlightedLines: highlighted.highlightedLines,
          totalLines: highlighted.totalLines,
          chunks: chunkTimings.length,
        };
      } catch (error) {
        if ((error as DOMException)?.name === "AbortError") return null;
        debugLog(`高亮失败（${mode}）：${error instanceof Error ? error.message : String(error)}`);
        setHighlights(null);
        setHighlightError(error instanceof Error ? error.message : String(error));
        setStatus(`${mode} 高亮失败`);
        return null;
      }
    },
    [],
  );

  /** 视图切换的位置保持：记住锚点行，切换后把同一行放回顶部。 */
  const keepAnchor = useCallback(
    (model: FileModel, targetView: ViewMode, engine: HighlighterEngine) => {
      const element = document.querySelector<HTMLElement>(".diff-scroll");
      if (!element) return;
      const before = rowAt(model, element.scrollTop, ROW_HEIGHT);
      if (!before) return;
      const targetIndex = engine === "scroll-top" ? 0 : findRowForAnchor(model, before);
      pendingAnchor.current = { line: lineNumberOf(before), rowIndex: targetIndex, mode: targetView };
    },
    [],
  );

  const applyView = useCallback(
    (targetView: ViewMode, engine: HighlighterEngine = "row-map") => {
      if (selectedModel) keepAnchor(selectedModel, targetView, engine);
      setView(targetView);
    },
    [keepAnchor, selectedModel],
  );

  const runBenchmark = useCallback(
    async (mode: HighlightMode, options: { view?: ViewMode; path?: string | null; keepBusy?: boolean } = {}) => {
      if (busy && !options.keepBusy) return;
      const targetView = options.view ?? view;
      const targetPath = options.path !== undefined ? options.path : selectedPath;
      const label = `${mode} / ${targetView}`;
      if (targetView !== view) {
        // 统一视图的行数可能约为并排视图的两倍，先只渲染顶部，再由布局效应滚到锚点行。
        applyView(targetView, targetView === "unified" ? "scroll-top" : "row-map");
      } else {
        applyView(targetView);
      }
      try {
        debugLog(`runBenchmark 调用 loadDiff：${label}，path=${targetPath ?? "(全部)"}`);
        const { response, firstScreen } = await loadDiff({
          repo,
          staged,
          path: targetPath,
          highlightMode: mode,
          fresh: true,
        });
        const highlightSample = await applyHighlight(response, targetPath, mode, { force: true });
        const element = document.querySelector<HTMLElement>(".diff-scroll");
        let scroll: ScrollSample | null = null;
        let switchView: SwitchSample | null = null;
        const visibility = visibilitySample();
        visibility.rafPerSecond = await probeRaf(1);
        debugLog(`可见性：state=${visibility.state} viewport=${visibility.viewportWidth}x${visibility.viewportHeight} rAF/s=${visibility.rafPerSecond}`);
        if (element) {
          // 窗口不可见时 rAF 停摆，退回“逐屏推进 + 同步布局”的测量方式。
          if (visibility.rafPerSecond > 5) {
            setStatus("滚动测量中（逐帧）…");
            scroll = await measureScrollFrames(element, 5000, 14);
          } else {
            setStatus("滚动测量中（逐屏，窗口不可见）…");
            scroll = measureScrollSteps(element, 120);
          }
          setStatus(`${mode} 滚动测量完成，切换视图…`);
          const file = response.files.find((candidate) => candidate.path === targetPath);
          switchView = file ? await measureSwitchView(element, buildFileModel(file), targetView) : null;
        }
        const measurement: Measurement = {
          id: 0,
          label,
          startedAt: nowStamp(),
          view: targetView,
          highlight: mode,
          visibility,
          firstScreen,
          scroll,
          switchView,
          highlightSample,
        };
        setMeasurements((previous) => {
          measurement.id = previous.length + 1;
          return [measurement, ...previous];
        });
        setStatus(`${label} 测量完成`);
        return measurement;
      } catch (error) {
        debugLog(`测量失败（${label}）：${error instanceof Error ? error.message : String(error)}`);
        setStatus(`测量失败：${error instanceof Error ? error.message : String(error)}`);
        return null;
      } finally {
        if (!options.keepBusy) setBusy(false);
      }
    },
    [applyHighlight, applyView, busy, loadDiff, repo, selectedPath, staged, view],
  );

  const measureSwitchView = useCallback(
    async (element: HTMLElement, model: FileModel, currentView: ViewMode): Promise<SwitchSample> => {
      element.scrollTop = Math.floor(model.rows.length / 2) * ROW_HEIGHT;
      const before = rowAt(model, element.scrollTop, ROW_HEIGHT);
      const scrollTopBefore = element.scrollTop;
      const started = performance.now();
      const targetView: ViewMode = currentView === "split" ? "unified" : "split";
      applyView(targetView, targetView === "unified" ? "scroll-top" : "row-map");
      await nextPaint();
      const after = rowAt(model, element.scrollTop, ROW_HEIGHT);
      return {
        anchorLineBefore: before ? lineNumberOf(before) : null,
        anchorLineAfter: after ? lineNumberOf(after) : null,
        scrollTopBefore,
        scrollTopAfter: element.scrollTop,
        layoutMs: performance.now() - started,
      };
    },
    [applyView],
  );

  /** 自动测量：`REVIA_M0_MEASURE=1` 时按顺序跑完全部组合并写文件。 */
  const runAutoMeasurement = useCallback(
    async (repoPath: string, outputPath: string | null, only: string[] | null, targetFile: string | null) => {
    debugLog(`runAutoMeasurement 进入：repo=${repoPath} only=${only?.join(",") ?? "(全部)"}`);
    const all: { mode: HighlightMode; view: ViewMode }[] = [
      { mode: "none", view: "split" },
      { mode: "shiki", view: "split" },
      { mode: "syntect", view: "split" },
      { mode: "none", view: "unified" },
      { mode: "shiki", view: "unified" },
      { mode: "syntect", view: "unified" },
    ];
    const combos = only && only.length > 0
      ? all.filter((combo) => only.includes(`${combo.mode}:${combo.view}`))
      : all;
    const autoPath = targetFile && targetFile.length > 0 ? targetFile : targetFile === "" ? null : AUTO_PATH;
    const results: Measurement[] = [];
    setBusy(true);
    for (const combo of combos) {
      setStatus(`自动测量 ${combo.mode} / ${combo.view}…`);
      debugLog(`开始组合 ${combo.mode}/${combo.view}`);
      const measurement = await runBenchmark(combo.mode, { view: combo.view, path: autoPath, keepBusy: true });
      debugLog(
        `组合 ${combo.mode}/${combo.view}：${
          measurement
            ? `首屏 ${measurement.firstScreen.firstScreenMs?.toFixed(1)} ms，滚动帧 ${
                measurement.scroll?.frames ?? 0
              }`
            : "失败"
        }`,
      );
      if (measurement) results.push(measurement);
    }
    setStatus("自动测量完成，写入结果…");
    try {
      const written = await invoke<string | null>("write_measurement", {
        payload: {
          repo: repoPath,
          outputPath,
          combos: results,
          userAgent: navigator.userAgent,
        },
      });
      debugLog(`写入结果：${written ?? "未配置输出路径"}`);
      setStatus(written ? `测量结果已写入 ${written}` : "测量完成（未配置输出路径）");
    } catch (error) {
      debugLog(`写入结果失败：${error instanceof Error ? error.message : String(error)}`);
      setStatus(`写入测量结果失败：${error instanceof Error ? error.message : String(error)}`);
    }
    setBusy(false);
  }, [runBenchmark]);

  const load = useCallback(async () => {
    try {
      const { response } = await loadDiff({ repo, staged, path: null, highlightMode: highlight, fresh: true });
      const first = response.files[0]?.path ?? null;
      setSelectedPath(first);
      if (first) {
        await applyHighlight(response, first, highlight);
      }
    } catch (error) {
      setStatus(`加载失败：${error instanceof Error ? error.message : String(error)}`);
    } finally {
      setBusy(false);
    }
  }, [applyHighlight, highlight, loadDiff, repo, staged]);

  const loadWorktree = useCallback(async () => {
    const info = await invoke<{ gitPath: string; gitVersion: string; env: { elapsedMs: number; shell: string; usedFallback: boolean; pathEntries: string[] } }>(
      "resolve_worktree",
      { request: { worktree: repo, reloadEnv: false } },
    );
    const node = info.env.pathEntries.length;
    setEnvLine(
      `${info.gitPath} · ${info.gitVersion} · shell ${info.env.shell} · 解析 ${info.env.elapsedMs} ms${info.env.usedFallback ? "（回退）" : ""} · PATH 条目 ${node}`,
    );
  }, [repo]);

  // 心跳：窗口不可见时用来区分“主线程卡死”和“合成器停摆”。
  useEffect(() => {
    const timer = setInterval(() => {
      debugLog(`心跳 ${Math.round(performance.now())} ms（visibility=${globalThis.document.visibilityState}）`);
    }, 2000);
    return () => clearInterval(timer);
  }, []);

  useEffect(() => {
    debugLog("界面已挂载，读取 measure_config");
    void invoke<{
      autoRun: boolean;
      repo: string | null;
      outputPath: string | null;
      only: string[] | null;
      path: string | null;
    }>("measure_config")
      .then((config) => {
        const target = config.repo ?? repo;
        if (target !== repo) setRepo(target);
        if (config.autoRun) {
          if (autoRan.current) {
            debugLog("自动测量已在运行，忽略重复触发");
            return;
          }
          autoRan.current = true;
          debugLog(`自动测量启动：repo=${target} output=${config.outputPath ?? "(未配置)"}`);
          void runAutoMeasurement(target, config.outputPath, config.only, config.path);
          return;
        }
        void load();
        void loadWorktree().catch((error) => setEnvLine(`环境解析失败：${String(error)}`));
      })
      .catch(() => {
        void load();
      });
    // 只在首次挂载时执行
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    if (envLine) return;
    void loadWorktree().catch((error) => setEnvLine(`环境解析失败：${String(error)}`));
  }, [envLine, loadWorktree]);

  // 视图刚切换完，把之前记住的锚点行重新放回顶部，保证“停在同一位置”。
  useLayoutEffect(() => {
    const anchor = pendingAnchor.current;
    if (!anchor || !selectedModel || anchor.mode !== view) return;
    const element = document.querySelector<HTMLElement>(".diff-scroll");
    if (element) {
      const rows = globalThis.document.querySelectorAll(".diff-scroll > div > div").length;
      const target = anchor.rowIndex * ROW_HEIGHT;
      // 只做一次定位，并且确保目标行落在当前的渲染窗口内。
      element.scrollTop = target;
      debugLog(`视图切换定位：首行 ${anchor.line ?? "?"} -> 行 ${anchor.rowIndex}（目标 ${target}px，窗口内已渲染 ${rows} 行）`);
    }
    pendingAnchor.current = null;
  }, [selectedModel, view]);

  const selectFile = useCallback(
    async (file: FileChange) => {
      setSelectedPath(file.path);
      setHighlights(null);
      if (diff) {
        await applyHighlight(diff, file.path, highlight);
      }
    },
    [applyHighlight, diff, highlight],
  );

  return (
    <div className="app">
      <aside className="sidebar">
        <div className="sidebar-head">
          <input
            value={repo}
            spellCheck={false}
            onChange={(event) => setRepo(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Enter") void load();
            }}
          />
          <button type="button" onClick={() => void load()} disabled={busy}>
            加载
          </button>
        </div>
        <label className="row">
          <input type="checkbox" checked={staged} onChange={(event) => setStaged(event.target.checked)} />
          已暂存改动
        </label>
        <div className="file-list">
          {diff?.files.map((file) => {
            const lineCount = file.hunks.reduce((total, hunk) => total + hunk.lines.length, 0);
            const collapsed = isCollapsedByDefault(file.path, lineCount, file.binary);
            const isExpanded = expanded[file.path] ?? !collapsed;
            return (
              <div key={file.path} className={file.path === selectedPath ? "file current" : "file"}>
                <button type="button" className="file-main" onClick={() => void selectFile(file)}>
                  <span className={`status status-${file.status}`}>{statusLabel[file.status]}</span>
                  <span className="file-name">{file.path}</span>
                  {collapsed && !isExpanded ? <span className="badge">收起</span> : null}
                </button>
                {collapsed ? (
                  <button
                    type="button"
                    className="file-toggle"
                    onClick={() => setExpanded((previous) => ({ ...previous, [file.path]: !isExpanded }))}
                  >
                    {isExpanded ? "收起" : "展开"}
                  </button>
                ) : null}
              </div>
            );
          })}
          {diff && diff.files.length === 0 ? <div className="empty">没有改动</div> : null}
        </div>
        <div className="sidebar-foot">{envLine || "解析环境…"}</div>
      </aside>

      <main className="main">
        <div className="toolbar">
          <div className="group">
            <span className="label">视图</span>
            <button type="button" className={view === "split" ? "active" : ""} onClick={() => applyView("split")}>
              并排
            </button>
            <button type="button" className={view === "unified" ? "active" : ""} onClick={() => applyView("unified")}>
              统一
            </button>
          </div>
          <div className="group">
            <span className="label">高亮</span>
            {(["none", "shiki", "syntect"] as HighlightMode[]).map((mode) => (
              <button
                key={mode}
                type="button"
                className={highlight === mode ? "active" : ""}
                onClick={() => {
                  setHighlight(mode);
                  if (diff) void applyHighlight(diff, selectedPath, mode);
                }}
              >
                {mode === "none" ? "无" : mode}
              </button>
            ))}
          </div>
          <div className="group">
            <span className="label">测量</span>
            <button type="button" onClick={() => void runBenchmark("none")} disabled={busy}>
              none
            </button>
            <button type="button" onClick={() => void runBenchmark("shiki")} disabled={busy}>
              shiki
            </button>
            <button type="button" onClick={() => void runBenchmark("syntect")} disabled={busy}>
              syntect
            </button>
          </div>
          <div className="spacer" />
          <span className="status-text">{busy ? "…" : status}</span>
        </div>

        {highlightError ? <div className="error">高亮错误：{highlightError}</div> : null}

        {selectedFile && selectedModel ? (
          selectedModel.rows.length === 0 ? (
            <div className="empty large">
              {selectedFile.binary ? "二进制文件，不渲染内容" : "这个文件没有可显示的 hunk"}
            </div>
          ) : (
            <DiffView file={selectedFile} view={view} highlights={highlights} viewportRef={viewportRef} />
          )
        ) : (
          <div className="empty large">选择左侧文件查看 diff</div>
        )}

        <MeasurementPanel
          measurements={measurements}
          onClear={() => setMeasurements([])}
        />
      </main>
    </div>
  );
}

function MeasurementPanel({ measurements, onClear }: { measurements: Measurement[]; onClear: () => void }) {
  const [open, setOpen] = useState(true);
  const latest = measurements[0];
  return (
    <section className="panel">
      <header>
        <button type="button" onClick={() => setOpen((value) => !value)}>
          {open ? "▼" : "▶"} 测量面板（{measurements.length}）
        </button>
        <span className="muted">只渲染可见区域 · 行高 {ROW_HEIGHT}px · 超大文件阈值 {LARGE_FILE_LINES} 行</span>
        <div className="spacer" />
        <button type="button" onClick={onClear} disabled={measurements.length === 0}>
          清空
        </button>
        <button
          type="button"
          disabled={measurements.length === 0}
          onClick={() => {
            const blob = new Blob([JSON.stringify(measurements, null, 2)], { type: "application/json" });
            const url = URL.createObjectURL(blob);
            const anchor = document.createElement("a");
            anchor.href = url;
            anchor.download = `revia-m0-measurements-${Date.now()}.json`;
            anchor.click();
            URL.revokeObjectURL(url);
          }}
        >
          导出 JSON
        </button>
      </header>
      {open && latest ? (
        <div className="panel-body">
          <div className="metrics">
            <Metric label="标签" value={latest.label} />
            <Metric label="Rust 解析" value={formatMs(latest.firstScreen.parseMs)} />
            <Metric label="IPC" value={formatMs(latest.firstScreen.ipcMs)} />
            <Metric label="前端渲染" value={formatMs(latest.firstScreen.renderMs)} />
            <Metric label="首屏合计" value={formatMs(latest.firstScreen.firstScreenMs)} />
            <Metric label="行数" value={String(latest.firstScreen.rowCount ?? "—")} />
            <Metric label="文件 / hunk" value={`${latest.firstScreen.fileCount ?? "—"} / ${latest.firstScreen.hunkCount ?? "—"}`} />
            <Metric label="IPC 字符数" value={latest.firstScreen.payloadChars == null ? "—" : (latest.firstScreen.payloadChars / 1024 / 1024).toFixed(1) + " M"} />
            <Metric label="视口高度" value={`${latest.firstScreen.viewportHeight ?? "—"} px @${latest.firstScreen.devicePixelRatio ?? "?"}x`} />
            <Metric label="窗口可见性" value={`${latest.visibility.state} · rAF/s ${latest.visibility.rafPerSecond.toFixed(1)}`} />
            <Metric label="合成帧" value={latest.firstScreen.rafPainted ? "已提交" : "未提交（窗口不可见）"} />
          </div>
          {latest.highlightSample ? (
            <div className="metrics">
              <Metric label="高亮引擎" value={latest.highlightSample.engine} />
              <Metric label="高亮总耗时" value={formatMs(latest.highlightSample.totalMs)} />
              <Metric label="旧版本" value={formatMs(latest.highlightSample.oldMs)} />
              <Metric label="新版本" value={formatMs(latest.highlightSample.newMs)} />
              <Metric label="引擎报告" value={latest.highlightSample.engineMs == null ? "—" : formatMs(latest.highlightSample.engineMs)} />
              <Metric label="token 行" value={String(latest.highlightSample.tokenLines)} />
              <Metric label="高亮后重渲染" value={formatMs(latest.highlightSample.rerenderMs)} />
              <Metric label="语法 / 主题" value={`${latest.highlightSample.syntax ?? "—"} / ${latest.highlightSample.theme ?? "—"}`} />
            </div>
          ) : null}
          {latest.scroll ? (
            <div className="metrics">
              <Metric label="滚动模式" value={latest.scroll.mode === "frame" ? "逐帧" : "逐屏"} />
              <Metric label="样本数" value={`${latest.scroll.frames}（${latest.scroll.seconds.toFixed(1)}s）`} />
              <Metric label="间隔 p50 / p95" value={`${formatMs(latest.scroll.frameP50)} / ${formatMs(latest.scroll.frameP95)}`} />
              <Metric label="间隔 max" value={formatMs(latest.scroll.frameMax)} />
              <Metric label="超过 20ms" value={String(latest.scroll.droppedFrames)} />
              <Metric label="滚动距离" value={`${Math.round(latest.scroll.scrollTopTo - latest.scroll.scrollTopFrom)} px`} />
              <Metric label="每屏渲染 p50 / p95" value={`${formatMs(latest.scroll.stepRenderP50)} / ${formatMs(latest.scroll.stepRenderP95)}`} />
            </div>
          ) : null}
          {latest.switchView ? (
            <div className="metrics">
              <Metric label="切换前首行" value={String(latest.switchView.anchorLineBefore ?? "—")} />
              <Metric label="切换后首行" value={String(latest.switchView.anchorLineAfter ?? "—")} />
              <Metric label="滚动位置变化" value={`${Math.round(latest.switchView.scrollTopAfter - latest.switchView.scrollTopBefore)} px`} />
              <Metric label="布局耗时" value={formatMs(latest.switchView.layoutMs)} />
            </div>
          ) : null}
          <pre className="raw">{JSON.stringify(latest, null, 2)}</pre>
        </div>
      ) : null}
    </section>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric">
      <span className="metric-label">{label}</span>
      <span className="metric-value">{value}</span>
    </div>
  );
}
