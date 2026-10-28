/**
 * 合并页签的块状态（Q143，设计见 docs/milestones/m4/merge-design.md）。
 *
 * 块的位置与种类都来自 Rust（由 Git 的 diff 与 `git merge-file` 生成）；
 * 这里只记录用户对每块做了什么（放进结果、忽略、手动修改），并据此算出
 * 这一块在结果里应有的文本。同一处冲突两侧都放进结果时，左边在前。
 */

import type { MergeChunk } from "@/types/bindings/MergeChunk";
import type { MergeChunkKind } from "@/types/bindings/MergeChunkKind";
import type { SyncSegment } from "@/features/diff/syncScroll";

/** 一侧在这一块里的状态：没有改动、待处理、已放进结果、已忽略。 */
export type SideState = "none" | "pending" | "applied" | "ignored";

export type MergeSideName = "ours" | "theirs";

export interface ChunkState {
  kind: MergeChunkKind;
  ours: SideState;
  theirs: SideState;
  /** 用户在结果栏里改过这一块。 */
  edited: boolean;
  /** 这一块在结果文本里的字符区间（左闭右开）。 */
  from: number;
  to: number;
}

/** 三个版本的文本与每行起点（多一个元素是文末）。 */
export interface MergeTexts {
  left: string;
  base: string;
  right: string;
  leftStarts: number[];
  baseStarts: number[];
  rightStarts: number[];
}

/** 每行起点（字符下标），最后一个元素是文本长度。 */
export function lineStarts(text: string): number[] {
  const starts = [0];
  let index = text.indexOf("\n");
  while (index !== -1) {
    starts.push(index + 1);
    index = text.indexOf("\n", index + 1);
  }
  if (starts[starts.length - 1] !== text.length) starts.push(text.length);
  return starts;
}

/** 行区间 `[start, end)` 对应的文本（含行尾）。 */
export function sliceLines(text: string, starts: number[], start: number, end: number): string {
  const last = starts.length - 1;
  const from = starts[Math.min(start, last)];
  const to = starts[Math.min(Math.max(start, end), last)];
  return text.slice(from, to);
}

export function mergeTexts(left: string, base: string, right: string): MergeTexts {
  return { left, base, right, leftStarts: lineStarts(left), baseStarts: lineStarts(base), rightStarts: lineStarts(right) };
}

function oursText(texts: MergeTexts, chunk: MergeChunk): string {
  return sliceLines(texts.left, texts.leftStarts, chunk.oursStart, chunk.oursEnd);
}

function theirsText(texts: MergeTexts, chunk: MergeChunk): string {
  return sliceLines(texts.right, texts.rightStarts, chunk.theirsStart, chunk.theirsEnd);
}

function baseText(texts: MergeTexts, chunk: MergeChunk): string {
  return sliceLines(texts.base, texts.baseStarts, chunk.baseStart, chunk.baseEnd);
}

/** 先放左边、再放右边；左边最后一行没有换行时补一个，避免两行粘在一起。 */
function join(first: string, second: string): string {
  if (first && second && !first.endsWith("\n")) return `${first}\n${second}`;
  return first + second;
}

/** 按两侧的状态算出这一块在结果里的文本（用户没手动改过时）。 */
export function chunkContent(texts: MergeTexts, chunk: MergeChunk, state: ChunkState): string {
  const ours = state.ours === "applied";
  const theirs = state.theirs === "applied";
  switch (chunk.kind) {
    case "both":
      return ours || theirs ? oursText(texts, chunk) : baseText(texts, chunk);
    case "ours":
      return ours ? oursText(texts, chunk) : baseText(texts, chunk);
    case "theirs":
      return theirs ? theirsText(texts, chunk) : baseText(texts, chunk);
    case "conflict":
      if (!ours && !theirs) return baseText(texts, chunk);
      return join(ours ? oursText(texts, chunk) : "", theirs ? theirsText(texts, chunk) : "");
  }
}

/** 某一侧这一块的文本（手动改过的块再放进结果时插在块首或块尾）。 */
export function sideText(texts: MergeTexts, chunk: MergeChunk, side: MergeSideName): string {
  return side === "ours" ? oursText(texts, chunk) : theirsText(texts, chunk);
}

/** 初始状态：Rust 已按“合并时自动应用无冲突改动”拼好初始结果。 */
export function initialStates(chunks: MergeChunk[], resultText: string): ChunkState[] {
  const starts = lineStarts(resultText);
  const at = (line: number) => starts[Math.min(line, starts.length - 1)];
  return chunks.map((chunk) => {
    const hasOurs = chunk.kind !== "theirs";
    const hasTheirs = chunk.kind !== "ours";
    return {
      kind: chunk.kind,
      ours: hasOurs ? (chunk.oursApplied ? "applied" : "pending") : "none",
      theirs: hasTheirs ? (chunk.theirsApplied ? "applied" : "pending") : "none",
      edited: false,
      from: at(chunk.resultStart),
      to: at(chunk.resultEnd),
    };
  });
}

/** 两侧都处理过（放进结果或忽略），或者用户手动改过。 */
export function isResolved(state: ChunkState): boolean {
  return state.edited || (state.ours !== "pending" && state.theirs !== "pending");
}

export function unresolvedConflicts(states: ChunkState[]): number {
  return states.filter((state) => state.kind === "conflict" && !isResolved(state)).length;
}

/** 还有没处理的不冲突改动（“应用所有不冲突的改动”可用）。 */
export function hasPendingNonConflicts(states: ChunkState[]): boolean {
  return states.some((state) => state.kind !== "conflict" && !isResolved(state));
}

/** 放进一侧：两侧改得一样的块，放进任一侧就两侧都算处理过。 */
export function applySide(state: ChunkState, side: MergeSideName): ChunkState {
  if (state.kind === "both") return { ...state, ours: "applied", theirs: "applied" };
  return { ...state, [side]: "applied" };
}

/** 忽略一侧：结果里不放这一侧的改动。 */
export function ignoreSide(state: ChunkState, side: MergeSideName): ChunkState {
  if (state.kind === "both") return { ...state, ours: "ignored", theirs: "ignored" };
  return { ...state, [side]: "ignored" };
}

/**
 * 结果栏与一侧栏之间的同步映射段（结果行号在 left，那一侧行号在 right）。
 * `resultLines[i]` 是第 i 块在结果里的行区间。
 */
export function syncSegments(
  chunks: MergeChunk[],
  resultLines: Array<[number, number]>,
  side: MergeSideName,
): SyncSegment[] {
  return chunks.map((chunk, index) => ({
    leftStart: resultLines[index][0],
    leftEnd: resultLines[index][1],
    rightStart: side === "ours" ? chunk.oursStart : chunk.theirsStart,
    rightEnd: side === "ours" ? chunk.oursEnd : chunk.theirsEnd,
  }));
}

/** 文本里还留着冲突标记（用户手动粘进来的也算）。 */
export function hasConflictMarkers(text: string): boolean {
  return /^(<{7}|>{7})( |$)/m.test(text);
}
