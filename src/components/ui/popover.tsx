/**
 * 轻量弹出层（M4；返修 Comments 3 换成 Base UI Popover）。
 *
 * 用 Base UI 的 Portal + Positioner：弹层挂在顶层，空间不够时自动翻转
 * （flip）到锚点另一侧并贴边平移（shift），窗口任何位置都完整可见——
 * 提交选项弹层曾被窗口底部裁掉（返修 B2）。锚点由 `trigger` 属性提供：
 * Popover.Trigger 用 render 接管调用方的按钮，保留原有样式与受控逻辑。
 */

import { type ReactNode, type ReactElement } from "react";
import { Popover as PopoverParts } from "@base-ui/react/popover";

export function Popover({
  open,
  onOpen,
  onClose,
  trigger,
  children,
  align = "left",
  width,
}: {
  open: boolean;
  onOpen: () => void;
  onClose: () => void;
  /** 锚点元素（通常是触发按钮）；Base UI 接管它的 aria/点击，样式原样保留。 */
  trigger: ReactElement;
  children: ReactNode;
  align?: "left" | "right";
  width?: number;
}) {
  return (
    <PopoverParts.Root open={open} onOpenChange={(next) => { if (next) onOpen(); else onClose(); }}>
      <PopoverParts.Trigger render={trigger} nativeButton />
      <PopoverParts.Portal className="revia-popover-host">
        <PopoverParts.Positioner className="pointer-events-auto" align={align === "right" ? "end" : "start"} side="bottom" sideOffset={4}>
          <PopoverParts.Popup
            className="revia-popper overflow-y-auto p-1 text-[12.5px]"
            style={width ? { width } : { minWidth: 240 }}
            role="menu"
          >
            {children}
          </PopoverParts.Popup>
        </PopoverParts.Positioner>
      </PopoverParts.Portal>
    </PopoverParts.Root>
  );
}

export function MenuItem({
  children,
  selected,
  disabled,
  onClick,
  title,
}: {
  children: ReactNode;
  selected?: boolean;
  disabled?: boolean;
  onClick?: () => void;
  title?: string;
}) {
  return (
    <button
      type="button"
      role="menuitem"
      disabled={disabled}
      title={title}
      onClick={onClick}
      className="flex w-full items-center gap-2 rounded-[3px] px-2 py-[5px] text-left whitespace-nowrap outline-none transition-colors duration-100 hover:bg-toolbar-hoverbackground focus-visible:bg-toolbar-hoverbackground disabled:opacity-50"
    >
      <span className="flex w-4 shrink-0 justify-center">
        {selected ? <span className="codicon codicon-check" aria-hidden /> : null}
      </span>
      <span className="min-w-0 flex-1">{children}</span>
    </button>
  );
}

export function MenuSeparator() {
  return <div className="my-1 h-px bg-panel-border" role="separator" />;
}
