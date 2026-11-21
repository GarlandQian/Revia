/**
 * 轻量弹出层（Base UI Popover，Q51、M13 实色设计系统）。
 */

import { useState, type ReactNode, type ReactElement } from "react";
import { Popover as PopoverParts } from "@base-ui/react/popover";
import { AppIcon } from "@/components/ui/app-icon";

export function Popover({
  open,
  onOpen,
  onClose,
  trigger,
  children,
  align = "left",
  side = "bottom",
  width,
}: {
  open: boolean;
  onOpen: () => void;
  onClose: () => void;
  trigger: ReactElement;
  children: ReactNode;
  align?: "left" | "right";
  side?: "top" | "bottom";
  width?: number;
}) {
  return (
    <PopoverParts.Root
      open={open}
      onOpenChange={(next) => {
        if (next) onOpen();
        else onClose();
      }}
    >
      <PopoverParts.Trigger render={trigger} nativeButton />
      <PopoverParts.Portal className="revia-popover-host">
        <PopoverParts.Positioner
          className="pointer-events-auto"
          align={align === "right" ? "end" : "start"}
          side={side}
          sideOffset={4}
          collisionPadding={10}
        >
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

export function Dropdown({
  trigger,
  children,
  align = "left",
  width,
}: {
  trigger: ReactElement;
  children: ReactNode;
  align?: "left" | "right";
  width?: number;
}) {
  const [open, setOpen] = useState(false);
  return (
    <Popover
      open={open}
      onOpen={() => setOpen(true)}
      onClose={() => setOpen(false)}
      trigger={trigger}
      align={align}
      width={width}
    >
      {children}
    </Popover>
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
      className="menu-item flex w-full items-center gap-2 rounded-[6px] px-2 py-[5px] text-left whitespace-nowrap outline-none transition-colors duration-100 hover:bg-[var(--surface-hover)] focus-visible:bg-[var(--surface-hover)] disabled:opacity-50"
    >
      <span className="flex w-4 shrink-0 justify-center text-[var(--accent)]">
        {selected ? <AppIcon name="check" size={13} /> : null}
      </span>
      <span className="min-w-0 flex-1">{children}</span>
    </button>
  );
}

export function MenuSeparator() {
  return <div className="my-1 h-px bg-[var(--border-subtle)]" role="separator" />;
}
