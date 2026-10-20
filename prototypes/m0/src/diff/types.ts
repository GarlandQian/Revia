/**
 * 与 Rust 层对齐的数据类型。
 *
 * 所有 Git 输出都由 Rust 解析（ADR-0004），前端只接收这里定义的结构，
 * 不重新计算差异，也不拼接任何 Git 参数。
 */

export type LineKind = "context" | "deletion" | "addition" | "noEol";

export interface DiffLine {
  kind: LineKind;
  oldLine: number | null;
  newLine: number | null;
  /** 去掉前缀之后的内容。 */
  text: string;
}

export interface Hunk {
  section: string;
  oldStart: number;
  oldCount: number;
  newStart: number;
  newCount: number;
  header: string;
  lines: DiffLine[];
}

export type FileStatus = "added" | "deleted" | "modified" | "renamed";

export interface FileChange {
  path: string;
  oldPath: string | null;
  status: FileStatus;
  binary: boolean;
  hunks: Hunk[];
}

export interface EnvCaptureInfo {
  worktree: string;
  shell: string;
  elapsedMs: number;
  timedOut: boolean;
  usedFallback: boolean;
  varCount: number;
  pathEntries: string[];
  diagnostics: string | null;
}

export interface DiffResponse {
  worktree: string;
  staged: boolean;
  /** 用户看到的这份 diff 的指纹；写入操作必须携带（ADR-0005）。 */
  fingerprint: string;
  files: FileChange[];
  parseMs: number;
  gitPath: string;
  gitVersion: string;
  env: EnvCaptureInfo;
}

export interface FileVersionResponse {
  head: string | null;
  working: string | null;
  staged: string | null;
  truncated: boolean;
}

export interface HighlightToken {
  text: string;
  color: string;
  bold: boolean;
  italic: boolean;
  underline: boolean;
}

export type HighlightLine = HighlightToken[] | null;

export interface HighlightResult {
  lines: HighlightLine[];
  syntax: string;
  theme: string;
  foreground: string;
  background: string;
  elapsedMs: number;
  lineCount: number;
}

export interface WorktreeInfo {
  worktree: string;
  gitPath: string;
  gitVersion: string;
  env: EnvCaptureInfo;
}

/** 一个版本（旧 / 新）的高亮结果，按 1 基行号索引。 */
export interface HighlightedDocument {
  old: Map<number, HighlightToken[]>;
  new: Map<number, HighlightToken[]>;
  engine: "shiki" | "syntect";
  elapsedMs: number;
  engineMs: number | null;
  syntax: string | null;
  theme: string | null;
  /** syntect 分块高亮：可见区域之外是否已经补齐。 */
  complete: boolean;
  /** 已经处理过的行数（两个版本合计），用于显示进度。 */
  highlightedLines: number;
  /** 需要处理的总行数。 */
  totalLines: number;
}

export type ViewMode = "split" | "unified";
export type HighlightMode = "none" | "shiki" | "syntect";

export const lineKindLabel: Record<LineKind, string> = {
  context: " ",
  deletion: "-",
  addition: "+",
  noEol: "\\",
};

export const statusLabel: Record<FileStatus, string> = {
  added: "A",
  deleted: "D",
  modified: "M",
  renamed: "R",
};
