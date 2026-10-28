/**
 * 改动树（M4 F03、Q8、Q30、Q120、Q121、Q128）。
 *
 * 三组依次是已暂存、未暂存、未跟踪；未跟踪默认收起。同一文件可以同时出现在两组。
 * 每行是文件类型图标、按状态上色的文件名、灰色的所在目录；已审阅文件在行尾显示小图标。
 * 支持 Shift 连续选择、⌘ 增减选择、⇧↑ / ⇧↓ 扩展与 ⌘A 全选。
 */

import { useLayoutEffect, useMemo, useRef, useState } from "react";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesKind } from "@/types/bindings/ChangesKind";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";
import { messages } from "@/lib/i18n/zh-CN";
import { isAltModifier, isPrimaryClick, isPrimaryModifier } from "@/lib/platform";
import { Codicon } from "@/components/ui/codicon";
import { FileTypeIcon } from "./FileTypeIcon";

export const TREE_ROW_HEIGHT = 22;
const OVERSCAN = 8;

/** 分组：冲突组在最上方（返修 F10，Q143），其余三组照旧。 */
export type TreeGroupKey = ChangesKind | "conflicts";
export type TreeGroupOpen = Record<TreeGroupKey, boolean>;

export function entryKey(entry: Pick<ChangeEntry, "path" | "changes">): string {
  return `${entry.changes}:${entry.path}`;
}

/** 改动树的展示顺序：已暂存 → 未暂存 → 未跟踪（M4 F03.2）。 */
export function treeOrder(snapshot: ChangesSnapshot): ChangeEntry[] {
  const conflicts = snapshot.entries.filter((entry) => entry.unmerged);
  const staged = snapshot.entries.filter((entry) => entry.changes === "staged" && !entry.unmerged);
  const unstaged = snapshot.entries.filter((entry) => entry.changes === "unstaged" && !entry.unmerged);
  const untracked = snapshot.entries.filter((entry) => entry.changes === "untracked");
  return [...conflicts, ...staged, ...unstaged, ...untracked];
}

const GROUPS: Array<{ kind: TreeGroupKey; title: string }> = [
  { kind: "conflicts", title: messages.merge.conflictsGroup },
  { kind: "staged", title: messages.changes.staged },
  { kind: "unstaged", title: messages.changes.unstaged },
  { kind: "untracked", title: messages.changes.untracked },
];

/** 条目属于哪个显示分组：冲突文件不再出现在已暂存、未暂存组里（Q143）。 */
function groupOf(entry: ChangeEntry): TreeGroupKey {
  return entry.unmerged ? "conflicts" : entry.changes;
}

interface ChangesTreeProps {
  snapshot: ChangesSnapshot;
  active: { path: string; changes: ChangesKind } | null;
  selectedKeys: Set<string>;
  open: TreeGroupOpen;
  /** 按目录分组（返修 A2，Q139）：false 是平铺。 */
  groupByDirectory: boolean;
  /** 已展开的目录（按目录名）；仅在 groupByDirectory 时使用。 */
  dirOpen: Set<string>;
  isWriting: boolean;
  onActivate: (entry: ChangeEntry, modifiers: { toggle: boolean; extend: boolean }) => void;
  onPin: (entry: ChangeEntry) => void;
  onMove: (direction: 1 | -1, extend: boolean) => void;
  onSelectAll: () => void;
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

type TreeItem =
  | { type: "header"; kind: TreeGroupKey; title: string; entries: ChangeEntry[] }
  | { type: "dir"; kind: TreeGroupKey; dir: string }
  | { type: "row"; entry: ChangeEntry };

/** 按目录把同组条目分层（返修 A2，Q139）：目录节点之间用 `/` 分隔，逐级展开。 */
function buildDirectoryItems(
  entries: ChangeEntry[],
  kind: TreeGroupKey,
  dirOpen: Set<string>,
  push: (item: TreeItem) => void,
): void {
  interface Node { children: Map<string, Node>; rows: ChangeEntry[] }
  const root: Node = { children: new Map(), rows: [] };
  for (const entry of entries) {
    const parts = entry.path.split("/");
    let node = root;
    for (let index = 0; index < parts.length - 1; index += 1) {
      let child = node.children.get(parts[index]);
      if (!child) {
        child = { children: new Map(), rows: [] };
        node.children.set(parts[index], child);
      }
      node = child;
    }
    node.rows.push(entry);
  }
  const walk = (node: Node, prefix: string, depth: number) => {
    const dirs = [...node.children.entries()].sort(([a], [b]) => a.localeCompare(b));
    for (const [name, child] of dirs) {
      const dir = prefix ? `${prefix}/${name}` : name;
      push({ type: "dir", kind, dir });
      if (dirOpen.has(dir)) walk(child, dir, depth + 1);
    }
    for (const entry of node.rows) push({ type: "row", entry });
  };
  walk(root, "", 0);
}

export function ChangesTree(props: ChangesTreeProps) {
  const {
    snapshot,
    active,
    selectedKeys,
    open,
    groupByDirectory,
    dirOpen,
    isWriting,
    onActivate,
    onPin,
    onMove,
    onSelectAll,
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

  const items = useMemo(() => {
    const list: TreeItem[] = [];
    for (const group of GROUPS) {
      const entries = snapshot.entries.filter((entry) => groupOf(entry) === group.kind);
      if (entries.length === 0) continue;
      list.push({ type: "header", kind: group.kind, title: group.title, entries });
      if (!open[group.kind]) continue;
      if (groupByDirectory) {
        buildDirectoryItems(entries, group.kind, dirOpen, (item) => list.push(item));
      } else {
        for (const entry of entries) list.push({ type: "row", entry });
      }
    }
    return list;
  }, [dirOpen, groupByDirectory, open, snapshot.entries]);

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

  const activeEntry = active
    ? snapshot.entries.find((entry) => entry.path === active.path && entry.changes === active.changes) ?? null
    : null;

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
    if (event.key === "Enter" && activeEntry) {
      event.preventDefault();
      onPin(activeEntry);
      return;
    }
    if (event.key === " " && activeEntry && !isWriting && !activeEntry.readonlyReason) {
      event.preventDefault();
      if (activeEntry.changes === "staged") onUnstageEntry(activeEntry);
      else onStageEntry(activeEntry);
    }
  };

  if (snapshot.entries.length === 0) {
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
                title={item.title}
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
                  aria-label={`${item.dir}：${open_ ? messages.changes.collapse : messages.changes.expand}`}
                >
                  <Codicon name={open_ ? "chevron-down" : "chevron-right"} size={14} />
                  <Codicon name={open_ ? "folder-opened" : "folder"} size={14} />
                  <span className="truncate text-[12px] font-normal">{item.dir}</span>
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
    ? messages.changes.renamedFrom(dir, entry.originalPath)
    : entry.collapsed
      ? `${dir ? `${dir} · ` : ""}${messages.changes.collapsed}`
      : dir;
  const readonlyDesc = entry.readonlyReason ? messages.changes.readonlyReasons[entry.readonlyReason] : null;
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
