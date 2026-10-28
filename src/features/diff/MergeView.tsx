/**
 * 合并页签（Q143，设计见 docs/milestones/m4/merge-design.md），照 Rebased 的合并窗口：
 * 你的版本 | 结果 | 对方的版本 三栏，中间两条窄栏用弧形色带连起对应的块，
 * » / « 把一侧的一块放进结果，✕ 忽略这一侧；结果栏可以直接编辑。
 *
 * 块的位置与种类来自 Rust（Git 生成）。块状态放在结果栏编辑器的状态里，
 * 与文本一起撤销、重做。“应用”由 Rust 写前校验、先存回收区、写入并 `git add`。
 */

import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { Annotation, Compartment, EditorState, StateEffect, StateField, Transaction, type Range } from "@codemirror/state";
import { Decoration, EditorView, keymap, lineNumbers, type DecorationSet, type ViewUpdate } from "@codemirror/view";
import { defaultKeymap, history, historyKeymap, invertedEffects } from "@codemirror/commands";
import type { MergeLayout } from "@/types/bindings/MergeLayout";
import type { MergeChunk } from "@/types/bindings/MergeChunk";
import type { MergeSide } from "@/types/bindings/MergeSide";
import { ipc } from "@/lib/ipc";
import { messages } from "@/lib/i18n/zh-CN";
import { getErrorMessage, toAppError } from "@/lib/error";
import { Codicon } from "@/components/ui/codicon";
import { Modal } from "@/components/app/Dialogs";
import { mapLine, type SyncSegment } from "./syncScroll";
import { bandVisible, drawBand, prepareCanvas, type BandEnds } from "./bands";
import { ROW_HEIGHT } from "./DiffView";
import { rustHighlight } from "@/features/merge/codeHighlight";
import type { MergeWriteRequest } from "@/features/merge/mergeWrite";
import { useMountedRef } from "@/hooks/useMountedRef";
import { mergeEditorAppearance, type MergeAppearanceOptions } from "@/features/merge/editorAppearance";
import {
  applySide,
  chunkContent,
  hasConflictMarkers,
  hasPendingNonConflicts,
  ignoreSide,
  initialStates,
  isResolved,
  mergeTexts,
  sideText,
  syncSegments,
  unresolvedConflicts,
  type ChunkState,
  type MergeSideName,
} from "@/features/merge/mergeModel";

export interface MergeViewProps {
  worktreeId: string;
  path: string;
  fontSize: number;
  fontFamily: string | null;
  softWrap: boolean;
  onClose: () => void;
  /** 写入（应用或接受一侧）：由 App 经全局写门禁与关闭/退出保护执行，并重读原目标。 */
  onWrite: (request: MergeWriteRequest) => Promise<void>;
}

export function MergeView({ worktreeId, path, fontSize, fontFamily, softWrap, onClose, onWrite }: MergeViewProps) {
  const [layout, setLayout] = useState<MergeLayout | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const [generation, setGeneration] = useState(0);

  useEffect(() => {
    let cancelled = false;
    setLayout(null);
    setLoadError(null);
    ipc
      .mergeOpen({ worktreeId, path })
      .then((next) => {
        if (!cancelled) setLayout(next);
      })
      .catch((caught: unknown) => {
        if (!cancelled) setLoadError(getErrorMessage(caught));
      });
    return () => {
      cancelled = true;
    };
  }, [worktreeId, path, generation]);

  const reload = useCallback(() => setGeneration((value) => value + 1), []);

  if (loadError) {
    return (
      <div className="merge-message">
        <p className="font-medium">{messages.merge.loadFailed}</p>
        <p className="text-descriptionforeground">{loadError}</p>
        <button type="button" className="merge-button" onClick={reload}>
          {messages.merge.reload}
        </button>
      </div>
    );
  }
  if (!layout) return <div className="merge-message text-descriptionforeground">{messages.changes.loading}</div>;
  if (layout.unavailable) {
    return <AcceptOnly layout={layout} onWrite={onWrite} onClose={onClose} onReload={reload} />;
  }
  return (
    <MergeEditor
      key={`${layout.fingerprint}/${generation}`}
      layout={layout}
      fontSize={fontSize}
      fontFamily={fontFamily}
      softWrap={softWrap}
      onWrite={onWrite}
      onClose={onClose}
      onReload={reload}
    />
  );
}

// ---------- 块状态（放在结果栏编辑器里，随文本一起撤销） ----------

