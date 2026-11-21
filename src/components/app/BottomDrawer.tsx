/**
 * 底部可拖拽收起抽屉（M13 Q241）：统一承载「内置终端」与「Git 命令日志」。
 */

import { useRef, type ReactNode } from "react";
import { messages, t } from "@/lib/i18n/index";
import { AppIcon } from "@/components/ui/app-icon";

export type BottomDrawerTab = "terminal" | "commandLog";

export function BottomDrawer({
  open,
  activeTab,
  height,
  canShowTerminal,
  terminalMounted = false,
  onSelectTab,
  onHeightChange,
  onClose,
  terminalContent,
  commandLogContent,
}: {
  open: boolean;
  activeTab: BottomDrawerTab;
  height: number;
  canShowTerminal: boolean;
  terminalMounted?: boolean;
  onSelectTab: (tab: BottomDrawerTab) => void;
  onHeightChange: (height: number) => void;
  onClose: () => void;
  terminalContent: ReactNode;
  commandLogContent: ReactNode;
}) {
  const dragRef = useRef<{ pointerId: number; startY: number; startHeight: number; scale: number } | null>(null);

  if (!open) return null;

  const effectiveTab: BottomDrawerTab = activeTab === "terminal" && !canShowTerminal ? "commandLog" : activeTab;

  return (
    <section
      className="bottom-drawer"
      style={{ height }}
      aria-label={effectiveTab === "terminal" ? t("terminal.title") : messages.commandLog.title}
    >
      <div
        className="drawer-resizer"
        role="separator"
        aria-orientation="horizontal"
        onPointerDown={(event) => {
          const section = event.currentTarget.parentElement;
          const offsetHeight = section?.offsetHeight || height;
          const scale = section && section.offsetHeight > 0 ? section.getBoundingClientRect().height / section.offsetHeight : 1;
          dragRef.current = { pointerId: event.pointerId, startY: event.clientY, startHeight: offsetHeight, scale };
          event.currentTarget.setPointerCapture(event.pointerId);
          event.currentTarget.dataset.dragging = "true";
          event.preventDefault();
        }}
        onPointerMove={(event) => {
          const drag = dragRef.current;
          if (!drag || drag.pointerId !== event.pointerId) return;
          const next = Math.round(Math.min(540, Math.max(140, drag.startHeight + (drag.startY - event.clientY) / drag.scale)));
          onHeightChange(next);
        }}
        onPointerUp={(event) => {
          const drag = dragRef.current;
          if (!drag || drag.pointerId !== event.pointerId) return;
          dragRef.current = null;
          delete event.currentTarget.dataset.dragging;
          event.currentTarget.releasePointerCapture(event.pointerId);
        }}
        onLostPointerCapture={(event) => {
          dragRef.current = null;
          delete event.currentTarget.dataset.dragging;
        }}
      />

      <div className="drawer-hdr">
        <div className="flex items-center gap-1" role="tablist">
          {canShowTerminal ? (
            <button
              type="button"
              role="tab"
              className="drawer-tab"
              aria-selected={effectiveTab === "terminal"}
              onClick={() => onSelectTab("terminal")}
            >
              <AppIcon name="terminal" size={13} />
              <span>{t("terminal.title")}</span>
            </button>
          ) : null}
          <button
            type="button"
            role="tab"
            className="drawer-tab"
            aria-selected={effectiveTab === "commandLog"}
            onClick={() => onSelectTab("commandLog")}
          >
            <AppIcon name="output" size={13} />
            <span>{messages.commandLog.title}</span>
          </button>
        </div>

        <button
          type="button"
          className="mini-icon-btn"
          title={messages.recovery.close}
          aria-label={messages.recovery.close}
          onClick={onClose}
        >
          <AppIcon name="close" size={13} />
        </button>
      </div>

      <div className="flex min-h-0 flex-1 flex-col overflow-hidden">
        {canShowTerminal && (terminalMounted || effectiveTab === "terminal") ? (
          <div className={effectiveTab === "terminal" ? "flex min-h-0 flex-1 flex-col overflow-hidden" : "hidden"}>
            {terminalContent}
          </div>
        ) : null}
        {effectiveTab === "commandLog" ? commandLogContent : null}
      </div>
    </section>
  );
}
