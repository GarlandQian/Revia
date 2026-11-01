/**
 * 编辑器页签与页签工具条（M4 F05、Q119、Q121）。
 *
 * 页签标题是文件名加“未暂存 / 已暂存 / 未跟踪”；预览页签斜体，
 * 双击或 Enter 固定的页签为普通页签。右侧工具条：上一处 / 下一处改动、
 * 并排与统一切换、标记为已审阅、更多。下方是路径导航行。
 */

import { useLayoutEffect, useRef, useState } from "react";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesKind } from "@/types/bindings/ChangesKind";
import type { ViewMode } from "@/types/bindings/ViewMode";
import type { DiffHighlight } from "@/features/diff/SideBySideView";
import { messages } from "@/lib/i18n/index";
import { highlightLevelText } from "@/lib/i18n/text";
import { Codicon } from "@/components/ui/codicon";
import { Popover, MenuItem, MenuSeparator } from "@/components/ui/popover";
import { FileTypeIcon } from "./FileTypeIcon";
import { useGlassLens } from "@/features/m7/glass/glassLens";

/** 主区域固定的日志页签内容（M5 L01）：不可关闭，打开仓库后默认显示。 */
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
  /** 预览页签：单击文件产生，会被下一个预览替换。 */
  preview: boolean;
  /** 页签类型：diff（默认）或内置三方合并页签（返修 F10，Q143）。 */
  kind?: "diff" | "merge";
}

/** 合并页签的 key。 */
export function mergeTabKey(path: string): string {
  return `merge:${path}`;
}

function tabKey(path: string, changes: ChangesKind): string {
  return `${changes}:${path}`;
}

/** 日志页签的固定 key（不可关闭，也不进 diff 页签列表）。 */
export const LOG_TAB_KEY = "log";

export { tabKey };

