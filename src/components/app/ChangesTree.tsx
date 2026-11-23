/**
 * 改动树（M4 F03、M12 Q227–Q231、M13 Q237/Q243）。
 *
 * - 26px 平衡行高，中性清晰文件名（删除文件带淡删除线）、淡灰目录路径、已审阅小对勾；
 * - 行尾右侧显示等宽语义单字母状态徽标（M / A / D / R / ? / !），悬停平滑切换为行内快捷操作；
 * - 完整保留 Shift 连续选择、⌘/Ctrl 增减选择、折叠目录选中计数与空格暂存语义。
 */

import { useLayoutEffect, useRef, useState } from "react";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { FileStatus } from "@/types/bindings/FileStatus";
import { entryKey, type TreeGroupKey, type TreeGroupOpen, type TreeItem } from "@/hooks/changesTreeState";
import { messages } from "@/lib/i18n/index";
import { readonlyReasonText, renamedFrom } from "@/lib/i18n/text";
import { isAltModifier, isPrimaryClick, isPrimaryModifier } from "@/lib/platform";
import { matchShortcut } from "@/lib/shortcuts";
import { AppIcon } from "@/components/ui/app-icon";
import { FileTypeIcon } from "./FileTypeIcon";

export const TREE_ROW_HEIGHT = 26;
const OVERSCAN = 8;

const STATUS_LETTER: Record<FileStatus, string> = {
  modified: "M",
  added: "A",
  deleted: "D",
  renamed: "R",
  untracked: "?",
  conflict: "!",
  typeChanged: "T",
};

const groupTitle: Record<TreeGroupKey, () => string> = {
  conflicts: () => messages.merge.conflictsGroup,
  staged: () => messages.changes.staged,
  unstaged: () => messages.changes.unstaged,
  untracked: () => messages.changes.untracked,
};

interface ChangesTreeProps {
  items: TreeItem[];
  directoryCounts: Map<string, number>;
  active: ChangeEntry | null;
  selectedKeys: Set<string>;
  open: TreeGroupOpen;
  dirOpen: Set<string>;
  isWriting: boolean;
  onActivate: (entry: ChangeEntry, modifiers: { toggle: boolean; extend: boolean }) => void;
  onPin: (entry: ChangeEntry) => void;
  onMove: (direction: 1 | -1, extend: boolean) => void;
  onSelectAll: () => void;
  onStageSelection: () => void;
  onToggleGroup: (kind: TreeGroupKey) => void;
  onResolveConflicts: () => void;
  onToggleDir: (dir: string) => void;
  onStageEntries: (entries: ChangeEntry[]) => void;
  onUnstageEntries: (entries: ChangeEntry[]) => void;
  onStageEntry: (entry: ChangeEntry) => void;
  onUnstageEntry: (entry: ChangeEntry) => void;
  onDiscardEntry: (entry: ChangeEntry) => void;
}

