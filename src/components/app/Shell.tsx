/**
 * 主窗口框架（M13、Q240–Q242）：
 * - 40px 顶部主工具栏：侧栏开关、紧凑的 仓库 / Worktree / 分支 层级切换器、
 *   右侧高频同步（更新 / 推送）、PR / 终端开关与设置按钮（有新版本时亮提示点）；
 * - 克隆入口归入仓库下拉底部、欢迎页与命令面板；
 * - 24px 底部状态栏与现代卡片式欢迎页。
 */

import { useState, type ReactNode } from "react";
import type { RepoEntry } from "@/types/bindings/RepoEntry";
import type { WorktreeEntry } from "@/types/bindings/WorktreeEntry";
import type { WorktreeStatusEntry } from "@/types/bindings/WorktreeStatusEntry";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { messages, t } from "@/lib/i18n/index";
import { formatAppError } from "@/lib/error";
import { MAIN_TOOLBAR_INSET } from "@/lib/platform";
import { AppIcon } from "@/components/ui/app-icon";
import { Popover, MenuItem, MenuSeparator } from "@/components/ui/popover";

export function TitleBar({
  repo,
  repos,
  worktrees,
  statuses,
  activeWorktreeId,
  busy,
  hasActiveWorktree,
  sidebarVisible,
  onToggleSidebar,
  onSelectRepo,
  onRemoveRepo,
  onChooseRepo,
  onCloneRepo,
  onWorktree,
  onSettings,
  onRefreshWorktrees,
  branchWidget,
  syncActions,
  prOpen,
  onTogglePr,
  terminalOpen,
  onToggleTerminal,
  updateAvailable = false,
}: {
  repo: RepoEntry | null;
  repos: RepoEntry[];
  worktrees: WorktreeEntry[];
  statuses: WorktreeStatusEntry[];
  activeWorktreeId: string | null;
  busy: boolean;
  hasActiveWorktree: boolean;
  sidebarVisible: boolean;
  onToggleSidebar: () => void;
  onSelectRepo: (repo: RepoEntry) => void;
  onRemoveRepo: (repo: RepoEntry) => void;
  onChooseRepo: () => void;
  onCloneRepo: () => void;
  onWorktree: (worktreeId: string) => void;
  onSettings: () => void;
  onRefreshWorktrees: () => void;
  branchWidget?: ReactNode;
  syncActions?: ReactNode;
  prOpen: boolean;
  onTogglePr: () => void;
  terminalOpen: boolean;
  onToggleTerminal: () => void;
  updateAvailable?: boolean;
}) {
  const [repoOpen, setRepoOpen] = useState(false);
  const [worktreeOpen, setWorktreeOpen] = useState(false);
  const active = worktrees.find((worktree) => worktree.id === activeWorktreeId);
  const statusById = new Map(statuses.map((status) => [status.id, status]));
  const activeStatus = activeWorktreeId ? statusById.get(activeWorktreeId) : undefined;
  const worktreeLabel = active?.isMain
    ? messages.changes.worktreeMain
    : active?.path.split(/[\\/]/).filter(Boolean).pop() ?? activeStatus?.branch ?? active?.branch ?? "";
  const name = repo?.name ?? messages.app.name;

  return (
    <header className="titlebar relative" data-tauri-drag-region style={{ paddingLeft: MAIN_TOOLBAR_INSET }}>
      {repo ? (
        <>
          <button
            type="button"
            className="tb-icon-btn"
            aria-pressed={sidebarVisible}
            title={messages.changes.toolWindow}
            aria-label={messages.changes.toolWindow}
            onClick={onToggleSidebar}
          >
            <AppIcon name="panel-left" size={15} />
          </button>
          <span className="tb-divider" aria-hidden />
        </>
      ) : null}

      {/* 紧凑层级切换器：仓库 / Worktree / 分支（Q240） */}
      <div className="context-chain" role="group">
        <div className="relative">
          <Popover
            open={repoOpen}
            onOpen={() => setRepoOpen(true)}
            onClose={() => setRepoOpen(false)}
            width={340}
            trigger={
              <button
                type="button"
                className="ctx-btn"
                disabled={busy}
                title={repo ? `${repo.path}\n${messages.toolbar.chooseRepo}` : messages.toolbar.chooseRepo}
              >
                <span aria-hidden className="repo-badge">
                  {name.slice(0, 1).toUpperCase()}
                </span>
                <span className="max-w-[200px] truncate">{name}</span>
                <AppIcon name="chevron-down" size={12} className="text-[var(--text-muted)]" />
              </button>
            }
          >
            <p className="px-2 py-1 text-[11px] font-medium text-[var(--text-muted)]">{messages.toolbar.repositories}</p>
            {repos.map((entry) => (
              <RepositoryListItem
                key={entry.id}
                repo={entry}
                selected={entry.id === repo?.id}
                disabled={busy}
                onSelect={() => { setRepoOpen(false); onSelectRepo(entry); }}
                onRemove={() => { setRepoOpen(false); onRemoveRepo(entry); }}
              />
            ))}
            <MenuSeparator />
            <MenuItem
              onClick={() => {
                setRepoOpen(false);
                onChooseRepo();
              }}
            >
              <span className="flex items-center gap-1.5">
                <AppIcon name="add" size={14} />
                {messages.toolbar.addRepository}
              </span>
            </MenuItem>
            <MenuItem
              onClick={() => {
                setRepoOpen(false);
                onCloneRepo();
              }}
            >
              <span className="flex items-center gap-1.5">
                <AppIcon name="repo-clone" size={14} />
                {t("toolbar.clone")}
              </span>
            </MenuItem>
          </Popover>
        </div>

        {active ? (
          <>
            <span className="ctx-slash" aria-hidden>/</span>
            <div className="relative min-w-0">
              <Popover
                open={worktreeOpen}
                onOpen={() => {
                  setWorktreeOpen(true);
                  onRefreshWorktrees();
                }}
                onClose={() => setWorktreeOpen(false)}
                width={420}
                trigger={
                  <button
                    type="button"
                    className="ctx-btn ctx-subtle min-w-0"
                    disabled={busy || worktrees.length === 0}
                    title={active.path}
                  >
                    <AppIcon name="folder" size={13} />
                    <span className="max-w-[160px] truncate">{worktreeLabel}</span>
                    <AppIcon name="chevron-down" size={12} className="text-[var(--text-muted)]" />
                  </button>
                }
              >
                <p className="px-2 py-1 text-[11px] font-medium text-[var(--text-muted)]">{messages.toolbar.worktrees}</p>
                {worktrees.map((worktree) => {
                  const status = statusById.get(worktree.id);
                  const activeRow = worktree.id === activeWorktreeId;
                  return (
                    <button
                      key={worktree.id}
                      type="button"
                      role="menuitem"
                      title={worktree.path}
                      className="flex w-full items-start gap-2 rounded-[6px] px-2 py-1.5 text-left hover:bg-[var(--surface-hover)]"
                      onClick={() => {
                        setWorktreeOpen(false);
                        onWorktree(worktree.id);
                      }}
                    >
                      <span className="mt-0.5 flex w-4 shrink-0 justify-center text-[var(--accent)]">
                        {activeRow ? <AppIcon name="check" size={13} /> : null}
                      </span>
                      <span className="min-w-0 flex-1">
                        <span className="flex items-center gap-1.5">
                          <span className="truncate font-medium">{status?.branch ?? worktree.branch ?? messages.changes.detachedHead}</span>
                          {worktree.isMain ? (
                            <span className="count-pill shrink-0">
                              {messages.changes.worktreeMain}
                            </span>
                          ) : null}
                          {status?.dirty ? (
                            <span className="size-1.5 shrink-0 rounded-full bg-[var(--git-m)]" title={messages.toolbar.dirty} />
                          ) : null}
                        </span>
                        <span className="block truncate text-[11px] text-[var(--text-muted)]">{worktree.path}</span>
                        {status ? (
                          <span className="mt-0.5 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[11px] text-[var(--text-secondary)]">
                            <span>{status.dirty ? messages.toolbar.dirty : messages.toolbar.clean}</span>
                            {status.operationLabel ? (
                              <span className="text-[var(--warn)]">
                                {messages.toolbar.operation(status.operationLabel)}
                              </span>
                            ) : null}
                            <span className="inline-flex items-center gap-1 font-mono tabular-nums">
                              <AppIcon name="arrow-up" size={11} />
                              {status.ahead}
                              <AppIcon name="arrow-down" size={11} />
                              {status.behind}
                            </span>
                            {status.error ? <span className="text-[var(--danger)]">{formatAppError(status.error)}</span> : null}
                            {!status.accessible ? <span>{messages.toolbar.inaccessible}</span> : null}
                          </span>
                        ) : null}
                      </span>
                    </button>
                  );
                })}
              </Popover>
            </div>
          </>
        ) : null}

        {branchWidget ? (
          <>
            <span className="ctx-slash" aria-hidden>/</span>
            {branchWidget}
          </>
        ) : null}
      </div>

      <span className="flex-1 self-stretch" data-tauri-drag-region />

      {/* 右侧高频同步、PR / 终端、设置 */}
      {syncActions}
      {syncActions ? <span className="tb-divider" aria-hidden /> : null}
      {repo ? (
        <>
          <button
            type="button"
            className="tb-icon-btn"
            aria-pressed={prOpen}
            disabled={busy || !hasActiveWorktree}
            title={t("toolbar.pullRequests")}
            aria-label={t("toolbar.pullRequests")}
            onClick={onTogglePr}
          >
            <AppIcon name="git-pull-request" size={14} />
            <span>PR</span>
          </button>
          <button
            type="button"
            className="tb-icon-btn"
            aria-pressed={terminalOpen}
            disabled={busy || !hasActiveWorktree}
            title={t("toolbar.terminal")}
            aria-label={t("toolbar.terminal")}
            onClick={onToggleTerminal}
          >
            <AppIcon name="terminal" size={15} />
          </button>
        </>
      ) : null}
      <button
        type="button"
        className="tb-icon-btn"
        title={messages.settings.open}
        aria-label={messages.settings.open}
        onClick={onSettings}
      >
        <AppIcon name="gear" size={15} />
        {updateAvailable ? <span className="update-dot" aria-hidden /> : null}
      </button>
    </header>
  );
}

