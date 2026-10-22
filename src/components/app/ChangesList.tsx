/**
 * 文件列表：未暂存 / 未跟踪（默认收起）/ 已暂存三组（Q8、Q30、Q31）。
 *
 * 文件名保持正常颜色，右侧用带颜色的单字母表示状态；同一文件同时出现在两组时是两行，
 * 选中哪一行就显示哪一侧的 diff。
 */

import { useEffect, useMemo, useRef, useState } from "react";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesKind } from "@/types/bindings/ChangesKind";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";
import { messages } from "@/lib/i18n/zh-CN";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";

const ROW_HEIGHT = 26;

export interface Selection {
  path: string;
  changes: ChangesKind;
}

interface ChangesListProps {
  snapshot: ChangesSnapshot;
  selected: Selection | null;
  onSelect: (selection: Selection) => void;
  onStage?: (entry: ChangeEntry) => void;
  onUnstage?: (entry: ChangeEntry) => void;
  isWriting?: boolean;
}

export function ChangesList({ snapshot, selected, onSelect, onStage, onUnstage, isWriting }: ChangesListProps) {
  const [untrackedOpen, setUntrackedOpen] = useState(false);

  useEffect(() => {
    if (selected?.changes === "untracked" && !untrackedOpen) {
      setUntrackedOpen(true);
    }
  }, [selected?.changes, untrackedOpen]);

  const groups = useMemo(() => {
    const unstaged = snapshot.entries.filter((entry) => entry.changes === "unstaged");
    const staged = snapshot.entries.filter((entry) => entry.changes === "staged");
    const untracked = snapshot.entries.filter((entry) => entry.changes === "untracked");
    return { unstaged, staged, untracked };
  }, [snapshot.entries]);

  const onKeyDown = (event: React.KeyboardEvent<HTMLDivElement>) => {
    if (event.key === " " && selected && !isWriting) {
      const target = event.target as HTMLElement | null;
      if (target && target.tagName === "BUTTON") {
        return; // 按钮由自身的键盘行为处理，不穿透
      }
      const targetEntry = snapshot.entries.find(
        (e) => e.path === selected.path && e.changes === selected.changes,
      );
      if (targetEntry && !targetEntry.readonlyReason) {
        event.preventDefault();
        if (targetEntry.changes === "staged") {
          onUnstage?.(targetEntry);
        } else {
          onStage?.(targetEntry);
        }
      }
    }
  };

  return (
    <div className="flex h-full flex-col outline-none" tabIndex={0} onKeyDown={onKeyDown}>
      <div className="flex items-center gap-2 border-b px-3 py-1.5 text-xs text-muted-foreground">
        <span className="truncate" title={snapshot.worktreePath}>
          {snapshot.branch ?? "（detached HEAD）"}
        </span>
        <span className="ml-auto tabular-nums">{snapshot.entries.length}</span>
      </div>
      <div className="min-h-0 flex-1 overflow-auto">
        <Group
          title={messages.changes.unstaged}
          entries={groups.unstaged}
          selected={selected}
          onSelect={onSelect}
          onStage={onStage}
          onUnstage={onUnstage}
          isWriting={isWriting}
        />
        <Group
          title={messages.changes.untracked}
          entries={groups.untracked}
          selected={selected}
          onSelect={onSelect}
          onStage={onStage}
          onUnstage={onUnstage}
          isWriting={isWriting}
          collapsible
          open={untrackedOpen}
          onToggle={() => setUntrackedOpen((value) => !value)}
        />
        <Group
          title={messages.changes.staged}
          entries={groups.staged}
          selected={selected}
          onSelect={onSelect}
          onStage={onStage}
          onUnstage={onUnstage}
          isWriting={isWriting}
        />
        {snapshot.entries.length === 0 ? (
          <div className="p-4 text-xs text-muted-foreground">{messages.changes.empty}</div>
        ) : null}
      </div>
    </div>
  );
}

function Group({
  title,
  entries,
  selected,
  onSelect,
  onStage,
  onUnstage,
  isWriting,
  collapsible,
  open = true,
  onToggle,
}: {
  title: string;
  entries: ChangeEntry[];
  selected: Selection | null;
  onSelect: (selection: Selection) => void;
  onStage?: (entry: ChangeEntry) => void;
  onUnstage?: (entry: ChangeEntry) => void;
  isWriting?: boolean;
  collapsible?: boolean;
  open?: boolean;
  onToggle?: () => void;
}) {
  if (entries.length === 0) return null;
  const expanded = collapsible ? open : true;
  return (
    <section>
      <header className="flex items-center gap-1 px-2 py-1 text-xs font-medium text-muted-foreground">
        {collapsible ? (
          <Button
            variant="ghost"
            size="sm"
            className="h-5 px-1 text-xs"
            onClick={onToggle}
            aria-label={expanded ? messages.changes.collapse : messages.changes.expand}
          >
            {expanded ? "▾" : "▸"}
          </Button>
        ) : (
          <span className="w-5" />
        )}
        <span>{title}</span>
        <Badge variant="secondary" className="ml-1 h-4 px-1 text-[10px] tabular-nums">
          {entries.length}
        </Badge>
      </header>
      {expanded ? (
        <VirtualRows
          entries={entries}
          selected={selected}
          onSelect={onSelect}
          onStage={onStage}
          onUnstage={onUnstage}
          isWriting={isWriting}
        />
      ) : null}
    </section>
  );
}

