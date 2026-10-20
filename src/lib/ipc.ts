/**
 * Rust 命令的薄封装。前端只能调用这些固定、带类型的操作，
 * 不拼接 Git 参数，也不传任意目录路径（路径用 Rust 层登记的 ID）。
 *
 * 参数与返回类型由 ts-rs 从 Rust 侧生成（`pnpm bindings`）。
 */

import { invoke } from "@tauri-apps/api/core";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesKind } from "@/types/bindings/ChangesKind";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";
import type { FileVersion } from "@/types/bindings/FileVersion";
import type { RepoEntry } from "@/types/bindings/RepoEntry";
import type { RowSlice } from "@/types/bindings/RowSlice";
import type { ViewMode } from "@/types/bindings/ViewMode";
import type { WorktreeSessionInfo } from "@/types/bindings/WorktreeSessionInfo";

/** 与 Rust 侧 `HighlightResult` 对应。 */
export interface HighlightTokenPayload {
  text: string;
  /** 语法类别索引；名称表由 `syntaxCategories()` 返回。 */
  category: number;
}

export type HighlightLine = HighlightTokenPayload[] | null;

export interface HighlightRangeResult {
  fromLine: number;
  lines: HighlightLine[];
  kind: "highlighted" | "plainText";
  needContent: boolean;
  elapsedMs: number;
  parsedLines: number;
  totalLines: number;
}

/** 与 Rust 侧 `Settings` 对应。 */
export interface SettingsPayload {
  repos: string[];
  lastWorktreeId: string | null;
  diffFontSize: number;
}

export interface CommandRecordPayload {
  sequence: number;
  program: string;
  args: string[];
  directory: string;
  exitCode: number;
  elapsedMs: number;
  timedOut: boolean;
  stdoutBytes: number;
  stderrExcerpt: string;
}

export const ipc = {
  listRepos: () => invoke<RepoEntry[]>("list_repos"),
  listSettings: () => invoke<SettingsPayload>("list_settings"),
  addRepo: (path: string) => invoke<RepoEntry>("add_repo", { request: { path } }),
  selectRepo: (repoId: string, worktreeId?: string) =>
    invoke<RepoEntry>("select_repo", {
      request: { repoId, worktreeId: worktreeId ?? null },
    }),
  setDiffFontSize: (size: number) => invoke<SettingsPayload>("set_diff_font_size", { size }),
  openWorktree: (worktreeId: string) =>
    invoke<WorktreeSessionInfo>("open_worktree", { request: { worktreeId } }),
  listChanges: (worktreeId: string) =>
    invoke<ChangesSnapshot>("list_changes", { request: { worktreeId } }),
  getRows: (request: {
    worktreeId: string;
    path: string;
    changes: ChangesKind;
    view: ViewMode;
    from: number;
    to: number;
  }) => invoke<RowSlice>("get_rows", { request }),
  highlightRange: (request: {
    worktreeId: string;
    path: string;
    changes: ChangesKind;
    version: FileVersion;
    fromLine: number;
    toLine: number;
  }) => invoke<HighlightRangeResult>("highlight_range", { request }),
  rowIndexForLine: (request: {
    worktreeId: string;
    path: string;
    changes: ChangesKind;
    view: ViewMode;
    line: number;
    version: FileVersion;
  }) => invoke<number>("row_index_for_line", { request }),
  syntaxCategories: () => invoke<string[]>("syntax_categories"),
  commandLog: () => invoke<CommandRecordPayload[]>("command_log_snapshot"),
  measureConfig: () =>
    invoke<{
      logging: boolean;
      autoRun: boolean;
      only: string[] | null;
      outputPath: string | null;
      repoPath: string | null;
      targetPath: string | null;
      smoke: boolean;
    }>("measure_config"),
  writeMeasurement: (payload: unknown) => invoke<string | null>("write_measurement", { payload }),
  debugLog: (message: string) => invoke<void>("debug_log", { message }),
  changeEntry: (request: {
    worktreeId: string;
    path: string;
    changes: ChangesKind;
    view: ViewMode;
    from: number;
    to: number;
  }) => invoke<ChangeEntry>("change_entry", { request }),
};