export function StatusBar({
  branch,
  ahead,
  behind,
  watchError,
  reviewed,
  total,
  codeThemeLabel,
  onRefresh,
  onOpenCommandLog,
  autoFetchError,
  environmentLoading = false,
}: {
  branch: string | null;
  ahead: number;
  behind: number;
  watchError: AppErrorPayload | null;
  reviewed: number;
  total: number;
  codeThemeLabel?: string;
  onRefresh: () => void;
  onOpenCommandLog: () => void;
  autoFetchError: string | null;
  environmentLoading?: boolean;
}) {
  return (
    <footer className="window-status flex shrink-0 items-center gap-1 overflow-hidden px-2.5 whitespace-nowrap">
      <span className="status-item">
        <AppIcon name="git-branch" size={12} />
        <span className="max-w-[240px] truncate">{branch ?? messages.changes.detachedHead}</span>
      </span>
      <span className="status-item font-mono tabular-nums" title={messages.statusBar.upstreamHint}>
        <AppIcon name="arrow-up" size={11} />
        {ahead}
        <AppIcon name="arrow-down" size={11} />
        {behind}
        <span className="ml-1 font-sans text-[var(--text-muted)]">{messages.statusBar.asOfLastFetch}</span>
      </span>
      {environmentLoading ? (
        <span className="status-item" title={messages.statusBar.environmentLoadingHint}>
          <AppIcon name="sync" size={12} spin />
          {messages.statusBar.environmentLoading}
        </span>
      ) : null}
      {watchError ? (
        <button
          type="button"
          className="status-item text-[var(--warn)]"
          title={formatAppError(watchError)}
          onClick={onRefresh}
        >
          <AppIcon name="refresh" size={12} />
          {messages.changes.watchUnavailable}
        </button>
      ) : null}
      <button
        type="button"
        className="status-item"
        title={messages.commandLog.open}
        onClick={onOpenCommandLog}
      >
        <AppIcon name="terminal" size={12} />
        {messages.commandLog.title}
      </button>
      {autoFetchError ? (
        <button
          type="button"
          className="status-item text-[var(--warn)]"
          title={autoFetchError}
          onClick={onOpenCommandLog}
        >
          <AppIcon name="cloud-offline" size={12} />
          {messages.remote.autoFetchFailed}
        </button>
      ) : null}
      <span className="flex-1" />
      <span className="status-item tabular-nums">
        <AppIcon name="pass" size={12} className="text-[var(--reviewed)]" />
        {messages.review.progress(reviewed, total)}
      </span>
      {codeThemeLabel ? (
        <span className="status-item text-[var(--text-muted)]">{codeThemeLabel}</span>
      ) : null}
    </footer>
  );
}

