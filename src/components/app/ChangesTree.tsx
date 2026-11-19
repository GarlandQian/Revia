/**
 * 改动树（M4 F03、Q8、Q30、Q120、Q121、Q128）。
 *
 * 三组依次是已暂存、未暂存、未跟踪；未跟踪默认收起。同一文件可以同时出现在两组。
 * 每行是文件类型图标、按状态上色的文件名、灰色的所在目录；已审阅文件在行尾显示小图标。
 * 支持 Shift 连续选择、⌘ 增减选择、⇧↑ / ⇧↓ 扩展与 ⌘A 全选。
 */

import { useLayoutEffect, useRef, useState } from "react";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import { entryKey, type TreeGroupKey, type TreeGroupOpen, type TreeItem } from "@/hooks/changesTreeState";
import { messages } from "@/lib/i18n/index";
import { readonlyReasonText, renamedFrom } from "@/lib/i18n/text";
import { isAltModifier, isPrimaryClick, isPrimaryModifier } from "@/lib/platform";
import { matchShortcut } from "@/lib/shortcuts";
import { Codicon } from "@/components/ui/codicon";
import { FileTypeIcon } from "./FileTypeIcon";

export const TREE_ROW_HEIGHT = 22;
const OVERSCAN = 8;

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
  /** 冲突组标题右侧的“解决…”按钮（返修 F10）。 */
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
    items, directoryCounts,
    active,
    selectedKeys,
    open,
    dirOpen,
    isWriting,
    onActivate,
    onPin,
    onMove,
    onSelectAll, onStageSelection,
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

  useLayoutEffect(() => {
    const element = scrollerRef.current;
    if (!element) return;
    const observer = new ResizeObserver(() => setViewport(element.clientHeight));
    observer.observe(element);
    setViewport(element.clientHeight);
    return () => observer.disconnect();
  }, []);

  const first = Math.max(0, Math.floor(scrollTop / TREE_ROW_HEIGHT) - OVERSCAN);
  const last = Math.min(items.length, Math.ceil((scrollTop + Math.max(viewport, TREE_ROW_HEIGHT)) / TREE_ROW_HEIGHT) + OVERSCAN);

  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    const target = event.target as HTMLElement | null;
    if (target && target.tagName === "INPUT") return;
    // 按钮保留自己的空格 / 回车行为，不穿透成整文件操作（Q93）。
    const onButton = target?.closest("button") != null;
    if (onButton && (event.key === " " || event.key === "Enter")) return;
    // ⌥⌘A 是暂存文件的快捷键，不能当成 ⌘A 全选（返修 C4）。
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
    return <div className="px-3 py-3 text-[12.5px] text-descriptionforeground">{messages.changes.empty}</div>;
  }

  return (
    <div
      ref={scrollerRef}
      className="changes-tree min-h-0 flex-1 overflow-auto pb-2 outline-none"
      tabIndex={0}
      onKeyDown={onKeyDown}
      onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
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
                className="section-header group absolute left-0 right-0"
                style={{ top: index * TREE_ROW_HEIGHT, height: TREE_ROW_HEIGHT, paddingLeft: 16 }}
              >
                <button
                  type="button"
                  className="flex min-w-0 flex-1 items-center gap-0.5 text-left outline-none"
                  onClick={() => onToggleDir(item.dir)}
                  aria-expanded={open_}
                >
                  <Codicon name={open_ ? "chevron-down" : "chevron-right"} size={14} />
                  <Codicon name={open_ ? "folder-opened" : "folder"} size={14} />
                  <span className="truncate text-[12px] font-normal">{item.dir}</span>
                  {!open_ && count > 0 && <span className="count-badge ml-1 shrink-0">{messages.changes.selectedCount(count)}</span>}
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
  // 全部条目都交给批量入口：只读条目由后端逐个返回跳过原因，不再静默过滤（返修 C6）。
  const label = kind === "staged" ? messages.changes.unstageAll : messages.changes.stageAll;
  if (kind === "conflicts") {
    return (
      <div className="section-header group absolute left-0 right-0" style={{ top, height: TREE_ROW_HEIGHT }}>
        <button
          type="button"
          className="flex min-w-0 flex-1 items-center gap-0.5 text-left outline-none"
          onClick={onToggle}
          aria-expanded={open}
          aria-label={`${title}：${open ? messages.changes.collapse : messages.changes.expand}`}
        >
          <Codicon name={open ? "chevron-down" : "chevron-right"} size={16} />
          <span className="truncate">{title}</span>
        </button>
        <button
          type="button"
          className="h-[18px] shrink-0 rounded-[3px] px-1 text-[11px] text-descriptionforeground hover:bg-toolbar-hoverbackground hover:text-foreground"
          title={messages.merge.resolveButton}
          aria-label={messages.merge.resolveButton}
          disabled={isWriting}
          onClick={onResolveConflicts}
        >
          {messages.merge.resolveButton}
        </button>
        <span className="count-badge ml-1">{entries.length}</span>
      </div>
    );
  }
  return (
    <div className="section-header group absolute left-0 right-0" style={{ top, height: TREE_ROW_HEIGHT }}>
      <button
        type="button"
        className="flex min-w-0 flex-1 items-center gap-0.5 text-left outline-none"
        onClick={onToggle}
        aria-expanded={open}
        aria-label={`${title}：${open ? messages.changes.collapse : messages.changes.expand}`}
      >
        <Codicon name={open ? "chevron-down" : "chevron-right"} size={16} />
        <span className="truncate">{title}</span>
      </button>
      <button
        type="button"
        className="icon-action size-[18px] opacity-0 group-hover:opacity-100 group-focus-within:opacity-100"
        title={label}
        aria-label={label}
        disabled={isWriting || entries.length === 0}
        onClick={() => (kind === "staged" ? onUnstageEntries(entries) : onStageEntries(entries))}
      >
        <Codicon name={kind === "staged" ? "remove" : "add"} size={15} />
      </button>
      <span className="count-badge ml-1">{entries.length}</span>
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
  // 未跟踪文件只能整文件操作：行内提供丢弃（先存回收区再删除）与暂存（F03.4、Q120）。
  const showDiscard = (entry.changes === "unstaged" || entry.changes === "untracked") && !entry.readonlyReason;
  return (
    <div
      className="change-row group absolute left-0 right-0 flex items-center gap-1.5 pr-2 pl-6"
      style={{ top, height: TREE_ROW_HEIGHT }}
      data-selected={selected}
      data-active={active}
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
        <span className={`min-w-0 shrink truncate status-${entry.status}`}>{name}</span>
        <span className="min-w-0 flex-1 truncate text-[11.5px] text-descriptionforeground">{detail}</span>
      </button>
      {entry.readonlyReason ? (
        <span className="shrink-0 text-descriptionforeground" title={readonlyDesc ?? undefined} aria-label={messages.changes.readonly}>
          <Codicon name="lock" size={14} />
        </span>
      ) : (
        <span className="flex shrink-0 items-center gap-0.5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 group-data-[selected=true]:opacity-100">
          {showDiscard ? (
            <button
              type="button"
              tabIndex={-1}
              disabled={isWriting}
              className="icon-action size-[18px]"
              aria-label={messages.changes.discardShort}
              title={messages.changes.shortcuts.discardFile}
              onClick={(event) => {
                event.stopPropagation();
                // 已多选时行内按钮作用于全部选中项（返修 C7，F03.4）。
                onDiscard(entry);
              }}
            >
              <Codicon name="discard" size={14} />
            </button>
          ) : null}
          <button
            type="button"
            tabIndex={-1}
            disabled={isWriting}
            className="icon-action size-[18px]"
            aria-label={entry.changes === "staged" ? messages.changes.unstage : messages.changes.stage}
            title={entry.changes === "staged" ? messages.changes.shortcuts.unstage : messages.changes.shortcuts.stage}
            onClick={(event) => {
              event.stopPropagation();
              if (entry.changes === "staged") onUnstage(entry);
              else onStage(entry);
            }}
          >
            <Codicon name={entry.changes === "staged" ? "remove" : "add"} size={14} />
          </button>
        </span>
      )}
      {entry.reviewed ? (
        <span className="shrink-0 text-descriptionforeground" title={messages.review.marked} aria-label={messages.review.marked}>
          <Codicon name="pass" size={14} />
        </span>
      ) : null}
    </div>
  );
}
