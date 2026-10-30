/**
 * 主窗口框架（M4 F02、Q119、Q126）：主工具栏、工具窗口图标条、状态栏与欢迎界面。
 *
 * 主工具栏左侧是 macOS 红绿灯留白；仓库组件与 worktree 组件点开是下拉列表；
 * 右侧是设置按钮。状态栏显示当前分支、领先和落后数、自动刷新提示与审阅进度。
 */

import { useState } from "react";
import type { RepoEntry } from "@/types/bindings/RepoEntry";
import type { WorktreeEntry } from "@/types/bindings/WorktreeEntry";
import type { WorktreeStatusEntry } from "@/types/bindings/WorktreeStatusEntry";
import type { AppErrorPayload } from "@/types/bindings/AppErrorPayload";
import { messages, t } from "@/lib/i18n/index";
import { formatAppError } from "@/lib/error";
import { MAIN_TOOLBAR_INSET } from "@/lib/platform";
import { Codicon } from "@/components/ui/codicon";
import { Popover, MenuItem, MenuSeparator } from "@/components/ui/popover";

export function TitleBar({
  repo,
  repos,
  worktrees,
  statuses,
  activeWorktreeId,
  busy,
  onSelectRepo,
  onChooseRepo,
  onWorktree,
  onSettings,
  onRefreshWorktrees,
  extra,
}: {
  repo: RepoEntry | null;
  repos: RepoEntry[];
  worktrees: WorktreeEntry[];
  statuses: WorktreeStatusEntry[];
  activeWorktreeId: string | null;
  busy: boolean;
  onSelectRepo: (repo: RepoEntry) => void;
  onChooseRepo: () => void;
  onWorktree: (worktreeId: string) => void;
  onSettings: () => void;
  onRefreshWorktrees: () => void;
  /** 分支组件与“更新 / 推送”按钮（M5 B02）。 */
  extra?: React.ReactNode;
}) {
  const [repoOpen, setRepoOpen] = useState(false);
  const [worktreeOpen, setWorktreeOpen] = useState(false);
  const active = worktrees.find((worktree) => worktree.id === activeWorktreeId);
  const statusById = new Map(statuses.map((status) => [status.id, status]));
  const activeStatus = activeWorktreeId ? statusById.get(activeWorktreeId) : undefined;
  const branch = activeStatus?.branch ?? active?.branch ?? null;
  const name = repo?.name ?? messages.app.name;

  return (
    <header className="titlebar relative" data-tauri-drag-region style={{ paddingLeft: MAIN_TOOLBAR_INSET }}>
      <div className="relative">
        <Popover
          open={repoOpen}
          onOpen={() => setRepoOpen(true)}
          onClose={() => setRepoOpen(false)}
          width={340}
          trigger={
            <button
              type="button"
              className="toolbar-widget"
              disabled={busy}
              title={repo ? `${repo.path}\n${messages.toolbar.chooseRepo}` : messages.toolbar.chooseRepo}
            >
              <span aria-hidden className="grid size-[18px] shrink-0 place-items-center rounded-[4px] bg-[var(--badge-background)] text-[11px] font-bold text-[var(--badge-foreground)]">
                {name.slice(0, 1).toUpperCase()}
              </span>
              <span className="max-w-[220px] truncate">{name}</span>
              <Codicon name="chevron-down" size={12} />
            </button>
          }
        >
          <p className="px-2 py-1 text-[11px] text-descriptionforeground">{messages.toolbar.repositories}</p>
          {repos.map((entry) => (
            <MenuItem
              key={entry.id}
              selected={entry.id === repo?.id}
              title={entry.path}
              onClick={() => {
                setRepoOpen(false);
                onSelectRepo(entry);
              }}
            >
              <span className="block truncate">{entry.name}</span>
              <span className="block truncate text-[11px] text-descriptionforeground">{entry.path}</span>
            </MenuItem>
          ))}
          <MenuSeparator />
          <MenuItem
            onClick={() => {
              setRepoOpen(false);
              onChooseRepo();
            }}
          >
            <span className="flex items-center gap-1.5">
              <Codicon name="add" size={14} />
              {messages.toolbar.addRepository}
            </span>
          </MenuItem>
        </Popover>
      </div>

      {active ? (
        <div className="relative min-w-0">
          <Popover
            open={worktreeOpen}
            onOpen={() => setWorktreeOpen(true)}
            onClose={() => setWorktreeOpen(false)}
            width={420}
            trigger={
              <button
                type="button"
                className="toolbar-widget min-w-0"
                disabled={busy || worktrees.length === 0}
                title={active.path}
                onClick={() => {
                  if (!worktreeOpen) onRefreshWorktrees();
                }}
              >
                <Codicon name="git-branch" size={14} />
                <span className="max-w-[260px] truncate">{branch ?? messages.changes.detachedHead}</span>
                <Codicon name="chevron-down" size={12} />
              </button>
            }
          >
            <p className="px-2 py-1 text-[11px] text-descriptionforeground">{messages.toolbar.worktrees}</p>
            {worktrees.map((worktree) => {
              const status = statusById.get(worktree.id);
              const activeRow = worktree.id === activeWorktreeId;
              return (
                <button
                  key={worktree.id}
                  type="button"
                  role="menuitem"
                  title={worktree.path}
                  className="flex w-full items-start gap-2 rounded-[3px] px-2 py-1.5 text-left hover:bg-toolbar-hoverbackground"
                  onClick={() => {
                    setWorktreeOpen(false);
                    onWorktree(worktree.id);
                  }}
                >
                  <span className="mt-0.5 flex w-4 shrink-0 justify-center">
                    {activeRow ? <Codicon name="check" size={13} /> : null}
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="flex items-center gap-1.5">
                      <span className="truncate font-medium">{status?.branch ?? worktree.branch ?? messages.changes.detachedHead}</span>
                      {worktree.isMain ? (
                        <span className="shrink-0 rounded bg-[var(--badge-background)] px-1 text-[10px] text-[var(--badge-foreground)]">
                          {messages.changes.worktreeMain}
                        </span>
                      ) : null}
                      {status?.dirty ? (
                        <span className="size-1.5 shrink-0 rounded-full bg-[var(--gitdecoration-modifiedresourceforeground)]" title={messages.toolbar.dirty} />
                      ) : null}
                    </span>
                    <span className="block truncate text-[11px] text-descriptionforeground">{worktree.path}</span>
                    {status ? (
                      <span className="mt-0.5 flex flex-wrap items-center gap-x-3 gap-y-0.5 text-[11px] text-descriptionforeground">
                        <span>{status.dirty ? messages.toolbar.dirty : messages.toolbar.clean}</span>
                        {status.operationLabel ? (
                          <span className="text-[var(--editorwarning-foreground)]">
                            {messages.toolbar.operation(status.operationLabel)}
                          </span>
                        ) : null}
                        <span className="inline-flex items-center gap-1 tabular-nums">
                          <Codicon name="arrow-up" size={11} />
                          {status.ahead}
                          <Codicon name="arrow-down" size={11} />
                          {status.behind}
                        </span>
                        {status.error ? <span className="text-destructive">{formatAppError(status.error)}</span> : null}
                        {!status.accessible ? <span>{messages.toolbar.inaccessible}</span> : null}
                      </span>
                    ) : null}
                  </span>
                </button>
              );
            })}
          </Popover>
        </div>
      ) : null}

      {extra}
      <span className="flex-1 self-stretch" data-tauri-drag-region />
      <button type="button" className="icon-action" title={messages.settings.open} aria-label={messages.settings.open} onClick={onSettings}>
        <Codicon name="settings-gear" size={16} />
      </button>
    </header>
  );
}

