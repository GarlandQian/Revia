/**
 * 轻量弹出层（M4；返修 Comments 3 换成 Base UI Popover）。
 *
 * 用 Base UI 的 Portal + Positioner：弹层挂在顶层，空间不够时自动翻转
 * （flip）到锚点另一侧并贴边平移（shift），窗口任何位置都完整可见——
 * 提交选项弹层曾被窗口底部裁掉（返修 B2）。锚点由 `trigger` 属性提供：
 * Popover.Trigger 用 render 接管调用方的按钮，保留原有样式与受控逻辑。
 */

import { useId, useState, type ReactNode, type ReactElement } from "react";

import { Popover as PopoverParts } from "@base-ui/react/popover";
import { GlassLayer } from "@/features/m7/glass/GlassLayer";
import { glassPane } from "@/features/m7/glass/nativeGlass";

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
  /** 锚点元素（通常是触发按钮）；Base UI 接管它的 aria/点击，样式原样保留。 */
  trigger: ReactElement;
  children: ReactNode;
  align?: "left" | "right";
  /** 首选弹出方向；锚点贴近窗口底部（如侧栏最下面的按钮）时用 top，不要等到放不下才翻转。 */
  side?: "top" | "bottom";
  width?: number;
}) {
  // 系统材质下弹层是一块原生玻璃：每个弹层一块，按实例区分。
  const glassId = useId();
  return (
    <PopoverParts.Root
      open={open}
      onOpenChange={(next) => {
        // 打开与关闭都同步给调用方：触发器自己会切换内部状态，
        // 受控状态跟随它即可，不需要给触发按钮再加 onClick（会互相抵消）。
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
          // 离窗口边至少留出与卡片外边距相同的空隙，弹层不会贴着窗口边。
          collisionPadding={10}
        >
          <PopoverParts.Popup
            ref={glassPane({ id: `popover-${glassId}` })}
            className="revia-popper glass-popover glass-isolate overflow-y-auto p-1 text-[12.5px]"
            style={width ? { width } : { minWidth: 240 }}
            role="menu"
          >
            {/* 浮层材质（M7 任务 01）：第四版统一参数，玻璃层压在文字下面，不改弹层的尺寸与滚动。 */}
            <GlassLayer variant="popover" />
            {children}
          </PopoverParts.Popup>
        </PopoverParts.Positioner>
      </PopoverParts.Portal>
    </PopoverParts.Root>
  );
}

/**
 * 自带开关状态的弹出层：触发按钮的点击由这里接管（内部受控）。
 *
 * 用在筛选菜单这类“点开、选完再关”的场景，调用方不必自己维护 open 状态。
 */
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
  // 打开 / 关闭都交给 Base UI 的触发点击（`onOpen` / `onClose`），
  // 不再给按钮加 onClick：那会和触发器的原生切换互相抵消。
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
      className="menu-item flex w-full items-center gap-2 rounded-[3px] px-2 py-[5px] text-left whitespace-nowrap outline-none transition-colors duration-100 hover:bg-toolbar-hoverbackground focus-visible:bg-toolbar-hoverbackground disabled:opacity-50"
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