/** 程序替换（放进结果、忽略）：不算用户手动修改。 */
const programmatic = Annotation.define<boolean>();

const setChunk = StateEffect.define<{ index: number; state: ChunkState }>({
  map: (value, mapping) => {
    const from = mapping.mapPos(value.state.from, -1);
    return { index: value.index, state: { ...value.state, from, to: Math.max(from, mapping.mapPos(value.state.to, 1)) } };
  },
});

const chunkField = StateField.define<ChunkState[]>({
  create: () => [],
  update(value, tr) {
    let next = value;
    if (tr.docChanged) {
      const user = !tr.annotation(programmatic);
      next = value.map((state) => {
        let touched = false;
        if (user) {
          tr.changes.iterChangedRanges((fromA, toA) => {
            if (fromA <= state.to && toA >= state.from) touched = true;
          });
        }
        // 手动改到块边上时把新输入算进块里；其余改动不会碰到块的边界。
        const from = tr.changes.mapPos(state.from, touched ? -1 : 1);
        const to = Math.max(from, tr.changes.mapPos(state.to, touched ? 1 : -1));
        return { ...state, from, to, edited: state.edited || touched };
      });
    }
    for (const effect of tr.effects) {
      if (!effect.is(setChunk)) continue;
      if (next === value) next = value.slice();
      next[effect.value.index] = effect.value.state;
    }
    return next;
  },
});

/** 撤销放进结果、忽略时，同时恢复块状态。 */
const undoChunkEffects = invertedEffects.of((tr) => {
  const effects: StateEffect<{ index: number; state: ChunkState }>[] = [];
  for (const effect of tr.effects) {
    if (!effect.is(setChunk)) continue;
    const index = effect.value.index;
    effects.push(setChunk.of({ index, state: tr.startState.field(chunkField)[index] }));
  }
  return effects;
});

function chunkClass(state: ChunkState, pane: "result" | MergeSideName): string | null {
  if (pane === "result") {
    if (state.edited) return "merge-line-edited";
    if (!isResolved(state)) return state.kind === "conflict" ? "merge-line-conflict" : "merge-line-change";
    return state.ours === "applied" || state.theirs === "applied" ? "merge-line-applied" : null;
  }
  const side = state[pane];
  if (side === "none") return null;
  if (side === "pending") return state.kind === "conflict" ? "merge-line-conflict" : "merge-line-change";
  return side === "applied" ? "merge-line-applied" : "merge-line-ignored";
}

/** 行区间加行底色；空区间在它所在的那一行顶上画一条线。 */
function lineDecorations(state: EditorState, ranges: Array<{ from: number; to: number; className: string }>): DecorationSet {
  const doc = state.doc;
  const items: Range<Decoration>[] = [];
  for (const range of ranges) {
    if (range.to <= range.from) {
      const line = doc.lineAt(Math.min(range.from, doc.length));
      items.push(Decoration.line({ class: `${range.className} merge-line-gap` }).range(line.from));
      continue;
    }
    let position = range.from;
    while (position < range.to) {
      const line = doc.lineAt(position);
      items.push(Decoration.line({ class: range.className }).range(line.from));
      if (line.to >= doc.length) break;
      position = line.to + 1;
    }
  }
  items.sort((a, b) => a.from - b.from);
  return Decoration.set(items, true);
}

const resultDecorations = EditorView.decorations.compute([chunkField, "doc"], (state) =>
  lineDecorations(
    state,
    state.field(chunkField).flatMap((chunk) => {
      const className = chunkClass(chunk, "result");
      return className ? [{ from: chunk.from, to: chunk.to, className }] : [];
    }),
  ),
);

const setSideDecorations = StateEffect.define<DecorationSet>();
const sideDecorationField = StateField.define<DecorationSet>({
  create: () => Decoration.none,
  update: (value, tr) => {
    for (const effect of tr.effects) if (effect.is(setSideDecorations)) return effect.value;
    return value;
  },
  provide: (field) => EditorView.decorations.from(field),
});

// ---------- 行号与几何 ----------

function lineFrom(view: EditorView, line: number): number {
  const doc = view.state.doc;
  const index = Math.max(0, Math.floor(line));
  return index < doc.lines ? doc.line(index + 1).from : doc.length;
}

/** 字符位置所在行（0 基）；作为区间末端且不在行首时，算到下一行。 */
function lineIndexOf(view: EditorView, position: number, end = false): number {
  const line = view.state.doc.lineAt(position);
  return end && line.from !== position ? line.number : line.number - 1;
}

