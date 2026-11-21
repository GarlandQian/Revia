/**
 * 主区域页签栏与单条统一 Diff 上下文栏（M13 Q240、Q241、Q244）。
 *
 * - 页签栏（36px）：常驻「提交日志」+ 打开的提交 diff / 工作区 diff / 三方合并 / GitHub PR 页签；
 * - 统一 Diff 上下文栏（38px）：合并旧版面包屑与页签工具条，左侧路径面包屑与两侧对比标签，
 *   右侧文件切换、差异导航、并排/统一分段切换、显示选项弹窗、已审阅切换与更多操作。
 */

import { useLayoutEffect, useRef, useState, type ReactNode } from "react";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesKind } from "@/types/bindings/ChangesKind";
import type { ViewMode } from "@/types/bindings/ViewMode";
import type { DiffHighlight } from "@/features/diff/SideBySideView";
import { messages, t } from "@/lib/i18n/index";
import { highlightLevelText } from "@/lib/i18n/text";
import { AppIcon } from "@/components/ui/app-icon";
import { Popover, MenuItem, MenuSeparator } from "@/components/ui/popover";
import { FileTypeIcon } from "./FileTypeIcon";

export interface CommitDiffTabInfo {
  key: string;
  kind: "diff" | "merge";
  title: string;
  path: string;
  commit: string;
}

export interface EditorTab {
  key: string;
  path: string;
  changes: ChangesKind;
  preview: boolean;
  kind?: "diff" | "merge";
}

export function mergeTabKey(path: string): string {
  return `merge:${path}`;
}

function tabKey(path: string, changes: ChangesKind): string {
  return `${changes}:${path}`;
}

export const LOG_TAB_KEY = "log";
export const PR_TAB_KEY = "pr";

export { tabKey };

export interface DiffToolbarOptions {
  ignoreWhitespace: number;
  highlight: DiffHighlight;
  foldUnchanged: boolean;
  syncScroll: boolean;
  showWhitespace: boolean;
  softWrap: boolean;
  jumpToNextFile: boolean;
  contextLines: number;
}

