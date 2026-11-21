/**
 * 左侧工具侧栏外壳（M13 Q240、Q244、Q245）：
 * - 从 App.tsx 拆出独立布局壳，承载 提交 / Stash / 回收区 三段切换与右侧宽度拖拽条；
 * - 提交页签内部支持改动树与提交区之间的上下拖拽分隔高度。
 */

import { useRef, useState, type ReactNode } from "react";
import { messages } from "@/lib/i18n/index";
import { AppIcon } from "@/components/ui/app-icon";

export type SideTab = "commit" | "stash" | "recovery";

export function SidebarShell({
  visible,
  width,
  onWidthChange,
  onWidthCommit,
  sideTab,
  onSelectSideTab,
  changesCount,
  commitActions,
  commitTree,
  commitComposer,
  stashContent,
  recoveryContent,
}: {
  visible: boolean;
  width: number;
  onWidthChange: (width: number) => void;
  onWidthCommit: (width: number) => void;
  sideTab: SideTab;
  onSelectSideTab: (tab: SideTab) => void;
  changesCount: number;
  commitActions: ReactNode;
  commitTree: ReactNode;
  commitComposer: ReactNode;
  stashContent: ReactNode;
  recoveryContent: ReactNode;
}) {
  const dragRef = useRef<{
    pointerId: number;
    clientX: number;
    width: number;
    scale: number;
    maxWidth: number;
    latestWidth: number;
  } | null>(null);

  const [composerHeight, setComposerHeight] = useState<number | null>(null);
  const composerDragRef = useRef<{
    pointerId: number;
    startY: number;
    startHeight: number;
  } | null>(null);
  const composerWrapRef = useRef<HTMLDivElement | null>(null);

  if (!visible) return null;

  return (
    <aside className="changes-sidebar relative flex min-h-0 shrink-0 flex-col" style={{ width }}>
      <div
        className="sidebar-resizer"
        role="separator"
        aria-orientation="vertical"
        aria-label={messages.changes.resizeSidebar}
        onPointerDown={(event) => {
          const sidebar = event.currentTarget.parentElement;
          const shell = sidebar?.closest<HTMLElement>(".app-shell");
          if (!sidebar?.offsetWidth || !shell) return;
          const scale = sidebar.getBoundingClientRect().width / sidebar.offsetWidth;
          const editorMinWidth = Number.parseFloat(getComputedStyle(sidebar).getPropertyValue("--editor-min-width")) || 280;
          dragRef.current = {
            pointerId: event.pointerId,
            clientX: event.clientX,
            width: sidebar.offsetWidth,
            scale,
            maxWidth: Math.min(680, shell.clientWidth - editorMinWidth),
            latestWidth: sidebar.offsetWidth,
          };
          event.currentTarget.setPointerCapture(event.pointerId);
          event.currentTarget.dataset.dragging = "true";
          event.preventDefault();
        }}
        onPointerMove={(event) => {
          const drag = dragRef.current;
          if (!drag || drag.pointerId !== event.pointerId) return;
          drag.latestWidth = Math.round(
            Math.min(drag.maxWidth, Math.max(240, drag.width + (event.clientX - drag.clientX) / drag.scale)),
          );
          onWidthChange(drag.latestWidth);
        }}
        onPointerUp={(event) => {
          const drag = dragRef.current;
          if (!drag || drag.pointerId !== event.pointerId) return;
          dragRef.current = null;
          delete event.currentTarget.dataset.dragging;
          event.currentTarget.releasePointerCapture(event.pointerId);
          onWidthCommit(drag.latestWidth);
        }}
        onLostPointerCapture={(event) => {
          dragRef.current = null;
          delete event.currentTarget.dataset.dragging;
        }}
      />

      <div className="sidebar-header">
        <div className="side-seg" role="tablist">
          <button
            type="button"
            role="tab"
            className="side-tab"
            aria-selected={sideTab === "commit"}
            onClick={() => onSelectSideTab("commit")}
          >
            <span>{messages.commit.tab}</span>
            {changesCount > 0 ? <span className="count-pill">{changesCount > 99 ? "99+" : changesCount}</span> : null}
          </button>
          <button
            type="button"
            role="tab"
            className="side-tab"
            aria-selected={sideTab === "stash"}
            onClick={() => onSelectSideTab("stash")}
          >
            <span>{messages.stash.title}</span>
          </button>
          <button
            type="button"
            role="tab"
            className="side-tab"
            aria-selected={sideTab === "recovery"}
            onClick={() => onSelectSideTab("recovery")}
          >
            <span>{messages.recovery.title}</span>
          </button>
        </div>

        {sideTab === "commit" ? <div className="flex shrink-0 items-center gap-0.5">{commitActions}</div> : null}
      </div>

      {sideTab === "commit" ? (
        <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
          <div className="flex min-h-0 flex-1 flex-col overflow-hidden">{commitTree}</div>
          <div
            className="commit-resizer"
            role="separator"
            aria-orientation="horizontal"
            onPointerDown={(event) => {
              const current = composerWrapRef.current?.getBoundingClientRect().height ?? 210;
              composerDragRef.current = { pointerId: event.pointerId, startY: event.clientY, startHeight: current };
              event.currentTarget.setPointerCapture(event.pointerId);
              event.currentTarget.dataset.dragging = "true";
              event.preventDefault();
            }}
            onPointerMove={(event) => {
              const drag = composerDragRef.current;
              if (!drag || drag.pointerId !== event.pointerId) return;
              const next = Math.round(Math.min(460, Math.max(170, drag.startHeight + (drag.startY - event.clientY))));
              setComposerHeight(next);
            }}
            onPointerUp={(event) => {
              const drag = composerDragRef.current;
              if (!drag || drag.pointerId !== event.pointerId) return;
              composerDragRef.current = null;
              delete event.currentTarget.dataset.dragging;
              event.currentTarget.releasePointerCapture(event.pointerId);
            }}
            onLostPointerCapture={(event) => {
              composerDragRef.current = null;
              delete event.currentTarget.dataset.dragging;
            }}
          />
          <div
            ref={composerWrapRef}
            className="flex shrink-0 flex-col overflow-hidden"
            style={composerHeight ? { height: composerHeight } : undefined}
          >
            {commitComposer}
          </div>
        </div>
      ) : sideTab === "stash" ? (
        stashContent
      ) : (
        recoveryContent
      )}
    </aside>
  );
}

/** 供侧栏内快捷工具按钮复用的轻量包装。 */
export function SidebarIconButton({
  icon,
  title,
  disabled,
  pressed,
  onClick,
}: {
  icon: string;
  title: string;
  disabled?: boolean;
  pressed?: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className="mini-icon-btn"
      aria-pressed={pressed}
      disabled={disabled}
      title={title}
      aria-label={title}
      onClick={onClick}
    >
      <AppIcon name={icon} size={13} />
    </button>
  );
}