/** 某一行顶边相对这一栏可视区顶部的像素。 */
function lineY(view: EditorView, line: number): number {
  const doc = view.state.doc;
  const block = view.lineBlockAt(lineFrom(view, line));
  const top = line < doc.lines ? block.top : block.bottom;
  return (top + view.documentTop - view.scrollDOM.getBoundingClientRect().top) / view.scaleY;
}

/** 可视区中线所在的行（浮点）。 */
function middleLine(view: EditorView): number {
  const scroller = view.scrollDOM;
  // 首先让 CodeMirror 刷新待测量的几何；block 坐标包含界面缩放，scrollTop 不含。
  view.lineBlockAtHeight(0);
  const y = scroller.getBoundingClientRect().top + scroller.clientHeight * view.scaleY / 2 - view.documentTop;
  const block = view.lineBlockAtHeight(y);
  const index = view.state.doc.lineAt(block.from).number - 1;
  return index + Math.min(1, Math.max(0, (y - block.top) / Math.max(1, block.height)));
}

function scrollTopFor(view: EditorView, line: number): number {
  const position = Math.min(view.state.doc.lines, Math.max(0, line));
  const whole = Math.floor(position);
  const block = view.lineBlockAt(lineFrom(view, whole));
  const y = whole < view.state.doc.lines ? block.top + (position - whole) * block.height : block.bottom;
  return (y + view.documentPadding.top) / view.scaleY - view.scrollDOM.clientHeight / 2;
}

function resultLines(view: EditorView, chunks: ChunkState[]): Array<[number, number]> {
  return chunks.map((chunk) => {
    const start = lineIndexOf(view, chunk.from);
    return [start, Math.max(start, lineIndexOf(view, chunk.to, true))];
  });
}

// ---------- 编辑器 ----------

interface Panes {
  left: EditorView;
  result: EditorView;
  right: EditorView;
}

type ActTarget = { index: number; side: MergeSideName; op: "apply" | "ignore" };

