/**
 * 平台相关逻辑与快捷键修饰键集中管理（AGENTS.md 规则、Q55）。
 *
 * 避免平台差异与修饰键逻辑散落在各组件中。
 */

export const isMac =
  typeof navigator !== "undefined" &&
  (navigator.platform.toUpperCase().indexOf("MAC") >= 0 ||
    navigator.userAgent.toUpperCase().indexOf("MACINTOSH") >= 0);

/** Windows 安装器会退出应用，更新入口据此要求“安装并重启”的明确确认（M8 R2）。 */
export const isWindows =
  typeof navigator !== "undefined" &&
  navigator.userAgent.toUpperCase().indexOf("WINDOWS") >= 0;

/**
 * 主工具栏左侧留白（像素）：macOS 的红绿灯覆盖在主工具栏左端（titleBarStyle: Overlay），
 * 其他平台用原生标题栏，主工具栏从窗口边缘开始（Q60）。
 */
export const MAIN_TOOLBAR_INSET = isMac ? 92 : 10;

/** 平台修饰键符号（返修 C11）：macOS 用 ⌘ / ⌥ / ⇧，其他平台用 Ctrl / Alt / Shift。 */
export const MOD_SYMBOLS = isMac
  ? { primary: "⌘", alt: "⌥", shift: "⇧", control: "⌃" }
  : { primary: "Ctrl+", alt: "Alt+", shift: "Shift+", control: "Ctrl+" };

/**
 * 判断主修饰键（macOS 为 Command/Meta，Windows/Linux 为 Control）。
 *
 * 严格互斥：在 macOS 上 Ctrl 键绝不冒充 ⌘（Command），避免在 Mac 上按 Ctrl 触发 ⌘ 快捷键（审查意见 6）。
 */
export function isPrimaryModifier(event: KeyboardEvent | React.KeyboardEvent): boolean {
  // AltGr 在浏览器里同时报告 Ctrl+Alt（Windows 输入字符用），不能当成快捷键。
  if (isAltGraph(event)) return false;
  if (isMac) {
    return event.metaKey && !event.ctrlKey;
  }
  return event.ctrlKey && !event.metaKey;
}

/**
 * 是否由 AltGr 产生。Windows 上 AltGr = Ctrl+Alt，输入 `@`、`€` 等字符时
 * 不应触发 Ctrl+Alt 业务快捷键（Q61、M8 任务 04）。
 */
export function isAltGraph(event: {
  getModifierState?: (key: "AltGraph") => boolean;
}): boolean {
  return typeof event.getModifierState === "function" && event.getModifierState("AltGraph");
}

/** 输入法组合期间不执行后台快捷键；keyCode 229 覆盖旧版 WebView2。 */
export function isImeComposing(event: {
  isComposing?: boolean;
  keyCode?: number;
}): boolean {
  return event.isComposing === true || event.keyCode === 229;
}

/** 判断鼠标点击是否带主修饰键（返修 C11：判断逻辑与键盘一致，集中在这里）。 */
export function isPrimaryClick(event: { metaKey: boolean; ctrlKey: boolean }): boolean {
  if (isMac) {
    return event.metaKey && !event.ctrlKey;
  }
  return event.ctrlKey && !event.metaKey;
}

/** 判断交替修饰键（Alt / Option）。 */
export function isAltModifier(event: KeyboardEvent | React.KeyboardEvent): boolean {
  return event.altKey;
}

/**
 * 判断是否触发了整文件暂存/取消暂存快捷键（⌥⌘A / Alt+Ctrl+A）。
 *
 * 使用 event.code === "KeyA" 而非 event.key，因为在 macOS 下按住 Option 时
 * event.key 输出的是特殊字符 "å"，会导致 toLowerCase() === "a" 无法匹配。
 */
export function isStageFileShortcut(event: KeyboardEvent | React.KeyboardEvent): boolean {
  return isPrimaryModifier(event) && isAltModifier(event) && event.code === "KeyA";
}

export function isDiscardFileShortcut(event: KeyboardEvent | React.KeyboardEvent): boolean {
  return isPrimaryModifier(event) && isAltModifier(event) && event.code === "KeyZ";
}

/**
 * 判断是否触发了「标记已审阅并跳到下一个」快捷键（⌥⌘R / Alt+Ctrl+R，Q37、Q94）。
 *
 * 同样用 `event.code`：macOS 按住 Option 时 `event.key` 会变成 "®"。
 * 调用方必须在 ⌘R 刷新分支**之前**处理它，否则会被刷新吞掉。
 */
export function isReviewShortcut(event: KeyboardEvent | React.KeyboardEvent): boolean {
  return isPrimaryModifier(event) && isAltModifier(event) && event.code === "KeyR";
}

/**
 * 判断焦点是否落在按钮上。
 *
 * 按钮有自己的键盘行为（空格/回车激活、勾选框切换）：全局快捷键不能穿透到后台文件操作，
 * 但也不能抢走按钮本身的默认行为（Q93、spec 决策 7）。
 */
export function isButtonTarget(target: EventTarget | null): boolean {
  if (!target || !(target instanceof HTMLElement)) return false;
  return target.closest("button, [role='checkbox'], [role='button']") !== null;
}

/** 输入、选择控件或弹窗保留自己的键盘行为，不触发后台操作（Q93）。 */
export function isTextInputOrDialog(target: EventTarget | null): boolean {
  if (!target || !(target instanceof HTMLElement)) return false;
  const tag = target.tagName.toLowerCase();
  const isInput =
    tag === "textarea" ||
    tag === "select" ||
    (tag === "input" && !["button", "checkbox", "radio", "submit", "reset"].includes((target as HTMLInputElement).type)) ||
    target.isContentEditable;
  const inDialog = target.closest("[role='dialog']") !== null || target.closest("dialog") !== null;
  return isInput || inDialog;
}

/**
 * 判断是否触发了「提交」快捷键（⌘K / Ctrl+K，M3 F06.5）。
 *
 * 只认主修饰键 + K；信息框内的 ⌘K 由信息框自己处理（那里允许直接提交），
 * 其它输入控件与弹窗保留自己的行为，不穿透（Q93）。
 */
export function isCommitShortcut(event: KeyboardEvent | React.KeyboardEvent): boolean {
  return isPrimaryModifier(event) && !event.altKey && !event.shiftKey && event.code === "KeyK";
}

/** 提交快捷键的展示文案（macOS 用 ⌘，其他平台用 Ctrl）。 */
export const COMMIT_SHORTCUT_LABEL = isMac ? "⌘K" : "Ctrl+K";

/** 判断焦点是否落在提交信息框里（那里允许 ⌘K 直接提交）。 */
export function isCommitMessageTarget(target: EventTarget | null): boolean {
  if (!target || !(target instanceof HTMLElement)) return false;
  return target.closest("[data-commit-message]") !== null;
}
