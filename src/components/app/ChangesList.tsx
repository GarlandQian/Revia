/**
 * 文件列表：未暂存 / 未跟踪（默认收起）/ 已暂存三组（Q8、Q30、Q31）。
 *
 * 外观照 Rebased 的“变更”工具窗口（Q107）：分组标题写成“未暂存 3 个文件”，
 * 文件行带文件类型图标。文件名保持正常颜色，右侧用带颜色的单字母表示状态；
 * 同一文件同时出现在两组时是两行，选中哪一行就显示哪一侧的 diff。
 */

import { useEffect, useMemo, useRef, useState } from "react";
import { Check, ChevronDown, ChevronRight, Lock, Minus, Plus } from "lucide-react";
import type { ChangeEntry } from "@/types/bindings/ChangeEntry";
import type { ChangesKind } from "@/types/bindings/ChangesKind";
import type { ChangesSnapshot } from "@/types/bindings/ChangesSnapshot";
import { messages } from "@/lib/i18n/zh-CN";
import { FileTypeIcon } from "./FileTypeIcon";

const ROW_HEIGHT = 24;

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
  /** 切换审阅标记：只切换，不改变当前选择（Q37）。 */
  onToggleReview?: (entry: ChangeEntry, marked: boolean) => void;
}

export function ChangesList({ snapshot, selected, onSelect, onStage, onUnstage, isWriting, onToggleReview }: ChangesListProps) {
  // 未跟踪一组默认收起，只显示数量（Q30）；选中项进入某组时自动展开该组。
  const [open, setOpen] = useState<Record<ChangesKind, boolean>>({ unstaged: true, untracked: false, staged: true });
  const prevSelectedRef = useRef<string | null>(null);

  useEffect(() => {
    const key = selected ? `${selected.changes}:${selected.path}` : null;
    if (key !== prevSelectedRef.current) {
      prevSelectedRef.current = key;
      if (selected) {
        setOpen((current) => (current[selected.changes] ? current : { ...current, [selected.changes]: true }));
      }
    }
  }, [selected]);

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

  const groupProps = { selected, onSelect, onStage, onUnstage, isWriting, onToggleReview };
  const toggle = (kind: ChangesKind) => setOpen((current) => ({ ...current, [kind]: !current[kind] }));

  return (
    <div className="change-list min-h-0 flex-1 overflow-auto pt-0.5 pb-2 outline-none" tabIndex={0} onKeyDown={onKeyDown}>
      <Group title={messages.changes.unstaged} entries={groups.unstaged} open={open.unstaged} onToggle={() => toggle("unstaged")} {...groupProps} />
      <Group title={messages.changes.untracked} entries={groups.untracked} open={open.untracked} onToggle={() => toggle("untracked")} {...groupProps} />
      <Group title={messages.changes.staged} entries={groups.staged} open={open.staged} onToggle={() => toggle("staged")} {...groupProps} />
      {snapshot.entries.length === 0 ? (
        <div className="px-4 py-3 text-fg-2">{messages.changes.empty}</div>
      ) : null}
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
  onToggleReview,
  open,
  onToggle,
}: {
  title: string;
  entries: ChangeEntry[];
  selected: Selection | null;
  onSelect: (selection: Selection) => void;
  onStage?: (entry: ChangeEntry) => void;
  onUnstage?: (entry: ChangeEntry) => void;
  isWriting?: boolean;
  onToggleReview?: (entry: ChangeEntry, marked: boolean) => void;
  open: boolean;
  onToggle: () => void;
}) {
  if (entries.length === 0) return null;
  const Chevron = open ? ChevronDown : ChevronRight;
  return (
    <section>
      <button
        type="button"
        className="flex h-6 w-full items-center gap-1 px-2 text-left whitespace-nowrap outline-none focus-visible:bg-hover"
        onClick={onToggle}
        aria-expanded={open}
        aria-label={`${title}：${open ? messages.changes.collapse : messages.changes.expand}`}
      >
        <Chevron aria-hidden className="size-3 shrink-0 text-icon" strokeWidth={2} />
        <span className="font-semibold">{title}</span>
        <span className="ml-1 text-fg-2">{messages.changes.fileCount(entries.length)}</span>
      </button>
      {open ? (
        <VirtualRows
          entries={entries}
          selected={selected}
          onSelect={onSelect}
          onStage={onStage}
          onUnstage={onUnstage}
          isWriting={isWriting}
          onToggleReview={onToggleReview}
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
  onToggleReview,
}: {
  entries: ChangeEntry[];
  selected: Selection | null;
  onSelect: (selection: Selection) => void;
  onStage?: (entry: ChangeEntry) => void;
  onUnstage?: (entry: ChangeEntry) => void;
  isWriting?: boolean;
  onToggleReview?: (entry: ChangeEntry, marked: boolean) => void;
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
          const slash = entry.path.lastIndexOf("/");
          const name = entry.path.slice(slash + 1);
          const dir = slash > 0 ? entry.path.slice(0, slash) : "";
          const detail = entry.originalPath ? messages.changes.renamedFrom(dir, entry.originalPath) : dir;
          const reviewLabel = entry.reviewed
            ? messages.review.unmark
            : entry.worktreeVersionConfirmed
              ? messages.review.mark
              : messages.review.unconfirmed;
          return (
            <div
              key={`${entry.changes}-${entry.path}`}
              data-selected={isSelected}
              className="change-row group absolute left-0 right-0 flex items-center gap-1.5 pr-2.5 pl-[26px]"
              style={{ top: index * ROW_HEIGHT, height: ROW_HEIGHT }}
            >
              <button
                type="button"
                className="flex min-w-0 flex-1 items-center gap-1.5 overflow-hidden text-left outline-none cursor-pointer focus-visible:rounded-sm focus-visible:ring-1 focus-visible:ring-ring focus-visible:ring-inset"
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
                <FileTypeIcon path={entry.path} symlink={entry.readonlyReason === "symlink"} />
                <span className="min-w-0 shrink truncate">{name}</span>
                <span className="min-w-0 flex-1 truncate text-xs text-fg-2">{detail}</span>
                {entry.collapsed ? (
                  <span className="shrink-0 rounded px-1.5 text-[11px] leading-[18px] text-fg-2 ring-1 ring-edge ring-inset">
                    {messages.changes.collapsed}
                  </span>
                ) : null}
              </button>
              {readonlyDesc ? (
                <span className="shrink-0 text-fg-2" title={readonlyDesc} aria-label={messages.changes.readonly}>
                  <Lock aria-hidden className="size-3.5" strokeWidth={1.6} />
                </span>
              ) : (
                <button
                  type="button"
                  tabIndex={-1}
                  disabled={isWriting}
                  className="grid size-[18px] shrink-0 place-items-center rounded bg-hover text-icon opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 disabled:text-fg-3"
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
                  {entry.changes === "staged" ? <Minus className="size-3" strokeWidth={2} /> : <Plus className="size-3" strokeWidth={2} />}
                </button>
              )}
              <span className={`status-${entry.status} w-3 shrink-0 text-center text-xs font-semibold`}>
                {statusLetter(entry)}
              </span>
              {onToggleReview ? (
                <button
                  type="button"
                  role="checkbox"
                  aria-checked={entry.reviewed}
                  aria-label={reviewLabel}
                  title={reviewLabel}
                  // 版本无法确认时不能新增认领（SP07-R4-1）；已有标记仍可取消。
                  disabled={!entry.reviewed && !entry.worktreeVersionConfirmed}
                  tabIndex={-1}
                  className="review-check"
                  onClick={(e) => {
                    // 只切换标记，不改变当前选择，也不触发暂存（Q37）。
                    e.stopPropagation();
                    onToggleReview(entry, !entry.reviewed);
                  }}
                >
                  <Check aria-hidden className="size-2.5" strokeWidth={3} />
                </button>
              ) : null}
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