export function ActivityBar({
  pressed,
  count,
  onToggle,
}: {
  pressed: boolean;
  count: number;
  onToggle: () => void;
}) {
  return (
    <nav className="activity-bar" aria-label={messages.toolbar.toolWindows}>
      <button
        type="button"
        className="activity-item"
        aria-pressed={pressed}
        title={messages.changes.toolWindow}
        aria-label={messages.changes.toolWindow}
        onClick={onToggle}
      >
        <Codicon name="source-control" size={22} />
        {count > 0 ? <span className="activity-badge">{count > 99 ? "99+" : count}</span> : null}
      </button>
    </nav>
  );
}

export function StatusBar({
  branch,
  ahead,
  behind,
  watchError,
  reviewed,
  total,
  onRefresh,
  onOpenCommandLog,
  autoFetchError,
}: {
  branch: string | null;
  ahead: number;
  behind: number;
  watchError: AppErrorPayload | null;
  reviewed: number;
  total: number;
  onRefresh: () => void;
  /** 打开 Git 命令日志面板（B07）。 */
  onOpenCommandLog: () => void;
  /** 自动 fetch 失败的提示（只在状态栏，不弹通知）。 */
  autoFetchError: string | null;
}) {
  return (
    <footer className="window-status flex h-[var(--statusbar-height)] shrink-0 items-center gap-0.5 overflow-hidden px-1.5 text-[12px] whitespace-nowrap text-[var(--statusbar-foreground)]">
      <span className="inline-flex h-[var(--statusbar-height)] items-center gap-1 px-1.5">
        <Codicon name="git-branch" size={14} />
        <span className="max-w-[260px] truncate">{branch ?? messages.changes.detachedHead}</span>
      </span>
      <span className="inline-flex h-[var(--statusbar-height)] items-center gap-1 px-1.5 tabular-nums" title={messages.statusBar.upstreamHint}>
        <Codicon name="arrow-up" size={13} />
        {ahead}
        <Codicon name="arrow-down" size={13} />
        {behind}
        <span className="ml-1">{messages.statusBar.asOfLastFetch}</span>
      </span>
      {watchError ? (
        <button
          type="button"
          className="inline-flex h-[var(--statusbar-height)] items-center gap-1 px-1.5 text-[var(--editorwarning-foreground)]"
          title={formatAppError(watchError)}
          onClick={onRefresh}
        >
          <Codicon name="refresh" size={13} />
          {messages.changes.watchUnavailable}
        </button>
      ) : null}
      <button
        type="button"
        className="inline-flex h-[var(--statusbar-height)] items-center gap-1 px-1.5"
        title={messages.commandLog.open}
        onClick={onOpenCommandLog}
      >
        <Codicon name="terminal" size={13} />
        {messages.commandLog.title}
      </button>
      {autoFetchError ? (
        <button
          type="button"
          className="inline-flex h-[var(--statusbar-height)] items-center gap-1 px-1.5 text-[var(--editorwarning-foreground)]"
          title={autoFetchError}
          onClick={onOpenCommandLog}
        >
          <Codicon name="cloud-offline" size={13} />
          {messages.remote.autoFetchFailed}
        </button>
      ) : null}
      <span className="flex-1" />
      <span className="inline-flex h-[var(--statusbar-height)] items-center gap-1 px-1.5 tabular-nums">
        <Codicon name="pass" size={13} />
        {messages.review.progress(reviewed, total)}
      </span>
    </footer>
  );
}

