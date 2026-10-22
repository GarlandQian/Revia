/**
 * 平台相关逻辑与快捷键修饰键集中管理（AGENTS.md 规则、Q55）。
 *
 * 避免平台差异与修饰键逻辑散落在各组件中。
 */

export const isMac =
  typeof navigator !== "undefined" &&
  (navigator.platform.toUpperCase().indexOf("MAC") >= 0 ||
    navigator.userAgent.toUpperCase().indexOf("MACINTOSH") >= 0);

/**
 * 判断主修饰键（macOS 为 Command/Meta，Windows/Linux 为 Control）。
 *
 * 严格互斥：在 macOS 上 Ctrl 键绝不冒充 ⌘（Command），避免在 Mac 上按 Ctrl 触发 ⌘ 快捷键（审查意见 6）。
 */
export function isPrimaryModifier(event: KeyboardEvent | React.KeyboardEvent): boolean {
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

/** 判断目标元素是否为输入控件或按钮，避免键盘事件穿透到后台文件操作（Q93）。 */
export function isInteractiveElement(target: EventTarget | null): boolean {
  if (!target || !(target instanceof HTMLElement)) return false;
  const tag = target.tagName.toLowerCase();
  return (
    tag === "input" ||
    tag === "textarea" ||
    tag === "select" ||
    tag === "button" ||
    target.isContentEditable ||
    target.closest("button") !== null ||
    target.closest("input") !== null
  );
}