function MergeEditor({
  layout,
  fontSize,
  fontFamily,
  softWrap,
  onWrite,
  onClose,
  onReload,
}: {
  layout: MergeLayout;
  fontSize: number;
  fontFamily: string | null;
  softWrap: boolean;
  onWrite: (request: MergeWriteRequest) => Promise<void>;
  onClose: () => void;
  onReload: () => void;
}) {
  const texts = useMemo(() => mergeTexts(layout.leftText, layout.baseText, layout.rightText), [layout]);
  const leftHost = useRef<HTMLDivElement>(null);
  const resultHost = useRef<HTMLDivElement>(null);
  const rightHost = useRef<HTMLDivElement>(null);
  const panesRef = useRef<Panes | null>(null);
  const [appearance] = useState(() => new Compartment());
  const appearanceOptions = useRef<MergeAppearanceOptions>({ fontSize, fontFamily, softWrap });
  appearanceOptions.current = { fontSize, fontFamily, softWrap };
  const configuredAppearance = useRef<MergeAppearanceOptions | null>(null);
  const geometryRef = useRef<{ remember: () => void; refresh: () => void } | null>(null);
  const [panes, setPanes] = useState<Panes | null>(null);
  const [chunks, setChunks] = useState<ChunkState[]>(() => initialStates(layout.chunks, layout.resultText));
  const [tick, setTick] = useState(0);
  const [touched, setTouched] = useState(false);
  const [confirm, setConfirm] = useState<null | "apply" | "cancel" | MergeSide>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{ message: string; stale: boolean } | null>(null);
  const mounted = useMountedRef();

  const conflicts = unresolvedConflicts(chunks);

  // 外观通过 Compartment 重配；只有换合并快照才重建编辑器。
  useLayoutEffect(() => {
    if (!leftHost.current || !resultHost.current || !rightHost.current) return;
    const highlight = (pane: string) =>
      rustHighlight({
        worktreeId: layout.worktreeId,
        documentId: `${layout.worktreeId}/merge/${pane}/${layout.path}`,
        language: layout.language,
      });
    const readOnly = [EditorState.readOnly.of(true), EditorView.editable.of(false), sideDecorationField];
    let frame = 0;
    let disposed = false;
    const redraw = () => {
      if (disposed || frame) return;
      frame = requestAnimationFrame(() => {
        frame = 0;
        setTick((value) => value + 1);
      });
    };
    let scheduleGeometry: (source?: EditorView) => void = () => redraw();
    const common = [
      lineNumbers(),
      EditorState.tabSize.of(4),
      appearance.of(mergeEditorAppearance(appearanceOptions.current, ROW_HEIGHT)),
      EditorView.updateListener.of((update: ViewUpdate) => {
        if (update.geometryChanged || update.viewportChanged) scheduleGeometry();
      }),
    ];
    const left = new EditorView({
      parent: leftHost.current,
      state: EditorState.create({ doc: layout.leftText, extensions: [...common, ...readOnly, highlight("ours")] }),
    });
    const right = new EditorView({
      parent: rightHost.current,
      state: EditorState.create({ doc: layout.rightText, extensions: [...common, ...readOnly, highlight("theirs")] }),
    });
    const result = new EditorView({
      parent: resultHost.current,
      state: EditorState.create({
        doc: layout.resultText,
        extensions: [
          ...common,
          history(),
          keymap.of([...defaultKeymap, ...historyKeymap]),
          chunkField.init(() => initialStates(layout.chunks, layout.resultText)),
          undoChunkEffects,
          resultDecorations,
          highlight("result"),
          EditorView.updateListener.of((update: ViewUpdate) => {
            const after = update.state.field(chunkField);
            if (update.startState.field(chunkField) !== after) setChunks(after);
            if (update.docChanged) setTouched(true);
            if (update.docChanged) scheduleGeometry(update.view);
          }),
        ],
      }),
    });
    const created = { left, result, right };
    const views = [left, result, right];
    panesRef.current = created;
    configuredAppearance.current = appearanceOptions.current;
    setPanes(created);

    // 同步滚动：以被滚动的一栏为准，经结果栏映射到另外两栏（设计第 2 节）。
    const expected = new Map<EditorView, number>();
    let active = result;
    let savedAnchor: { view: EditorView; line: number } | null = null;
    const measureKey = {};
    const segmentsFor = (side: MergeSideName): SyncSegment[] =>
      syncSegments(layout.chunks, resultLines(result, result.state.field(chunkField)), side);
    scheduleGeometry = (source) => {
      if (disposed) return;
      if (source) {
        active = source;
        savedAnchor = null;
      }
      for (const view of views) view.requestMeasure();
      result.requestMeasure({
        key: measureKey,
        read: () => {
          if (disposed) return null;
          // 三栏都完成 CodeMirror 的行块测量，再读换行后的高度。这里不 dispatch。
          for (const view of views) view.lineBlockAtHeight(0);
          const origin = active;
          const anchor = savedAnchor?.view === origin ? savedAnchor : null;
          const sourceLine = anchor?.line ?? middleLine(origin);
          const ours = segmentsFor("ours");
          const theirs = segmentsFor("theirs");
          const resultLine = origin === left
            ? mapLine("right", sourceLine, ours)
            : origin === right ? mapLine("right", sourceLine, theirs) : sourceLine;
          const targets = views.flatMap((view) => {
            if (view === origin && !anchor) return [];
            const line = view === origin ? sourceLine : view === left
              ? mapLine("left", resultLine, ours)
              : view === right ? mapLine("left", resultLine, theirs) : resultLine;
            const max = Math.max(0, view.scrollDOM.scrollHeight - view.scrollDOM.clientHeight);
            const top = Math.max(0, Math.min(scrollTopFor(view, line), max));
            return Math.abs(view.scrollDOM.scrollTop - top) < 1 ? [] : [{ view, top }];
          });
          return { anchor, targets };
        },
        write: (measurement) => {
          if (!measurement || disposed) return;
          if (savedAnchor === measurement.anchor) savedAnchor = null;
          for (const { view, top } of measurement.targets) {
            expected.set(view, top);
            view.scrollDOM.scrollTop = top;
          }
          redraw();
        },
      });
    };
    geometryRef.current = {
      remember: () => { savedAnchor = { view: active, line: middleLine(active) }; },
      refresh: () => scheduleGeometry(),
    };
    const listeners = views.map((view) => {
      const listener = () => {
        redraw();
        const wanted = expected.get(view);
        expected.delete(view);
        if (wanted !== undefined && Math.abs(view.scrollDOM.scrollTop - wanted) < 1.5) return;
        // 外观重配引起的浏览器滚动不改变主动栏，也不覆盖重配前的锚点。
        if (!savedAnchor) active = view;
        scheduleGeometry();
      };
      view.scrollDOM.addEventListener("scroll", listener, { passive: true });
      return () => view.scrollDOM.removeEventListener("scroll", listener);
    });
    const refresh = () => scheduleGeometry();
    const resize = new ResizeObserver(refresh);
    for (const host of [leftHost.current, resultHost.current, rightHost.current]) resize.observe(host);
    document.fonts.addEventListener("loadingdone", refresh);
    // 打开时停在第一处未解决的冲突（没有就停在第一块）。
    const states = result.state.field(chunkField);
    const first = states.find((state) => state.kind === "conflict" && !isResolved(state)) ?? states[0];
    if (first) {
      result.dispatch({ selection: { anchor: first.from }, effects: EditorView.scrollIntoView(first.from, { y: "center" }) });
    }
    refresh();
    return () => {
      disposed = true;
      cancelAnimationFrame(frame);
      resize.disconnect();
      document.fonts.removeEventListener("loadingdone", refresh);
      listeners.forEach((remove) => remove());
      left.destroy();
      result.destroy();
      right.destroy();
      panesRef.current = null;
      geometryRef.current = null;
      configuredAppearance.current = null;
    };
  }, [layout, appearance]);

  useLayoutEffect(() => {
    const current = panesRef.current;
    const previous = configuredAppearance.current;
    if (!current || (previous?.fontSize === fontSize && previous.fontFamily === fontFamily && previous.softWrap === softWrap)) return;
    geometryRef.current?.remember();
    const next = { fontSize, fontFamily, softWrap };
    const effects = appearance.reconfigure(mergeEditorAppearance(next, ROW_HEIGHT));
    for (const view of [current.left, current.result, current.right]) {
      view.dispatch({ effects, annotations: Transaction.addToHistory.of(false) });
    }
    configuredAppearance.current = next;
    geometryRef.current?.refresh();
  }, [appearance, fontSize, fontFamily, softWrap]);

  // 两侧栏的块底色随块状态变化。
  useEffect(() => {
    if (!panes) return;
    const sideRanges = (side: MergeSideName, view: EditorView) =>
      chunks.flatMap((state, index) => {
        const className = chunkClass(state, side);
        if (!className) return [];
        const chunk = layout.chunks[index];
        const [start, end] = side === "ours" ? [chunk.oursStart, chunk.oursEnd] : [chunk.theirsStart, chunk.theirsEnd];
        return [{ from: lineFrom(view, start), to: lineFrom(view, end), className }];
      });
    for (const [side, view] of [["ours", panes.left], ["theirs", panes.right]] as const) {
      view.dispatch({ effects: setSideDecorations.of(lineDecorations(view.state, sideRanges(side, view))) });
    }
  }, [chunks, layout, panes]);

  /** 放进结果或忽略一侧；可以一次处理多块（“应用所有不冲突的改动”）。 */
  const act = useCallback(
    (targets: ActTarget[]) => {
      const view = panesRef.current?.result;
      if (!view || targets.length === 0) return;
      const states = view.state.field(chunkField);
      const changes: Array<{ from: number; to: number; insert: string }> = [];
      const updates: Array<{ index: number; state: ChunkState }> = [];
      for (const { index, side, op } of targets) {
        const state = states[index];
        const chunk = layout.chunks[index];
        const next = op === "apply" ? applySide(state, side) : ignoreSide(state, side);
        updates.push({ index, state: next });
        if (!state.edited) {
          changes.push({ from: state.from, to: state.to, insert: chunkContent(texts, chunk, next) });
        } else if (op === "apply") {
          // 手动改过的块：这一侧插在块首（左边）或块尾（右边），不覆盖手动内容。
          let insert = sideText(texts, chunk, side);
          const at = side === "ours" ? state.from : state.to;
          if (side === "ours" && insert && !insert.endsWith("\n") && at < view.state.doc.length) insert += "\n";
          changes.push({ from: at, to: at, insert });
        }
      }
      const changeSet = view.state.changes(changes);
      const effects = updates.map(({ index, state }) => {
        const old = states[index];
        const from = changeSet.mapPos(old.from, -1);
        return setChunk.of({ index, state: { ...state, from, to: Math.max(from, changeSet.mapPos(old.to, 1)) } });
      });
      view.dispatch({ changes: changeSet, effects, annotations: programmatic.of(true) });
      view.focus();
    },
    [layout, texts],
  );

  const applyNonConflicting = useCallback(() => {
    act(
      chunks.flatMap((state, index): ActTarget[] =>
        state.kind === "conflict" || isResolved(state)
          ? []
          : [{ index, side: state.kind === "theirs" ? "theirs" : "ours", op: "apply" }],
      ),
    );
  }, [act, chunks]);

  /** 上一处、下一处未解决的冲突（⇧F7、F7），到头后从另一头接着找。 */
  const jump = useCallback((direction: 1 | -1) => {
    const view = panesRef.current?.result;
    if (!view) return;
    const head = view.state.selection.main.head;
    const targets = view.state
      .field(chunkField)
      .filter((state) => state.kind === "conflict" && !isResolved(state))
      .map((state) => state.from);
    if (targets.length === 0) return;
    const next =
      direction > 0
        ? (targets.find((from) => from > head) ?? targets[0])
        : ([...targets].reverse().find((from) => from < head) ?? targets[targets.length - 1]);
    view.dispatch({ selection: { anchor: next }, effects: EditorView.scrollIntoView(next, { y: "center" }) });
    view.focus();
  }, []);

  const write = useCallback(
    async (request: MergeWriteRequest) => {
      setBusy(true);
      setError(null);
      try {
        await onWrite(request);
        if (mounted.current) onClose();
      } catch (caught) {
        if (!mounted.current) return;
        const appError = toAppError(caught);
        setError({ message: appError.message, stale: appError.kind === "rejected" });
      } finally {
        if (mounted.current) setBusy(false);
      }
    },
    [mounted, onClose, onWrite],
  );

  const apply = useCallback(() => {
    const view = panesRef.current?.result;
    if (!view) return;
    const text = view.state.doc.toString();
    void write({ kind: "apply", worktreeId: layout.worktreeId, path: layout.path, result: text, fingerprint: layout.fingerprint });
  }, [layout, write]);

  const requestApply = useCallback(() => {
    const view = panesRef.current?.result;
    if (!view) return;
    if (unresolvedConflicts(view.state.field(chunkField)) > 0 || hasConflictMarkers(view.state.doc.toString())) {
      setConfirm("apply");
      return;
    }
    apply();
  }, [apply]);

  const acceptSide = useCallback(
    (side: MergeSide) =>
      void write({
        kind: "acceptSide",
        worktreeId: layout.worktreeId,
        targets: [{ path: layout.path, fingerprint: layout.fingerprint }],
        side,
      }),
    [layout, write],
  );

  const onKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if (event.key !== "F7") return;
      event.preventDefault();
      event.stopPropagation();
      jump(event.shiftKey ? -1 : 1);
    },
    [jump],
  );

  const ranges = useMemo(
    () => (panes ? resultLines(panes.result, chunks) : []),
    // tick 只用来在结果栏文本变化后重新计算行号。
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [panes, chunks, tick],
  );

  return (
    <div className="merge-view" data-merge-view onKeyDown={onKeyDown}>
      <div className="merge-toolbar">
        <button
          type="button"
          className="icon-action"
          title={messages.merge.previousConflict}
          aria-label={messages.merge.previousConflict}
          disabled={conflicts === 0}
          onClick={() => jump(-1)}
        >
          <Codicon name="arrow-up" size={16} />
        </button>
        <button
          type="button"
          className="icon-action"
          title={messages.merge.nextConflict}
          aria-label={messages.merge.nextConflict}
          disabled={conflicts === 0}
          onClick={() => jump(1)}
        >
          <Codicon name="arrow-down" size={16} />
        </button>
        <span className="merge-toolbar-separator" />
        <button type="button" className="merge-button" disabled={busy || !hasPendingNonConflicts(chunks)} onClick={applyNonConflicting}>
          {messages.merge.applyAllClean}
        </button>
        <span className="flex-1" />
        <span className={conflicts > 0 ? "merge-count merge-count-open" : "merge-count"} data-merge-conflicts>{messages.merge.conflictCount(conflicts)}</span>
      </div>
      {error ? (
        <div role="alert" className="editor-banner shrink-0" data-tone="error">
          <Codicon name="error" size={16} />
          <div className="min-w-0 flex-1">{error.message}</div>
          {error.stale ? (
            <button type="button" className="banner-link" onClick={onReload}>
              {messages.merge.reload}
            </button>
          ) : null}
        </div>
      ) : null}
      <div className="merge-columns merge-headers">
        <div className="merge-header" title={sideTitle(layout, "ours")}>
          {sideTitle(layout, "ours")}
        </div>
        <div />
        <div className="merge-header">{messages.merge.result}</div>
        <div />
        <div className="merge-header" title={sideTitle(layout, "theirs")}>
          {sideTitle(layout, "theirs")}
        </div>
      </div>
      <div className="merge-columns merge-panes">
        <div ref={leftHost} className="merge-pane" data-merge-pane="ours" />
        <MergeGutter side="ours" layout={layout} chunks={chunks} resultLines={ranges} panes={panes} tick={tick} disabled={busy} onAct={act} />
        <div ref={resultHost} className="merge-pane merge-pane-result" data-merge-pane="result" />
        <MergeGutter side="theirs" layout={layout} chunks={chunks} resultLines={ranges} panes={panes} tick={tick} disabled={busy} onAct={act} />
        <div ref={rightHost} className="merge-pane" data-merge-pane="theirs" />
      </div>
      <div className="merge-footer">
        <button type="button" className="merge-button" disabled={busy} onClick={() => (touched ? setConfirm("ours") : acceptSide("ours"))}>
          {messages.merge.acceptLeft}
        </button>
        <button type="button" className="merge-button" disabled={busy} onClick={() => (touched ? setConfirm("theirs") : acceptSide("theirs"))}>
          {messages.merge.acceptRight}
        </button>
        <span className="min-w-0 flex-1 truncate px-2 text-descriptionforeground">{messages.merge.applyHint}</span>
        <button type="button" className="merge-button" disabled={busy} onClick={() => (touched ? setConfirm("cancel") : onClose())}>
          {messages.merge.cancel}
        </button>
        <button type="button" className="merge-button merge-button-primary" disabled={busy} onClick={requestApply}>
          {messages.merge.apply}
        </button>
      </div>
      {confirm ? (
        <MergeConfirm
          kind={confirm}
          conflicts={conflicts}
          onCancel={() => setConfirm(null)}
          onConfirm={() => {
            const kind = confirm;
            setConfirm(null);
            if (kind === "apply") apply();
            else if (kind === "cancel") onClose();
            else acceptSide(kind);
          }}
        />
      ) : null}
    </div>
  );
}