export function WelcomeScreen({
  repos,
  busy,
  onChooseRepo,
  onSelectRepo,
  onCloneRepo,
  error,
}: {
  repos: RepoEntry[];
  busy: boolean;
  onChooseRepo: () => void;
  onSelectRepo: (repo: RepoEntry) => void;
  onCloneRepo: () => void;
  error: React.ReactNode;
}) {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-4 p-8" data-tauri-drag-region>
      <div className="w-full max-w-lg">
        <h1 className="text-center text-[18px] font-semibold">{messages.firstRun.title}</h1>
        <p className="mt-2 text-center text-[12.5px] text-descriptionforeground">{messages.firstRun.description}</p>
        <div className="mt-5 flex justify-center gap-2">
          <button
            type="button"
            className="rounded-[4px] bg-primary px-4 py-1.5 text-primary-foreground disabled:opacity-50"
            disabled={busy}
            onClick={onChooseRepo}
          >
            {messages.firstRun.addButton}
          </button>
          <button
            type="button"
            className="rounded-[4px] border border-panel-border px-4 py-1.5 hover:bg-toolbar-hoverbackground disabled:opacity-50"
            disabled={busy}
            onClick={onCloneRepo}
          >
            {t("toolbar.clone")}
          </button>
        </div>
        {repos.length > 0 ? (
          <ul className="mt-6 overflow-hidden rounded border border-panel-border bg-[var(--sidebar-background)]">
            {repos.map((repo) => (
              <li key={repo.id}>
                <button
                  type="button"
                  className="flex w-full items-center gap-2 px-3 py-2 text-left hover:bg-toolbar-hoverbackground"
                  title={repo.path}
                  onClick={() => onSelectRepo(repo)}
                >
                  <Codicon name="repo" size={16} />
                  <span className="min-w-0 flex-1">
                    <span className="block truncate">{repo.name}</span>
                    <span className="block truncate text-[11px] text-descriptionforeground">{repo.path}</span>
                  </span>
                </button>
              </li>
            ))}
          </ul>
        ) : null}
        {error ? <div className="mt-4">{error}</div> : null}
      </div>
    </div>
  );
}
