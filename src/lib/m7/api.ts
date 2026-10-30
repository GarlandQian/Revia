/**
 * M7 命令与事件的薄封装（主题、编辑、终端、克隆、GitHub、诊断、更新）。
 *
 * 与 `src/lib/ipc.ts` 同一原则：前端只调用固定、带类型的操作，不拼 Git 参数、
 * 不接触文件系统与网络。参数与返回值优先用 ts-rs 生成的绑定；少数只作为
 * 入参的结构体在 Rust 侧没有导出绑定，这里按 camelCase 形状补齐。
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { CloneFinished } from "@/types/bindings/CloneFinished";
import type { CloneProgress } from "@/types/bindings/CloneProgress";
import type { DiagnosticsExport } from "@/types/bindings/DiagnosticsExport";
import type { DiagnosticsPreview } from "@/types/bindings/DiagnosticsPreview";
import type { DevicePoll } from "@/types/bindings/DevicePoll";
import type { DeviceStart } from "@/types/bindings/DeviceStart";
import type { EditDraft } from "@/types/bindings/EditDraft";
import type { EditSaved } from "@/types/bindings/EditSaved";
import type { EditableFile } from "@/types/bindings/EditableFile";
import type { GithubAccount } from "@/types/bindings/GithubAccount";
import type { GithubStatus } from "@/types/bindings/GithubStatus";
import type { ImportedTheme } from "@/types/bindings/ImportedTheme";
import type { PrMergeOutcome } from "@/types/bindings/PrMergeOutcome";
import type { MergeReceipt } from "@/types/bindings/MergeReceipt";
import type { PrCheckoutOutcome } from "@/types/bindings/PrCheckoutOutcome";
import type { PrCreated } from "@/types/bindings/PrCreated";
import type { ProtectionInfo } from "@/types/bindings/ProtectionInfo";
import type { PullRequestComment } from "@/types/bindings/PullRequestComment";
import type { PullRequestDetail } from "@/types/bindings/PullRequestDetail";
import type { PullRequestFile } from "@/types/bindings/PullRequestFile";
import type { PullRequestFilter } from "@/types/bindings/PullRequestFilter";
import type { PullRequestList } from "@/types/bindings/PullRequestList";
import type { RemoteInfo } from "@/types/bindings/RemoteInfo";
import type { ReviewDraft } from "@/types/bindings/ReviewDraft";
import type { ReviewEvent } from "@/types/bindings/ReviewEvent";
import type { ReviewReceipt } from "@/types/bindings/ReviewReceipt";
import type { ReviewCommentInput } from "@/types/bindings/ReviewCommentInput";
import type { ThemeKind } from "@/types/bindings/ThemeKind";
import type { UpdateProgress } from "@/types/bindings/UpdateProgress";
import type { AppUpdateStatus } from "@/types/bindings/AppUpdateStatus";
import type { TerminalExit } from "@/types/bindings/TerminalExit";
import type { TerminalOutput } from "@/types/bindings/TerminalOutput";
import type { TerminalSessionInfo } from "@/types/bindings/TerminalSessionInfo";
import type { TerminalTabRecord } from "@/types/bindings/TerminalTabRecord";
import type { MergeMethod } from "@/types/bindings/MergeMethod";
import type { CheckoutStrategy } from "@/types/bindings/CheckoutStrategy";
import type { SettingsPayload } from "@/lib/ipc";
import type { RepoEntry } from "@/types/bindings/RepoEntry";

/** 编辑保存请求（Rust 侧只作为入参，没有导出绑定）。 */
export interface EditSavePayload {
  worktreeId: string;
  path: string;
  baseSha256: string;
  text: string;
}

export interface CloneStartPayload {
  url: string;
  target: string;
  branch: string | null;
  depth: number | null;
  recurseSubmodules: boolean;
}

export interface TerminalOpenPayload {
  worktreeId: string;
  cwd: string | null;
  rows: number;
  cols: number;
}

export interface DiagnosticsExportPayload {
  destination: string;
  worktreeId: string | null;
}

export interface PrCheckoutPayload {
  worktreeId: string;
  remote: string;
  number: number;
  expectedHeadSha: string;
  strategy: CheckoutStrategy;
  localBranch: string | null;
}