/** 固定行高的窗口化列表：改动文件数量可能上千，只渲染可见部分。 */
function VirtualRows({
  entries,
  selected,
  onSelect,
  onStage,
  onUnstage,
  isWriting,
}: {
  entries: ChangeEntry[];
  selected: Selection | null;
  onSelect: (selection: Selection) => void;
  onStage?: (entry: ChangeEntry) => void;
  onUnstage?: (entry: ChangeEntry) => void;
  isWriting?: boolean;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const [scrollTop, setScrollTop] = useState(0);
  const height = containerRef.current?.clientHeight ?? 400;
  const overscan = 8;
  const first = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - overscan);
  const last = Math.min(entries.length, Math.ceil((scrollTop + Math.max(height, ROW_HEIGHT)) / ROW_HEIGHT) + overscan);

  return (
    <div
      ref={containerRef}
      className="overflow-auto"
      style={{ maxHeight: Math.min(entries.length, 14) * ROW_HEIGHT }}
      onScroll={(event) => setScrollTop(event.currentTarget.scrollTop)}
    >
      <div style={{ height: entries.length * ROW_HEIGHT, position: "relative" }}>
        {entries.slice(first, last).map((entry, offset) => {
          const index = first + offset;
          const isSelected = selected?.path === entry.path && selected?.changes === entry.changes;
          const readonlyDesc = entry.readonlyReason
            ? messages.changes.readonlyReasons[entry.readonlyReason]
            : null;
          return (
            <div
              key={`${entry.changes}-${entry.path}`}
              className={`group absolute left-0 right-0 flex items-center gap-1.5 px-2 text-left text-xs ${
                isSelected ? "bg-accent text-accent-foreground" : "hover:bg-accent/50"
              }`}
              style={{ top: index * ROW_HEIGHT, height: ROW_HEIGHT }}
            >
              <button
                type="button"
                className="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden text-left outline-none cursor-pointer focus-visible:ring-1 focus-visible:ring-ring focus-visible:rounded-sm focus-visible:ring-inset"
                onClick={() => onSelect({ path: entry.path, changes: entry.changes })}
                onKeyDown={(e) => {
                  if (e.key === " " && !isWriting && !entry.readonlyReason) {
                    e.preventDefault();
                    e.stopPropagation();
                    if (entry.changes === "staged") {
                      onUnstage?.(entry);
                    } else {
                      onStage?.(entry);
                    }
                  }
                }}
                title={
                  readonlyDesc
                    ? `${entry.path}（${readonlyDesc}）`
                    : entry.originalPath
                      ? `${entry.originalPath} → ${entry.path}`
                      : entry.path
                }
              >
                <span className="truncate flex-1">{entry.path}</span>
                {entry.collapsed ? (
                  <span className="shrink-0 text-[10px] text-muted-foreground">
                    {messages.changes.collapsed}
                  </span>
                ) : null}
                <span className="shrink-0 tabular-nums text-[10px] text-muted-foreground">
                  +{entry.added} −{entry.deleted}
                </span>
                <span className={`status-${entry.status} w-3 shrink-0 text-center font-semibold`}>
                  {statusLetter(entry)}
                </span>
              </button>
              {readonlyDesc ? (
                <span
                  className="shrink-0 px-1 text-[10px] text-muted-foreground/60 select-none"
                  title={readonlyDesc}
                >
                  {messages.changes.readonly}
                </span>
              ) : (
                <Button
                  variant="ghost"
                  size="sm"
                  tabIndex={-1}
                  disabled={isWriting}
                  className="h-4.5 w-4.5 p-0 shrink-0 text-[11px] font-bold opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 hover:bg-muted-foreground/20"
                  onClick={(e) => {
                    e.stopPropagation();
                    if (entry.changes === "staged") {
                      onUnstage?.(entry);
                    } else {
                      onStage?.(entry);
                    }
                  }}
                  aria-label={entry.changes === "staged" ? messages.changes.unstage : messages.changes.stage}
                  title={
                    entry.changes === "staged"
                      ? messages.changes.shortcuts.unstage
                      : messages.changes.shortcuts.stage
                  }
                >
                  {entry.changes === "staged" ? "−" : "+"}
                </Button>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function statusLetter(entry: ChangeEntry): string {
  if (entry.status === "untracked") return "?";
  if (entry.status === "conflict") return "U";
  if (entry.status === "added") return "A";
  if (entry.status === "deleted") return "D";
  if (entry.status === "renamed") return "R";
  if (entry.status === "typeChanged") return "T";
  return "M";
}