export function ChangesTree(props: ChangesTreeProps) {
  const {
    items,
    directoryCounts,
    active,
    selectedKeys,
    open,
    dirOpen,
    isWriting,
    onActivate,
    onPin,
    onMove,
    onSelectAll,
    onStageSelection,
    onToggleGroup,
    onResolveConflicts,
    onToggleDir,
    onStageEntries,
    onUnstageEntries,
    onStageEntry,
    onUnstageEntry,
    onDiscardEntry,
  } = props;
  const scrollerRef = useRef<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const [viewport, setViewport] = useState(0);

  const hasItems = items.length > 0;

  useLayoutEffect(() => {
    const element = scrollerRef.current;
    if (!element) return;
    const measure = () => setViewport(element.clientHeight);
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    measure();
    return () => observer.disconnect();
  }, [hasItems]);

  const first = Math.max(0, Math.floor(scrollTop / TREE_ROW_HEIGHT) - OVERSCAN);
  const last = Math.min(items.length, Math.ceil((scrollTop + Math.max(viewport, TREE_ROW_HEIGHT)) / TREE_ROW_HEIGHT) + OVERSCAN);

  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement | null;
    if (target && target.tagName === "INPUT") return;
    const onButton = target?.closest("button") != null;
    if (onButton && (event.key === " " || event.key === "Enter")) return;
    if (isPrimaryModifier(event) && !isAltModifier(event) && event.code === "KeyA") {
      event.preventDefault();
      onSelectAll();
      return;
    }
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      onMove(event.key === "ArrowDown" ? 1 : -1, event.shiftKey);
      return;
    }
    if (event.key === "Enter" && active) {
      event.preventDefault();
      onPin(active);
      return;
    }
    if (event.key === " " && !event.altKey && !event.ctrlKey && !event.metaKey && !event.shiftKey && !isWriting) {
      if (matchShortcut(event.nativeEvent)) return;
      event.preventDefault();
      event.stopPropagation();
      onStageSelection();
    }
  };

  if (items.length === 0) {
    return <div className="px-3.5 py-4 text-[12.5px] text-[var(--text-muted)]">{messages.changes.empty}</div>;
  }

  return (
    <div
      ref={scrollerRef}
      className="changes-tree min-h-0 flex-1 overflow-y-auto overflow-x-hidden py-1.5 outline-none"
      tabIndex={0}
      onKeyDown={onKeyDown}
      onScroll={(event) => {
        setScrollTop(event.currentTarget.scrollTop);
        const height = event.currentTarget.clientHeight;
        if (typeof height === "number" && height > 0 && height !== viewport) {
          setViewport(height);
        }
      }}
      role="tree"
      aria-label={messages.changes.toolWindow}
      aria-multiselectable
    >
      <div style={{ height: items.length * TREE_ROW_HEIGHT, position: "relative" }}>
        {items.slice(first, last).map((item, offset) => {
          const index = first + offset;
          if (item.type === "header") {
            return (
              <GroupHeader
                key={`header-${item.kind}`}
                kind={item.kind}
                title={groupTitle[item.kind]()}
                entries={item.entries}
                open={open[item.kind]}
                top={index * TREE_ROW_HEIGHT}
                isWriting={isWriting}
                onToggle={() => onToggleGroup(item.kind)}
                onStageEntries={onStageEntries}
                onUnstageEntries={onUnstageEntries}
                onResolveConflicts={onResolveConflicts}
              />
            );
          }
          if (item.type === "dir") {
            const open_ = dirOpen.has(item.dir);
            const count = directoryCounts.get(`${item.kind}:${item.dir}`) ?? 0;
            return (
              <div
                key={`dir-${item.kind}-${item.dir}`}
                className="section-header group absolute left-1.5 right-1.5"
                style={{ top: index * TREE_ROW_HEIGHT, height: TREE_ROW_HEIGHT, paddingLeft: 14 }}
              >
                <button
                  type="button"
                  className="flex min-w-0 flex-1 items-center gap-1 text-left outline-none"
                  onClick={() => onToggleDir(item.dir)}
                  aria-expanded={open_}
                >
                  <AppIcon name={open_ ? "chevron-down" : "chevron-right"} size={12} />
                  <AppIcon name={open_ ? "folder-opened" : "folder"} size={13} className="text-[var(--text-muted)]" />
                  <span className="truncate text-[12px] font-normal text-[var(--text-secondary)]">{item.dir}</span>
                  {!open_ && count > 0 ? <span className="count-pill ml-1 shrink-0">{messages.changes.selectedCount(count)}</span> : null}
                </button>
              </div>
            );
          }
          const entry = item.entry;
          const key = entryKey(entry);
          return (
            <ChangeRow
              key={`row-${key}`}
              entry={entry}
              top={index * TREE_ROW_HEIGHT}
              selected={selectedKeys.has(key)}
              active={active?.path === entry.path && active?.changes === entry.changes}
              isWriting={isWriting}
              onActivate={onActivate}
              onPin={onPin}
              onStage={onStageEntry}
              onUnstage={onUnstageEntry}
              onDiscard={onDiscardEntry}
            />
          );
        })}
      </div>
    </div>
  );
}

function GroupHeader({
  kind,
  title,
  entries,
  open,
  top,
  isWriting,
  onToggle,
  onStageEntries,
  onUnstageEntries,
  onResolveConflicts,
}: {
  kind: TreeGroupKey;
  title: string;
  entries: ChangeEntry[];
  open: boolean;
  top: number;
  isWriting: boolean;
  onToggle: () => void;
  onStageEntries: (entries: ChangeEntry[]) => void;
  onUnstageEntries: (entries: ChangeEntry[]) => void;
  onResolveConflicts: () => void;
}) {
  const label = kind === "staged" ? messages.changes.unstageAll : messages.changes.stageAll;
  if (kind === "conflicts") {
    return (
      <div className="section-header group absolute left-1.5 right-1.5" style={{ top, height: TREE_ROW_HEIGHT }}>
        <button
          type="button"
          className="flex min-w-0 flex-1 items-center gap-1 text-left outline-none"
          onClick={onToggle}
          aria-expanded={open}
          aria-label={`${title}：${open ? messages.changes.collapse : messages.changes.expand}`}
        >
          <AppIcon name={open ? "chevron-down" : "chevron-right"} size={12} />
          <span className="truncate text-[var(--danger)]">{title}</span>
          <span className="count-pill ml-0.5">{entries.length}</span>
        </button>
        <button
          type="button"
          className="h-[20px] shrink-0 rounded-[4px] border border-[var(--border-subtle)] bg-[var(--surface-bg)] px-1.5 text-[11px] font-medium text-[var(--text-primary)] hover:bg-[var(--surface-hover)]"
          title={messages.merge.resolveButton}
          aria-label={messages.merge.resolveButton}
          disabled={isWriting}
          onClick={onResolveConflicts}
        >
          {messages.merge.resolveButton}
        </button>
      </div>
    );
  }
  return (
    <div className="section-header group absolute left-1.5 right-1.5" style={{ top, height: TREE_ROW_HEIGHT }}>
      <button
        type="button"
        className="flex min-w-0 flex-1 items-center gap-1 text-left outline-none"
        onClick={onToggle}
        aria-expanded={open}
        aria-label={`${title}：${open ? messages.changes.collapse : messages.changes.expand}`}
      >
        <AppIcon name={open ? "chevron-down" : "chevron-right"} size={12} />
        <span className="truncate">{title}</span>
        <span className="count-pill ml-0.5">{entries.length}</span>
      </button>
      <button
        type="button"
        className="mini-icon-btn opacity-70 group-hover:opacity-100 group-focus-within:opacity-100"
        title={label}
        aria-label={label}
        disabled={isWriting || entries.length === 0}
        onClick={() => (kind === "staged" ? onUnstageEntries(entries) : onStageEntries(entries))}
      >
        <AppIcon name={kind === "staged" ? "remove" : "add"} size={13} />
      </button>
    </div>
  );
}