function sideTitle(layout: MergeLayout, side: MergeSideName): string {
  const label = side === "ours" ? layout.leftLabel : layout.rightLabel;
  return messages.merge.sideTitle(side, label.role, label.name);
}

// ---------- 窄栏 ----------

function MergeGutter({
  side,
  layout,
  chunks,
  resultLines: lines,
  panes,
  tick,
  disabled,
  onAct,
}: {
  side: MergeSideName;
  layout: MergeLayout;
  chunks: ChunkState[];
  resultLines: Array<[number, number]>;
  panes: Panes | null;
  tick: number;
  disabled: boolean;
  onAct: (targets: ActTarget[]) => void;
}) {
  const boxRef = useRef<HTMLDivElement>(null);
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const [buttons, setButtons] = useState<Array<{ index: number; y: number; conflict: boolean }>>([]);

  useLayoutEffect(() => {
    const prepared = prepareCanvas(canvasRef.current, boxRef.current);
    if (!prepared || !panes || lines.length !== chunks.length) return;
    const { context, width, height, styles } = prepared;
    const sideView = side === "ours" ? panes.left : panes.right;
    const color = (name: string, fallback: string) => styles.getPropertyValue(name).trim() || fallback;
    const fills = {
      conflict: color("--merge-band-conflict", "rgb(200 60 60 / 35%)"),
      change: color("--merge-band-change", "rgb(60 140 60 / 35%)"),
      done: color("--merge-band-done", "rgb(128 128 128 / 18%)"),
    };
    const next: Array<{ index: number; y: number; conflict: boolean }> = [];
    chunks.forEach((state, index) => {
      const sideState = state[side];
      if (sideState === "none") return;
      const chunk: MergeChunk = layout.chunks[index];
      const [start, end] = side === "ours" ? [chunk.oursStart, chunk.oursEnd] : [chunk.theirsStart, chunk.theirsEnd];
      const sideTop = lineY(sideView, start);
      const sideBottom = lineY(sideView, end);
      const resultTop = lineY(panes.result, lines[index][0]);
      const resultBottom = lineY(panes.result, lines[index][1]);
      const ends: BandEnds =
        side === "ours"
          ? { leftTop: sideTop, leftBottom: sideBottom, rightTop: resultTop, rightBottom: resultBottom }
          : { leftTop: resultTop, leftBottom: resultBottom, rightTop: sideTop, rightBottom: sideBottom };
      if (!bandVisible(ends, height)) return;
      const fill = sideState !== "pending" ? fills.done : state.kind === "conflict" ? fills.conflict : fills.change;
      drawBand(context, 0, width, ends, fill);
      if (sideState === "pending") {
        next.push({ index, y: Math.max(0, Math.min(height - ROW_HEIGHT, sideTop)), conflict: state.kind === "conflict" });
      }
    });
    setButtons((current) =>
      current.length === next.length &&
      current.every((item, i) => item.index === next[i].index && item.y === next[i].y && item.conflict === next[i].conflict)
        ? current
        : next,
    );
  }, [chunks, layout, lines, panes, side, tick]);

  const take = side === "ours" ? messages.merge.takeLeft : messages.merge.takeRight;
  const ignore = (index: number) => (
    <button
      type="button"
      className="merge-gutter-button merge-gutter-ignore"
      title={messages.merge.ignoreSide}
      aria-label={messages.merge.ignoreSide}
      disabled={disabled}
      onClick={() => onAct([{ index, side, op: "ignore" }])}
    >
      ✕
    </button>
  );
  return (
    <div ref={boxRef} className={`merge-gutter merge-gutter-${side}`}>
      <canvas ref={canvasRef} className="merge-gutter-canvas" aria-hidden />
      {buttons.map(({ index, y, conflict }) => (
        <div key={index} className="merge-gutter-actions" style={{ top: y }}>
          {side === "theirs" ? ignore(index) : null}
          <button
            type="button"
            className={conflict ? "merge-gutter-button merge-gutter-take merge-gutter-conflict" : "merge-gutter-button merge-gutter-take"}
            title={take}
            aria-label={take}
            disabled={disabled}
            onClick={() => onAct([{ index, side, op: "apply" }])}
          >
            {side === "ours" ? "»" : "«"}
          </button>
          {side === "ours" ? ignore(index) : null}
        </div>
      ))}
    </div>
  );
}