/** 选择与移除是并列按钮，移除不会冒泡触发打开仓库。 */
function RepositoryListItem({ repo, selected, disabled, onSelect, onRemove }: {
  repo: RepoEntry;
  selected?: boolean;
  disabled: boolean;
  onSelect: () => void;
  onRemove: () => void;
}) {
  return (
    <div className="flex items-center gap-1 rounded-[6px] hover:bg-[var(--surface-hover)]">
      <button
        type="button"
        role="menuitem"
        className="flex min-w-0 flex-1 items-center gap-2 px-2 py-1.5 text-left disabled:opacity-50"
        title={repo.path}
        disabled={disabled}
        onClick={onSelect}
      >
        <span className={`flex w-4 shrink-0 justify-center ${selected ? "text-[var(--accent)]" : "text-[var(--text-muted)]"}`}>
          <AppIcon name={selected ? "check" : "repo"} size={14} />
        </span>
        <span className="min-w-0 flex-1">
          <span className="block truncate font-medium">{repo.name}</span>
          <span className="block truncate text-[11px] text-[var(--text-muted)]">{repo.path}</span>
          {!repo.exists || repo.worktrees.length === 0 ? (
            <span className="block text-[11px] text-[var(--warn)]">{messages.toolbar.inaccessible}</span>
          ) : null}
        </span>
      </button>
      <button
        type="button"
        className="mini-icon-btn mr-1 shrink-0"
        title={t("repository.removeHint")}
        aria-label={t("repository.remove", { name: repo.name })}
        disabled={disabled}
        onClick={onRemove}
      >
        <AppIcon name="close" size={13} />
      </button>
    </div>
  );
}