export interface PrCreatePayload {
  worktreeId: string;
  remote: string;
  head: string;
  base: string;
  title: string;
  body: string;
  draft: boolean;
}

export interface ReviewSubmitPayload {
  worktreeId: string;
  number: number;
  event: ReviewEvent;
  body: string;
  comments: ReviewCommentInput[];
  expectedHeadSha: string;
}

export interface MergePayload {
  worktreeId: string;
  number: number;
  method: MergeMethod;
  expectedHeadSha: string;
  commitTitle: string | null;
  commitMessage: string | null;
  confirmed: boolean;
}

export interface PrRefPayload {
  worktreeId: string;
  remote: string | null;
  number: number;
}

export const m7 = {
  // 颜色主题
  listImportedThemes: () => invoke<ImportedTheme[]>("m7_list_imported_themes"),
  importTheme: (path: string, kind: ThemeKind | null, label: string | null) =>
    invoke<ImportedTheme>("m7_import_theme", { request: { path, kind, label } }),
  removeImportedTheme: (id: string) => invoke<SettingsPayload>("m7_remove_imported_theme", { id }),

  // 工作区编辑
  openEditable: (worktreeId: string, path: string) =>
    invoke<EditableFile>("m7_open_editable", { request: { worktreeId, path } }),
  saveEditable: (request: EditSavePayload) => invoke<EditSaved>("m7_save_editable", { request }),
  saveEditDraft: (draft: EditDraft) => invoke<EditDraft>("m7_save_edit_draft", { draft }),
  loadEditDraft: (worktreeId: string, path: string) =>
    invoke<EditDraft | null>("m7_load_edit_draft", { request: { worktreeId, path } }),
  clearEditDraft: (worktreeId: string, path: string) =>
    invoke<void>("m7_clear_edit_draft", { request: { worktreeId, path } }),
  listEditDrafts: (worktreeId: string) => invoke<EditDraft[]>("m7_list_edit_drafts", { worktreeId }),

  // 内置终端
  terminalOpen: (request: TerminalOpenPayload) =>
    invoke<TerminalSessionInfo>("m7_terminal_open", { request }),
  terminalList: () => invoke<TerminalSessionInfo[]>("m7_terminal_list"),
  terminalWrite: (sessionId: number, data: string) =>
    invoke<void>("m7_terminal_write", { sessionId, data }),
  terminalResize: (sessionId: number, rows: number, cols: number) =>
    invoke<void>("m7_terminal_resize", { sessionId, rows, cols }),
  terminalSnapshot: (sessionId: number) =>
    invoke<string>("m7_terminal_snapshot", { sessionId }),
  terminalClose: (sessionId: number) => invoke<void>("m7_terminal_close", { sessionId }),
  terminalCloseWorktree: (worktreeId: string) =>
    invoke<number[]>("m7_terminal_close_worktree", { worktreeId }),
  terminalTabs: () => invoke<TerminalTabRecord[]>("m7_terminal_tabs"),
  terminalSaveTabs: (tabs: TerminalTabRecord[]) =>
    invoke<void>("m7_terminal_save_tabs", { tabs }),

  // 克隆
  cloneStart: (request: CloneStartPayload) => invoke<number>("m7_clone_start", { request }),
  cloneCancel: (jobId: number) => invoke<void>("m7_clone_cancel", { jobId }),
  cloneActive: () => invoke<number[]>("m7_clone_active"),
  cloneRegister: (path: string) => invoke<RepoEntry>("m7_clone_register", { path }),

  // GitHub
  githubStatus: (worktreeId: string | null) =>
    invoke<GithubStatus>("m7_github_status", { worktreeId }),
  githubDeviceStart: () => invoke<DeviceStart>("m7_github_device_start"),
  githubDevicePoll: (flowId: string) => invoke<DevicePoll>("m7_github_device_poll", { flowId }),
  githubLoginToken: (token: string) => invoke<GithubAccount>("m7_github_login_token", { token }),
  githubLogout: (login: string) => invoke<void>("m7_github_logout", { login }),
  githubBindRepo: (worktreeId: string, login: string) =>
    invoke<void>("m7_github_bind_repo", { worktreeId, login }),
  githubRemotes: (worktreeId: string) => invoke<RemoteInfo[]>("m7_github_remotes", { worktreeId }),
  prList: (worktreeId: string, remote: string | null, filter: PullRequestFilter, page: number) =>
    invoke<PullRequestList>("m7_github_pr_list", { request: { worktreeId, remote, filter, page } }),
  prDetail: (request: PrRefPayload) => invoke<PullRequestDetail>("m7_github_pr_detail", { request }),
  prFiles: (request: PrRefPayload) => invoke<PullRequestFile[]>("m7_github_pr_files", { request }),
  prDiff: (request: PrRefPayload) => invoke<string>("m7_github_pr_diff", { request }),
  prComments: (request: PrRefPayload) =>
    invoke<PullRequestComment[]>("m7_github_pr_comments", { request }),
  prCommentCreate: (request: PrRefPayload & { body: string }) =>
    invoke<void>("m7_github_pr_comment_create", { request }),
  reviewDrafts: (request: PrRefPayload) =>
    invoke<ReviewDraft[]>("m7_github_review_drafts", { request }),
  reviewDraftSave: (draft: ReviewDraft) =>
    invoke<ReviewDraft>("m7_github_review_draft_save", { draft }),
  reviewDraftDelete: (id: string) => invoke<void>("m7_github_review_draft_delete", { id }),
  reviewSubmit: (request: ReviewSubmitPayload) =>
    invoke<ReviewReceipt>("m7_github_review_submit", { request }),
  prCheckout: (request: PrCheckoutPayload) =>
    invoke<PrCheckoutOutcome>("m7_github_pr_checkout", { request }),
  prCreate: (request: PrCreatePayload) => invoke<PrCreated>("m7_github_pr_create", { request }),
  prMerge: (request: MergePayload) => invoke<PrMergeOutcome>("m7_github_pr_merge", { request }),
  githubReceipts: (worktreeId: string) =>
    invoke<MergeReceipt[]>("m7_github_receipts", { worktreeId }),
  protectionRefresh: (worktreeId: string, remote: string | null, branch: string) =>
    invoke<ProtectionInfo>("m7_github_protection_refresh", { request: { worktreeId, remote }, branch }),

  // 诊断
  previewDiagnostics: (request: DiagnosticsExportPayload) =>
    invoke<DiagnosticsPreview>("m7_preview_diagnostics", { request }),
  exportDiagnostics: (request: DiagnosticsExportPayload) =>
    invoke<DiagnosticsExport>("m7_export_diagnostics", { request }),

  // 应用更新
  updateStatus: () => invoke<AppUpdateStatus>("m7_update_status"),
  updateCheck: (force: boolean) => invoke<AppUpdateStatus>("m7_update_check", { force }),
  updateInstall: () => invoke<AppUpdateStatus>("m7_update_install"),
  updateRestart: () => invoke<void>("m7_update_restart"),
};