// ---------- 确认与只能接受一侧 ----------

function MergeConfirm({
  kind,
  conflicts,
  onCancel,
  onConfirm,
}: {
  kind: "apply" | "cancel" | MergeSide;
  conflicts: number;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  const text = messages.merge.confirm(kind, conflicts);
  return (
    <Modal title={text.title} onClose={onCancel} width={460}>
      <p className="text-[12.5px] leading-relaxed">{text.body}</p>
      <div className="mt-5 flex justify-end gap-2">
        <button type="button" autoFocus className="rounded-[4px] border border-[color:color-mix(in_srgb,var(--icon-foreground)_28%,transparent)] px-3 py-1 hover:bg-toolbar-hoverbackground" onClick={onCancel}>
          {text.stay}
        </button>
        <button type="button" className="rounded-[4px] bg-primary px-3 py-1 text-primary-foreground" onClick={onConfirm}>
          {text.go}
        </button>
      </div>
    </Modal>
  );
}

function AcceptOnly({
  layout,
  onWrite,
  onClose,
  onReload,
}: {
  layout: MergeLayout;
  onWrite: (request: MergeWriteRequest) => Promise<void>;
  onClose: () => void;
  onReload: () => void;
}) {
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<{ message: string; stale: boolean } | null>(null);
  const mounted = useMountedRef();
  const accept = async (side: MergeSide) => {
    setBusy(true);
    setError(null);
    try {
      await onWrite({
        kind: "acceptSide",
        worktreeId: layout.worktreeId,
        targets: [{ path: layout.path, fingerprint: layout.fingerprint }],
        side,
      });
      if (mounted.current) onClose();
    } catch (caught) {
      if (!mounted.current) return;
      const appError = toAppError(caught);
      setError({ message: appError.message, stale: appError.kind === "rejected" });
    } finally {
      if (mounted.current) setBusy(false);
    }
  };
  return (
    <div className="merge-message">
      <p className="font-medium">{messages.merge.cannotMerge}</p>
      <p className="text-descriptionforeground">{layout.unavailable ? messages.merge.unavailable[layout.unavailable] : null}</p>
      {error ? (
        <p role="alert" className="text-destructive">
          {error.message}
          {error.stale ? (
            <button type="button" className="banner-link ml-2" onClick={onReload}>
              {messages.merge.reload}
            </button>
          ) : null}
        </p>
      ) : null}
      <div className="flex gap-2">
        <button type="button" className="merge-button" disabled={busy} onClick={() => void accept("ours")}>
          {messages.merge.acceptSideState("ours", layout.oursState)}
        </button>
        <button type="button" className="merge-button" disabled={busy} onClick={() => void accept("theirs")}>
          {messages.merge.acceptSideState("theirs", layout.theirsState)}
        </button>
      </div>
    </div>
  );
}