/** diff 页签工具条上的显示选项（Q139：与设置同步）。 */
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
}: {
  tabs: EditorTab[];
  activeKey: string | null;
  entry: ChangeEntry | null;
  view: ViewMode;
  canNavigate: boolean;
  isWriting: boolean;
  /** 改动处数（段数）：并排视图的段数，统一视图退回块数。 */
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
  /** 文件历史 / Blame 与外部 diff（M6）。 */
  onFileHistory: (path: string) => void;
  onExternalDiff: () => void;
  /** 日志页签（固定在最左，不可关闭）。 */
  logActive: boolean;
  onActivateLog: () => void;
  /** 提交 diff 页签（与改动 diff 页签分型）。 */
  commitTabs: CommitDiffTabInfo[];
  activeCommitKey: string | null;
  onActivateCommit: (tab: CommitDiffTabInfo) => void;
  onCloseCommit: (tab: CommitDiffTabInfo) => void;
}) {
  const [moreOpen, setMoreOpen] = useState(false);
  const [optionsOpen, setOptionsOpen] = useState(false);
  const tabsRef = useRef<HTMLDivElement>(null);
  const tabListRef = useRef<HTMLDivElement>(null);
  const activeTabRef = useRef<HTMLDivElement>(null);
  // 切换页签时的玻璃镜片（Q202）；键与各页签的 data-tab-key 一致。
  useGlassLens({
    host: tabsRef,
    track: tabListRef,
    selected: logActive ? LOG_TAB_KEY : activeCommitKey ? `commit-${activeCommitKey}` : activeKey,
    attribute: "tabKey",
  });
  useLayoutEffect(() => {
    const list = tabListRef.current;
    const tab = activeTabRef.current;
    if (!list || !tab) return;
    const reveal = () => {
      const left = tab.offsetLeft;
      const right = left + tab.offsetWidth;
      // 使用未缩放的布局坐标；页签比视口宽时优先露出左侧文件名。
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
  }, [activeKey, activeCommitKey, logActive, tabs, commitTabs]);
  const reviewed = entry?.reviewed ?? false;
  return (
    <div ref={tabsRef} className="editor-tabs glass-lens-host flex shrink-0 flex-wrap items-stretch border-b border-[var(--editorgroupheader-tabsborder)] bg-[var(--editorgroupheader-tabsbackground)]">
      <div ref={tabListRef} className="editor-tab-list relative flex min-w-0 items-stretch overflow-x-auto [scrollbar-width:none] [&::-webkit-scrollbar]:hidden">
        {/* 固定的“日志”页签（L01）：不可关闭，打开仓库后默认显示。 */}
        <div
          key={LOG_TAB_KEY}
          ref={logActive ? activeTabRef : null}
          className="editor-tab group"
          data-active={logActive}
          data-tab-key={LOG_TAB_KEY}
          role="tab"
          aria-selected={logActive}
          tabIndex={logActive ? 0 : -1}
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
          <Codicon name="list-unordered" size={13} />
          <span className="tab-title min-w-0 truncate">{messages.log.title}</span>
        </div>
        {/* 提交 diff 页签：标题是“文件名（短哈希）”，与改动 diff 分型。 */}
        {commitTabs.map((tab) => {
          const active = tab.key === activeCommitKey;
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
                className={`icon-action ml-0.5 size-5 shrink-0 ${active ? "" : "opacity-0 group-hover:opacity-100"}`}
                aria-label={messages.changes.closeTab}
                title={messages.changes.closeTab}
                onClick={(event) => {
                  event.stopPropagation();
                  onCloseCommit(tab);
                }}
              >
                <Codicon name="close" size={14} />
              </button>
            </div>
          );
        })}
        {tabs.map((tab) => {
          const slash = tab.path.lastIndexOf("/");
          const name = tab.path.slice(slash + 1);
          const group =
            tab.changes === "staged"
              ? messages.changes.staged
              : tab.changes === "untracked"
                ? messages.changes.untracked
                : messages.changes.unstaged;
          const active = tab.key === activeKey;
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
              <span className="shrink-0 text-[11.5px] text-descriptionforeground">
                {merge ? messages.merge.title : group}
              </span>
              <button
                type="button"
                className={`icon-action ml-0.5 size-5 shrink-0 ${active ? "" : "opacity-0 group-hover:opacity-100"}`}
                aria-label={messages.changes.closeTab}
                title={messages.changes.closeTab}
                onClick={(event) => {
                  event.stopPropagation();
                  onClose(tab);
                }}
              >
                <Codicon name="close" size={14} />
              </button>
            </div>
          );
        })}
      </div>
      {logActive || activeCommitKey ? (
        <div className="flex shrink-0 items-center gap-0.5 px-2 text-[11.5px] text-descriptionforeground">
          {logActive ? messages.log.title : messages.history.readonly}
        </div>
      ) : null}
      <div className={`editor-tabs-toolbar ${logActive || activeCommitKey ? "hidden" : "flex"} shrink-0 items-center gap-0.5 px-2`}>
        <button
          type="button"
          className="icon-action"
          disabled={fileTotal <= 1}
          title={messages.diff.previousFile}
          aria-label={messages.diff.previousFile}
          onClick={onFilePrev}
        >
          <Codicon name="arrow-left" size={15} />
        </button>
        <span className="editor-toolbar-count shrink-0 whitespace-nowrap px-1 text-[11.5px] text-descriptionforeground" data-file-position>
          {messages.diff.filePosition(Math.max(1, fileIndex), Math.max(1, fileTotal))}
        </span>
        <button
          type="button"
          className="icon-action"
          disabled={fileTotal <= 1}
          title={messages.diff.nextFile}
          aria-label={messages.diff.nextFile}
          onClick={onFileNext}
        >
          <Codicon name="arrow-right" size={15} />
        </button>
        <span className="editor-toolbar-count shrink-0 whitespace-nowrap px-1 text-[11.5px] text-descriptionforeground" data-change-count>
          {messages.diff.changeCount(changeCount)}
        </span>
        <button
          type="button"
          className="icon-action"
          disabled={!canNavigate}
          title={messages.diff.previousHunkShortcut}
          aria-label={messages.diff.previousHunkShortcut}
          onClick={() => onJump(-1)}
        >
          <Codicon name="arrow-up" size={16} />
        </button>
        <button
          type="button"
          className="icon-action"
          disabled={!canNavigate}
          title={messages.diff.nextHunkShortcut}
          aria-label={messages.diff.nextHunkShortcut}
          onClick={() => onJump(1)}
        >
          <Codicon name="arrow-down" size={16} />
        </button>
        <button
          type="button"
          className="icon-action"
          title={view === "split" ? messages.diff.switchToUnified : messages.diff.switchToSplit}
          aria-label={view === "split" ? messages.diff.switchToUnified : messages.diff.switchToSplit}
          data-view-toggle={view === "split" ? "unified" : "split"}
          onClick={() => onView(view === "split" ? "unified" : "split")}
        >
          <Codicon name={view === "split" ? "split-horizontal" : "list-flat"} size={16} />
        </button>
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
                <Codicon name="settings-gear" size={16} />
              </button>
            }
          >
            <MenuItem
              selected={diffOptions.syncScroll}
              onClick={() => onDiffOption({ syncScroll: !diffOptions.syncScroll })}
            >
              {messages.diff.syncScroll}
            </MenuItem>
            <MenuItem
              selected={diffOptions.foldUnchanged}
              onClick={() => onDiffOption({ foldUnchanged: !diffOptions.foldUnchanged })}
            >
              {messages.diff.foldUnchanged}
            </MenuItem>
            <MenuItem
              selected={diffOptions.showWhitespace}
              onClick={() => onDiffOption({ showWhitespace: !diffOptions.showWhitespace })}
            >
              {messages.diff.showWhitespace}
            </MenuItem>
            <MenuItem
              selected={diffOptions.softWrap}
              onClick={() => onDiffOption({ softWrap: !diffOptions.softWrap })}
            >
              {messages.diff.softWrap}
            </MenuItem>
            <MenuItem
              selected={diffOptions.jumpToNextFile}
              onClick={() => onDiffOption({ jumpToNextFile: !diffOptions.jumpToNextFile })}
            >
              {messages.diff.jumpToNextFile}
            </MenuItem>
            <MenuSeparator />
            <div className="px-2 py-1 text-[11px] text-descriptionforeground">{messages.diff.highlightLabel}</div>
            {(["word", "line", "char", "none"] as DiffHighlight[]).map((level) => (
              <MenuItem
                key={level}
                selected={diffOptions.highlight === level}
                onClick={() => onDiffOption({ highlight: level })}
              >
                {highlightLevelText(level)}
              </MenuItem>
            ))}
            <MenuSeparator />
            <div className="px-2 py-1 text-[11px] text-descriptionforeground">{messages.diff.ignoreWhitespace}</div>
            {([0, 1, 2, 3] as const).map((level) => (
              <MenuItem
                key={level}
                selected={diffOptions.ignoreWhitespace === level}
                onClick={() => onDiffOption({ ignoreWhitespace: level })}
              >
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
              <span className="flex-1 text-descriptionforeground">{messages.diff.contextLines}</span>
              <input
                className="settings-input w-14"
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
        {entry ? (
          <button
            type="button"
            className="toolbar-widget h-6 gap-1.5 px-2 text-[12px]"
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
            <Codicon name={reviewed ? "pass-filled" : "eye"} size={15} />
            <span className="editor-review-label">{reviewed ? messages.review.marked : messages.review.mark}</span>
          </button>
        ) : null}
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
                <Codicon name="ellipsis" size={16} />
              </button>
            }
          >
            <MenuItem disabled={!entry} onClick={() => entry && onClose({ key: tabKey(entry.path, entry.changes), path: entry.path, changes: entry.changes, preview: false })}>
              {messages.changes.closeTab}
            </MenuItem>
            <MenuItem disabled={tabs.length === 0} onClick={onCloseAll}>
              {messages.changes.closeAllTabs}
            </MenuItem>
            <MenuSeparator />
            <MenuItem disabled={!entry} onClick={() => entry && onFileHistory(entry.path)}>
              {messages.fileHistory.open}
            </MenuItem>
            <MenuItem disabled={!entry || entry.unmerged} onClick={onExternalDiff}>
              {messages.externalTools.openDiff}
            </MenuItem>
            <MenuSeparator />
            <MenuItem onClick={onRefresh}>{messages.changes.refresh}</MenuItem>
          </Popover>
        </div>
      </div>
    </div>
  );
}

/** 路径导航行（页签下方）：目录与文件名，右侧显示视图与改动处数。 */
export function Breadcrumbs({
  entry,
  view,
  changeCount,
}: {
  entry: ChangeEntry | null;
  view: ViewMode;
  changeCount: number;
}) {
  if (!entry) return null;
  const segments = entry.path.split("/");
  return (
    <div className="flex h-[22px] shrink-0 items-center gap-1 overflow-hidden px-3.5 text-[12px] text-[var(--breadcrumb-foreground)]">
      <FileTypeIcon path={entry.path} symlink={entry.readonlyReason === "symlink"} className="size-3.5" />
      {segments.map((segment, index) => (
        <span key={`${index}-${segment}`} className="flex min-w-0 items-center gap-1">
          {index > 0 ? <Codicon name="chevron-right" size={12} /> : null}
          <span className={index === segments.length - 1 ? "truncate text-foreground" : "truncate"}>{segment}</span>
        </span>
      ))}
      <span className="ml-auto shrink-0 whitespace-nowrap pl-2">
        {`${view === "split" ? messages.diff.split : messages.diff.unified} · ${messages.changes.hunkCount(changeCount)}`}
      </span>
    </div>
  );
}