/** 订阅 M7 的后端事件；返回取消订阅函数。 */
export function onM7Events(handlers: {
  onTerminalOutput?: (event: TerminalOutput) => void;
  onTerminalExit?: (event: TerminalExit) => void;
  onCloneProgress?: (event: CloneProgress) => void;
  onCloneFinished?: (event: CloneFinished) => void;
  onUpdateProgress?: (event: UpdateProgress) => void;
  onUpdateAvailable?: (event: { version: string }) => void;
}): () => void {
  const pending: Promise<UnlistenFn>[] = [];
  const add = <T,>(name: string, handler: ((payload: T) => void) | undefined) => {
    if (!handler) return;
    pending.push(listen<T>(name, (event) => handler(event.payload)));
  };
  add<TerminalOutput>("revia://terminal-output", handlers.onTerminalOutput);
  add<TerminalExit>("revia://terminal-exit", handlers.onTerminalExit);
  add<CloneProgress>("revia://clone-progress", handlers.onCloneProgress);
  add<CloneFinished>("revia://clone-finished", handlers.onCloneFinished);
  add<UpdateProgress>("revia://update-progress", handlers.onUpdateProgress);
  add<{ version: string }>("revia://update-available", handlers.onUpdateAvailable);
  return () => {
    for (const promise of pending) void promise.then((unlisten) => unlisten());
  };
}