function ChangeRow({
  entry,
  top,
  selected,
  active,
  isWriting,
  onActivate,
  onPin,
  onStage,
  onUnstage,
  onDiscard,
}: {
  entry: ChangeEntry;
  top: number;
  selected: boolean;
  active: boolean;
  isWriting: boolean;
  onActivate: (entry: ChangeEntry, modifiers: { toggle: boolean; extend: boolean }) => void;
  onPin: (entry: ChangeEntry) => void;
  onStage: (entry: ChangeEntry) => void;
  onUnstage: (entry: ChangeEntry) => void;
  onDiscard: (entry: ChangeEntry) => void;
}) {
  const slash = entry.path.lastIndexOf("/");
  const name = entry.path.slice(slash + 1);
  const dir = slash > 0 ? entry.path.slice(0, slash) : "";
  const detail = entry.originalPath
    ? renamedFrom(dir, entry.originalPath)
    : entry.collapsed
      ? `${dir ? `${dir} · ` : ""}${messages.changes.collapsed}`
      : dir;
  const readonlyDesc = entry.readonlyReason ? readonlyReasonText(entry.readonlyReason) : null;
  const showDiscard = (entry.changes === "unstaged" || entry.changes === "untracked") && !entry.readonlyReason;
  const letter = STATUS_LETTER[entry.status] ?? "M";

  return (
    <div
      className="change-row group absolute left-1.5 right-1.5 flex items-center gap-1.5 pl-4 pr-2"
      style={{ top, height: TREE_ROW_HEIGHT }}
      data-selected={selected}
      data-active={active}
      data-status={entry.status}
      role="treeitem"
      aria-selected={selected}
      aria-label={entry.path}
      onDoubleClick={() => onPin(entry)}
    >
      <button
        type="button"
        className="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden text-left outline-none"
        onClick={(event) =>
          onActivate(entry, { toggle: isPrimaryClick(event), extend: event.shiftKey })
        }
        title={readonlyDesc ? `${entry.path}（${readonlyDesc}）` : entry.originalPath ? `${entry.originalPath} → ${entry.path}` : entry.path}
      >
        <FileTypeIcon path={entry.path} symlink={entry.readonlyReason === "symlink"} />
        <span className="change-file-name min-w-0 shrink truncate">{name}</span>
        {detail ? <span className="min-w-0 flex-1 truncate text-[11.5px] text-[var(--text-muted)]">{detail}</span> : null}
      </button>

      {entry.reviewed ? (
        <span className="shrink-0 text-[var(--reviewed)]" title={messages.review.marked} aria-label={messages.review.marked}>
          <AppIcon name="check" size={13} />
        </span>
      ) : null}

      {entry.readonlyReason ? (
        <span className="shrink-0 text-[var(--text-muted)]" title={readonlyDesc ?? undefined} aria-label={messages.changes.readonly}>
          <AppIcon name="lock" size={13} />
        </span>
      ) : (
        <div className="change-tail">
          <span className={`change-status-badge status-${entry.status}`} aria-hidden>
            {letter}
          </span>
          <div className="change-hover-actions">
            <button
              type="button"
              tabIndex={-1}
              disabled={isWriting}
              className="mini-icon-btn"
              aria-label={entry.changes === "staged" ? messages.changes.unstage : messages.changes.stage}
              title={entry.changes === "staged" ? messages.changes.shortcuts.unstage : messages.changes.shortcuts.stage}
              onClick={(event) => {
                event.stopPropagation();
                if (entry.changes === "staged") onUnstage(entry);
                else onStage(entry);
              }}
            >
              <AppIcon name={entry.changes === "staged" ? "remove" : "add"} size={13} />
            </button>
            {showDiscard ? (
              <button
                type="button"
                tabIndex={-1}
                disabled={isWriting}
                className="mini-icon-btn danger"
                aria-label={messages.changes.discardShort}
                title={messages.changes.shortcuts.discardFile}
                onClick={(event) => {
                  event.stopPropagation();
                  onDiscard(entry);
                }}
              >
                <AppIcon name="discard" size={12} />
              </button>
            ) : null}
          </div>
        </div>
      )}
    </div>
  );
}