export function WelcomeScreen({
  repos,
  busy,
  onChooseRepo,
  onSelectRepo,
  onCloneRepo,
  onRemoveRepo,
  error,
}: {
  repos: RepoEntry[];
  busy: boolean;
  onChooseRepo: () => void;
  onSelectRepo: (repo: RepoEntry) => void;
  onCloneRepo: () => void;
  onRemoveRepo: (repo: RepoEntry) => void;
  error: ReactNode;
}) {
  return (
    <div className="flex min-h-0 flex-1 flex-col items-center justify-center gap-4 overflow-auto p-8" data-tauri-drag-region>
      <div className="w-full max-w-[520px]">
        <div className="mx-auto mb-4 grid size-11 place-items-center rounded-[10px] bg-[var(--accent)] text-[18px] font-bold text-white shadow-sm">
          R
        </div>
        <h1 className="text-center text-[19px] font-semibold tracking-tight text-[var(--text-primary)]">{messages.firstRun.title}</h1>
        <p className="mt-2 text-center text-[13px] text-[var(--text-secondary)]">{messages.firstRun.description}</p>
        <div className="mt-5 flex justify-center gap-2.5">
          <button
            type="button"
            className="revia-btn revia-btn-primary h-9 px-4"
            disabled={busy}
            onClick={onChooseRepo}
          >
            <AppIcon name="add" size={14} />
            {messages.firstRun.addButton}
          </button>
          <button
            type="button"
            className="revia-btn revia-btn-secondary h-9 px-4"
            disabled={busy}
            onClick={onCloneRepo}
          >
            <AppIcon name="repo-clone" size={14} />
            {t("toolbar.clone")}
          </button>
        </div>
        {repos.length > 0 ? (
          <ul className="welcome-card mt-6 divide-y divide-[var(--border-subtle)] overflow-hidden p-1.5">
            {repos.map((repo) => (
              <li key={repo.id}>
                <RepositoryListItem repo={repo} disabled={busy} onSelect={() => onSelectRepo(repo)} onRemove={() => onRemoveRepo(repo)} />
              </li>
            ))}
          </ul>
        ) : null}
        {error ? <div className="mt-4">{error}</div> : null}
      </div>
    </div>
  );
}