export function EditorTabs({
  tabs,
  activeKey,
  entry,
  view,
  canNavigate,
  isWriting,
  changeCount,
  fileIndex,
  fileTotal,
  diffOptions,
  onDiffOption,
  onActivate,
  onClose,
  onCloseAll,
  onView,
  onJump,
  onFilePrev,
  onFileNext,
  onToggleReview,
  onRefresh,
  onFileHistory,
  onExternalDiff,
  logActive,
  onActivateLog,
  commitTabs,
  activeCommitKey,
  onActivateCommit,
  onCloseCommit,
  prOpen = false,
  prActive = false,
  isMerge = false,
  onActivatePr,
  onClosePr,
  editAction,
}: {
  tabs: EditorTab[];
  activeKey: string | null;
  entry: ChangeEntry | null;
  view: ViewMode;
  canNavigate: boolean;
  isWriting: boolean;
  changeCount: number;
  fileIndex: number;
  fileTotal: number;
  diffOptions: DiffToolbarOptions;
  onDiffOption: (patch: Partial<DiffToolbarOptions>) => void;
  onActivate: (tab: EditorTab) => void;
  onClose: (tab: EditorTab) => void;
  onCloseAll: () => void;
  onView: (view: ViewMode) => void;
  onJump: (direction: 1 | -1) => void;
  onFilePrev: () => void;
  onFileNext: () => void;
  onToggleReview: () => void;
  onRefresh: () => void;
  onFileHistory: (path: string) => void;
  onExternalDiff: () => void;
  logActive: boolean;
  onActivateLog: () => void;
  commitTabs: CommitDiffTabInfo[];
  activeCommitKey: string | null;
  onActivateCommit: (tab: CommitDiffTabInfo) => void;
  onCloseCommit: (tab: CommitDiffTabInfo) => void;
  prOpen?: boolean;
  prActive?: boolean;
  isMerge?: boolean;
  onActivatePr?: () => void;
  onClosePr?: () => void;
  editAction?: ReactNode;
}) {
  const [moreOpen, setMoreOpen] = useState(false);
  const [optionsOpen, setOptionsOpen] = useState(false);
  const tabListRef = useRef<HTMLDivElement>(null);
  const activeTabRef = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const list = tabListRef.current;
    const tab = activeTabRef.current;
    if (!list || !tab) return;
    const reveal = () => {
      const left = tab.offsetLeft;
      const right = left + tab.offsetWidth;
      if (left < list.scrollLeft || tab.offsetWidth > list.clientWidth) {
        list.scrollLeft = left;
      } else if (right > list.scrollLeft + list.clientWidth) {
        list.scrollLeft = right - list.clientWidth;
      }
    };
    reveal();
    const observer = new ResizeObserver(reveal);
    observer.observe(list);
    observer.observe(tab);
    return () => observer.disconnect();
  }, [activeKey, activeCommitKey, logActive, prActive, prOpen, tabs, commitTabs]);

  const reviewed = entry?.reviewed ?? false;
  const showDiffContextBar = !logActive && !activeCommitKey && !prActive && !isMerge && Boolean(entry);
  const segments = entry ? entry.path.split("/") : [];
  const changesLabel = entry
    ? entry.changes === "staged"
      ? messages.changes.staged
      : entry.changes === "untracked"
        ? messages.changes.untracked
        : messages.changes.unstaged
    : "";

  return (
    <div className="flex shrink-0 flex-col">
      {/* 36px 主工作区页签栏 */}
      <div className="editor-tabs">
        <div ref={tabListRef} className="editor-tab-list" role="tablist">
          {/* 常驻「提交日志」页签 */}
          <div
            key={LOG_TAB_KEY}
            ref={logActive && !prActive ? activeTabRef : null}
            className="editor-tab group"
            data-active={logActive && !prActive}
            data-tab-key={LOG_TAB_KEY}
            role="tab"
            aria-selected={logActive && !prActive}
            tabIndex={logActive && !prActive ? 0 : -1}
            title={messages.log.title}
            onClick={onActivateLog}
            onDoubleClick={onActivateLog}
            onKeyDown={(event) => {
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onActivateLog();
              }
            }}
          >
            <AppIcon name="git-commit" size={14} />
            <span className="tab-title min-w-0 truncate">{messages.log.title}</span>
          </div>

          {/* 提交 diff 页签 */}
          {commitTabs.map((tab) => {
            const active = !prActive && tab.key === activeCommitKey;
            return (
              <div
                key={`commit-${tab.key}`}
                ref={active ? activeTabRef : null}
                className="editor-tab group"
                data-active={active}
                data-tab-key={`commit-${tab.key}`}
                role="tab"
                aria-selected={active}
                tabIndex={active ? 0 : -1}
                title={tab.path}
                onClick={() => onActivateCommit(tab)}
                onDoubleClick={() => onActivateCommit(tab)}
                onAuxClick={(event) => {
                  if (event.button === 1) onCloseCommit(tab);
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    onActivateCommit(tab);
                  }
                }}
              >
                <FileTypeIcon path={tab.path} />
                <span className="tab-title min-w-0 truncate">{tab.title}</span>
                <button
                  type="button"
                  className={`mini-icon-btn ml-0.5 size-4 shrink-0 ${active ? "" : "opacity-0 group-hover:opacity-100"}`}
                  aria-label={messages.changes.closeTab}
                  title={messages.changes.closeTab}
                  onClick={(event) => {
                    event.stopPropagation();
                    onCloseCommit(tab);
                  }}
                >
                  <AppIcon name="close" size={12} />
                </button>
              </div>
            );
          })}

          {/* 工作区 diff / 三方合并页签 */}
          {tabs.map((tab) => {
            const slash = tab.path.lastIndexOf("/");
            const name = tab.path.slice(slash + 1);
            const group =
              tab.changes === "staged"
                ? messages.changes.staged
                : tab.changes === "untracked"
                  ? messages.changes.untracked
                  : messages.changes.unstaged;
            const active = !prActive && !logActive && !activeCommitKey && tab.key === activeKey;
            const merge = tab.kind === "merge";
            return (
              <div
                key={tab.key}
                ref={active ? activeTabRef : null}
                className="editor-tab group"
                data-active={active}
                data-preview={tab.preview}
                data-tab-key={tab.key}
                role="tab"
                aria-selected={active}
                tabIndex={active ? 0 : -1}
                title={merge ? messages.merge.tabTitle(name) : `${tab.path} · ${group}`}
                onClick={() => onActivate(tab)}
                onDoubleClick={() => onActivate(tab)}
                onAuxClick={(event) => {
                  if (event.button === 1) onClose(tab);
                }}
                onKeyDown={(event) => {
                  if (event.key === "Enter" || event.key === " ") {
                    event.preventDefault();
                    onActivate(tab);
                  }
                }}
              >
                <FileTypeIcon path={tab.path} />
                <span className="tab-title min-w-0 truncate">{name}</span>
                <span className="shrink-0 text-[11px] text-[var(--text-muted)]">
                  {merge ? messages.merge.title : group}
                </span>
                <button
                  type="button"
                  className={`mini-icon-btn ml-0.5 size-4 shrink-0 ${active ? "" : "opacity-0 group-hover:opacity-100"}`}
                  aria-label={messages.changes.closeTab}
                  title={messages.changes.closeTab}
                  onClick={(event) => {
                    event.stopPropagation();
                    onClose(tab);
                  }}
                >
                  <AppIcon name="close" size={12} />
                </button>
              </div>
            );
          })}

          {/* GitHub PR 主区页签（Q241） */}
          {prOpen && onActivatePr ? (
            <div
              key={PR_TAB_KEY}
              ref={prActive ? activeTabRef : null}
              className="editor-tab group"
              data-active={prActive}
              data-tab-key={PR_TAB_KEY}
              role="tab"
              aria-selected={prActive}
              tabIndex={prActive ? 0 : -1}
              title={t("toolbar.pullRequests")}
              onClick={onActivatePr}
              onAuxClick={(event) => {
                if (event.button === 1 && onClosePr) onClosePr();
              }}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") {
                  event.preventDefault();
                  onActivatePr();
                }
              }}
            >
              <AppIcon name="git-pull-request" size={14} className="text-[var(--git-a)]" />
              <span className="tab-title min-w-0 truncate">{t("toolbar.pullRequests")}</span>
              {onClosePr ? (
                <button
                  type="button"
                  className={`mini-icon-btn ml-0.5 size-4 shrink-0 ${prActive ? "" : "opacity-0 group-hover:opacity-100"}`}
                  aria-label={messages.changes.closeTab}
                  title={messages.changes.closeTab}
                  onClick={(event) => {
                    event.stopPropagation();
                    onClosePr();
                  }}
                >
                  <AppIcon name="close" size={12} />
                </button>
              ) : null}
            </div>
          ) : null}
        </div>
      </div>

      {/* 单条统一 Diff 上下文栏（38px，Q244）：合并面包屑 + 差异导航 + 视图切换 + 显示选项 + 已审阅 */}
      {showDiffContextBar && entry ? (
        <div className="diff-context-bar">
          <div className="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden text-[12px] text-[var(--text-secondary)]">
            <FileTypeIcon path={entry.path} symlink={entry.readonlyReason === "symlink"} className="size-3.5" />
            <div className="flex min-w-0 items-center gap-1 overflow-hidden">
              {segments.map((segment, index) => (
                <span key={`${index}-${segment}`} className="flex min-w-0 items-center gap-1">
                  {index > 0 ? <span className="text-[var(--text-muted)]">/</span> : null}
                  <span className={index === segments.length - 1 ? "truncate font-semibold text-[var(--text-primary)]" : "truncate"}>
                    {segment}
                  </span>
                </span>
              ))}
            </div>
            <span className="diff-sides-tag">{changesLabel}</span>
          </div>

          <div className="flex shrink-0 items-center gap-1.5">
            {fileTotal > 1 ? (
              <div className="diff-nav-pill">
                <button
                  type="button"
                  className="mini-icon-btn"
                  title={messages.diff.previousFile}
                  aria-label={messages.diff.previousFile}
                  onClick={onFilePrev}
                >
                  <AppIcon name="arrow-left" size={12} />
                </button>
                <span className="px-1 font-mono text-[11px] tabular-nums" data-file-position>
                  {messages.diff.filePosition(Math.max(1, fileIndex), Math.max(1, fileTotal))}
                </span>
                <button
                  type="button"
                  className="mini-icon-btn"
                  title={messages.diff.nextFile}
                  aria-label={messages.diff.nextFile}
                  onClick={onFileNext}
                >
                  <AppIcon name="arrow-right" size={12} />
                </button>
              </div>
            ) : null}

            <div className="diff-nav-pill">
              <button
                type="button"
                className="mini-icon-btn"
                disabled={!canNavigate}
                title={messages.diff.previousHunkShortcut}
                aria-label={messages.diff.previousHunkShortcut}
                onClick={() => onJump(-1)}
              >
                <AppIcon name="chevron-up" size={13} />
              </button>
              <span className="px-1 font-mono text-[11px] tabular-nums" data-change-count>
                {messages.diff.changeCount(changeCount)}
              </span>
              <button
                type="button"
                className="mini-icon-btn"
                disabled={!canNavigate}
                title={messages.diff.nextHunkShortcut}
                aria-label={messages.diff.nextHunkShortcut}
                onClick={() => onJump(1)}
              >
                <AppIcon name="chevron-down" size={13} />
              </button>
            </div>

            <div className="seg-switch" role="group" aria-label={messages.diff.split}>
              <button
                type="button"
                aria-pressed={view === "split"}
                data-view-toggle={view === "split" ? undefined : "split"}
                onClick={() => onView("split")}
              >
                {messages.diff.split}
              </button>
              <button
                type="button"
                aria-pressed={view === "unified"}
                data-view-toggle={view === "unified" ? undefined : "unified"}
                onClick={() => onView("unified")}
              >
                {messages.diff.unified}
              </button>
            </div>

            <div className="relative">
              <Popover
                open={optionsOpen}
                onOpen={() => setOptionsOpen(true)}
                onClose={() => setOptionsOpen(false)}
                align="right"
                width={260}
                trigger={
                  <button
                    type="button"
                    className="icon-action"
                    title={messages.diff.options}
                    aria-label={messages.diff.options}
                    data-diff-options
                  >
                    <AppIcon name="sliders" size={14} />
                  </button>
                }
              >
                <MenuItem selected={diffOptions.syncScroll} onClick={() => onDiffOption({ syncScroll: !diffOptions.syncScroll })}>
                  {messages.diff.syncScroll}
                </MenuItem>
                <MenuItem selected={diffOptions.foldUnchanged} onClick={() => onDiffOption({ foldUnchanged: !diffOptions.foldUnchanged })}>
                  {messages.diff.foldUnchanged}
                </MenuItem>
                <MenuItem selected={diffOptions.showWhitespace} onClick={() => onDiffOption({ showWhitespace: !diffOptions.showWhitespace })}>
                  {messages.diff.showWhitespace}
                </MenuItem>
                <MenuItem selected={diffOptions.softWrap} onClick={() => onDiffOption({ softWrap: !diffOptions.softWrap })}>
                  {messages.diff.softWrap}
                </MenuItem>
                <MenuItem selected={diffOptions.jumpToNextFile} onClick={() => onDiffOption({ jumpToNextFile: !diffOptions.jumpToNextFile })}>
                  {messages.diff.jumpToNextFile}
                </MenuItem>
                <MenuSeparator />
                <div className="px-2 py-1 text-[11px] font-medium text-[var(--text-muted)]">{messages.diff.highlightLabel}</div>
                {(["word", "line", "char", "none"] as DiffHighlight[]).map((level) => (
                  <MenuItem key={level} selected={diffOptions.highlight === level} onClick={() => onDiffOption({ highlight: level })}>
                    {highlightLevelText(level)}
                  </MenuItem>
                ))}
                <MenuSeparator />
                <div className="px-2 py-1 text-[11px] font-medium text-[var(--text-muted)]">{messages.diff.ignoreWhitespace}</div>
                {([0, 1, 2, 3] as const).map((level) => (
                  <MenuItem key={level} selected={diffOptions.ignoreWhitespace === level} onClick={() => onDiffOption({ ignoreWhitespace: level })}>
                    {level === 0
                      ? messages.diff.ignoreWhitespaceLevels.off
                      : level === 1
                        ? messages.diff.ignoreWhitespaceLevels.eol
                        : level === 2
                          ? messages.diff.ignoreWhitespaceLevels.all
                          : messages.diff.ignoreWhitespaceLevels.allBlank}
                  </MenuItem>
                ))}
                <MenuSeparator />
                <div className="flex items-center gap-2 px-2 py-1 text-[11.5px]">
                  <span className="flex-1 text-[var(--text-secondary)]">{messages.diff.contextLines}</span>
                  <input
                    className="settings-input h-6 w-14 text-center"
                    type="number"
                    min={0}
                    max={64}
                    value={diffOptions.contextLines}
                    onChange={(event) => {
                      const parsed = Number.parseInt(event.target.value, 10);
                      if (Number.isFinite(parsed)) onDiffOption({ contextLines: parsed });
                    }}
                  />
                </div>
              </Popover>
            </div>

            {editAction}

            <button
              type="button"
              className="reviewed-toggle-btn"
              aria-pressed={reviewed}
              disabled={isWriting || (!reviewed && !entry.worktreeVersionConfirmed)}
              aria-label={reviewed ? messages.review.unmark : messages.review.mark}
              title={
                reviewed
                  ? messages.review.unmark
                  : entry.worktreeVersionConfirmed
                    ? messages.review.mark
                    : messages.review.unconfirmed
              }
              onClick={onToggleReview}
            >
              <AppIcon name={reviewed ? "check" : "eye"} size={13} />
              <span>{reviewed ? messages.review.marked : messages.review.mark}</span>
            </button>

            <div className="relative">
              <Popover
                open={moreOpen}
                onOpen={() => setMoreOpen(true)}
                onClose={() => setMoreOpen(false)}
                align="right"
                width={200}
                trigger={
                  <button
                    type="button"
                    className="icon-action"
                    title={messages.changes.more}
                    aria-label={messages.changes.more}
                  >
                    <AppIcon name="ellipsis" size={15} />
                  </button>
                }
              >
                <MenuItem onClick={() => onClose({ key: tabKey(entry.path, entry.changes), path: entry.path, changes: entry.changes, preview: false })}>
                  {messages.changes.closeTab}
                </MenuItem>
                <MenuItem disabled={tabs.length === 0} onClick={onCloseAll}>
                  {messages.changes.closeAllTabs}
                </MenuItem>
                <MenuSeparator />
                <MenuItem onClick={() => onFileHistory(entry.path)}>
                  {messages.fileHistory.open}
                </MenuItem>
                <MenuItem disabled={entry.unmerged} onClick={onExternalDiff}>
                  {messages.externalTools.openDiff}
                </MenuItem>
                <MenuSeparator />
                <MenuItem onClick={onRefresh}>{messages.changes.refresh}</MenuItem>
              </Popover>
            </div>
          </div>
        </div>
      ) : null}
    </div>
  );
}
