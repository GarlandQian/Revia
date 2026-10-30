/**
 * 可自定义快捷键（M7 任务 02、Q199）。
 *
 * 加速键统一存成 `mod+alt+a` 这样的规范形式：`mod` 表示平台主修饰键
 * （macOS 为 ⌘，其他平台为 Ctrl），其余修饰键是 `ctrl` / `alt` / `shift` / `meta`。
 * 按键部分优先用 `event.code` 推导（macOS 按住 Option 时 `event.key` 会变成特殊字符），
 * 标点用名字（`equal`、`minus`、`backquote`…），避免 `+` 与分隔符混淆。
 *
 * 设置里只保存用户改过的动作；未列出的动作用默认加速键。
 */

import { isMac } from "./platform";
import { t } from "./i18n/index";

export type ShortcutId =
  | "commit"
  | "stageFile"
  | "discardFile"
  | "reviewNextFile"
  | "openRepo"
  | "refresh"
  | "nextFile"
  | "prevFile"
  | "nextHunk"
  | "prevHunk"
  | "fontUp"
  | "fontDown"
  | "zoomIn"
  | "zoomOut"
  | "zoomReset"
  | "toggleTerminal"
  | "cloneRepo"
  | "pullRequests";

/** 快捷键作用范围：`diff` 只在有文件选中时才有意义。 */
export type ShortcutScope = "global" | "diff";

export interface ShortcutDef {
  id: ShortcutId;
  scope: ShortcutScope;
  /** 默认加速键（规范形式）。 */
  defaultAccelerator: string;
  label: () => string;
}

export const shortcutDefs: readonly ShortcutDef[] = [
  { id: "commit", scope: "global", defaultAccelerator: "mod+k", label: () => t("shortcuts.actions.commit") },
  { id: "stageFile", scope: "diff", defaultAccelerator: "mod+alt+a", label: () => t("shortcuts.actions.stageFile") },
  { id: "discardFile", scope: "diff", defaultAccelerator: "mod+alt+z", label: () => t("shortcuts.actions.discardFile") },
  { id: "reviewNextFile", scope: "diff", defaultAccelerator: "mod+alt+r", label: () => t("shortcuts.actions.reviewNextFile") },
  { id: "nextHunk", scope: "diff", defaultAccelerator: "f7", label: () => t("shortcuts.actions.nextHunk") },
  { id: "prevHunk", scope: "diff", defaultAccelerator: "shift+f7", label: () => t("shortcuts.actions.prevHunk") },
  { id: "nextFile", scope: "global", defaultAccelerator: "mod+down", label: () => t("shortcuts.actions.nextFile") },
  { id: "prevFile", scope: "global", defaultAccelerator: "mod+up", label: () => t("shortcuts.actions.prevFile") },
  { id: "openRepo", scope: "global", defaultAccelerator: "mod+o", label: () => t("shortcuts.actions.openRepo") },
  { id: "cloneRepo", scope: "global", defaultAccelerator: "mod+shift+o", label: () => t("shortcuts.actions.cloneRepo") },
  { id: "pullRequests", scope: "global", defaultAccelerator: "mod+shift+p", label: () => t("shortcuts.actions.pullRequests") },
  { id: "refresh", scope: "global", defaultAccelerator: "mod+r", label: () => t("shortcuts.actions.refresh") },
  { id: "fontUp", scope: "global", defaultAccelerator: "mod+equal", label: () => t("shortcuts.actions.fontUp") },
  { id: "fontDown", scope: "global", defaultAccelerator: "mod+minus", label: () => t("shortcuts.actions.fontDown") },
  { id: "zoomIn", scope: "global", defaultAccelerator: "ctrl+alt+equal", label: () => t("shortcuts.actions.zoomIn") },
  { id: "zoomOut", scope: "global", defaultAccelerator: "ctrl+alt+minus", label: () => t("shortcuts.actions.zoomOut") },
  { id: "zoomReset", scope: "global", defaultAccelerator: "ctrl+alt+0", label: () => t("shortcuts.actions.zoomReset") },
  { id: "toggleTerminal", scope: "global", defaultAccelerator: "ctrl+backquote", label: () => t("shortcuts.actions.toggleTerminal") },
];

const BY_ID = new Map<string, ShortcutDef>(shortcutDefs.map((def) => [def.id, def]));

let overrides: Record<string, string> = {};

/** 载入设置里的自定义加速键（启动与设置保存后调用）。 */
export function setShortcutOverrides(next: Record<string, string> | null | undefined): void {
  overrides = next ? { ...next } : {};
}

export function shortcutOverrides(): Record<string, string> {
  return { ...overrides };
}

/** 某个动作当前生效的加速键。 */
export function acceleratorOf(id: ShortcutId): string {
  return overrides[id] ?? BY_ID.get(id)?.defaultAccelerator ?? "";
}

export function isCustomized(id: ShortcutId): boolean {
  return typeof overrides[id] === "string" && overrides[id] !== BY_ID.get(id)?.defaultAccelerator;
}

/** 修改某个动作的加速键；返回新的覆盖表（不改动当前状态，由调用方保存）。 */
export function withShortcut(
  current: Record<string, string>,
  id: ShortcutId,
  accelerator: string,
): Record<string, string> {
  const next = { ...current };
  const def = BY_ID.get(id);
  if (def && accelerator === def.defaultAccelerator) delete next[id];
  else next[id] = accelerator;
  return next;
}

/** 恢复单个动作的默认值。 */
export function withoutShortcut(current: Record<string, string>, id: ShortcutId): Record<string, string> {
  const next = { ...current };
  delete next[id];
  return next;
}

/** 找出与给定加速键冲突的其他动作（同一加速键只能绑定一个动作）。 */
export function conflictFor(
  accelerator: string,
  except: ShortcutId,
  current: Record<string, string> = overrides,
): ShortcutId | null {
  const normalized = normalizeAccelerator(accelerator);
  for (const def of shortcutDefs) {
    if (def.id === except) continue;
    const active = current[def.id] ?? def.defaultAccelerator;
    if (active === normalized) return def.id;
  }
  return null;
}

export function shortcutLabel(id: ShortcutId): string {
  return BY_ID.get(id)?.label() ?? id;
}

// ---------------- 加速键的规范化与展示 ----------------

const CODE_TOKENS: Record<string, string> = {
  Equal: "equal",
  Minus: "minus",
  BracketLeft: "bracketleft",
  BracketRight: "bracketright",
  Backslash: "backslash",
  Semicolon: "semicolon",
  Quote: "quote",
  Comma: "comma",
  Period: "period",
  Slash: "slash",
  Backquote: "backquote",
  Space: "space",
  Enter: "enter",
  NumpadEnter: "enter",
  Tab: "tab",
  Escape: "escape",
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  Home: "home",
  End: "end",
  PageUp: "pageup",
  PageDown: "pagedown",
  Delete: "delete",
  Backspace: "backspace",
  NumpadAdd: "plus",
  NumpadSubtract: "minus",
};

const KEY_TOKENS: Record<string, string> = {
  "+": "plus",
  "-": "minus",
  "=": "equal",
  "[": "bracketleft",
  "]": "bracketright",
  "\\": "backslash",
  ";": "semicolon",
  "'": "quote",
  ",": "comma",
  ".": "period",
  "/": "slash",
  "`": "backquote",
  " ": "space",
};

/** 从按键事件取按键 token（优先 `event.code`）。 */
function keyToken(event: KeyboardEvent): string {
  const code = event.code;
  if (code.startsWith("Key") && code.length === 4) return code.slice(3).toLowerCase();
  if (code.startsWith("Digit") && code.length === 6) return code.slice(5);
  if (/^F\d{1,2}$/.test(code)) return code.toLowerCase();
  const fromCode = CODE_TOKENS[code];
  if (fromCode) return fromCode;
  const key = event.key;
  if (key.length === 1) {
    const lower = key.toLowerCase();
    return KEY_TOKENS[lower] ?? lower;
  }
  return key.toLowerCase();
}

const MODIFIER_TOKENS = new Set(["mod", "ctrl", "alt", "shift", "meta"]);

/** 把按键事件转成规范加速键；只按修饰键时返回 `null`。 */
export function captureAccelerator(event: KeyboardEvent): string | null {
  const key = keyToken(event);
  if (MODIFIER_TOKENS.has(key) || key === "meta" || key === "control" || key === "alt" || key === "shift") {
    return null;
  }
  const parts: string[] = [];
  if (isMac ? event.metaKey : event.ctrlKey) parts.push("mod");
  // 非主修饰键单独记录：macOS 上的 Ctrl 与 Windows 上的 Meta 都是额外修饰键。
  if (isMac && event.ctrlKey) parts.push("ctrl");
  if (!isMac && event.metaKey) parts.push("meta");
  if (event.altKey) parts.push("alt");
  if (event.shiftKey) parts.push("shift");
  parts.push(key);
  return normalizeAccelerator(parts.join("+"));
}

/** 规范形式：修饰键顺序固定、字母小写、`plus`/`equal` 统一。 */
export function normalizeAccelerator(value: string): string {
  const parts = value
    .trim()
    .toLowerCase()
    .split("+")
    .map((part) => part.trim())
    .filter((part) => part.length > 0);
  const modifiers: string[] = [];
  const keys: string[] = [];
  for (const part of parts) {
    if (MODIFIER_TOKENS.has(part)) {
      if (!modifiers.includes(part)) modifiers.push(part);
    } else {
      keys.push(part);
    }
  }
  const order = ["mod", "ctrl", "meta", "alt", "shift"];
  modifiers.sort((left, right) => order.indexOf(left) - order.indexOf(right));
  return [...modifiers, ...keys].join("+");
}

const SYMBOLS: Record<string, string> = isMac
  ? { mod: "⌘", ctrl: "⌃", meta: "⌘", alt: "⌥", shift: "⇧", equal: "=", minus: "-", plus: "+", backquote: "`", up: "↑", down: "↓", left: "←", right: "→", space: "Space", enter: "↩", escape: "Esc", tab: "⇥", comma: ",", period: ".", slash: "/", semicolon: ";", quote: "'", bracketleft: "[", bracketright: "]", backslash: "\\" }
  : { mod: "Ctrl+", ctrl: "Ctrl+", meta: "Win+", alt: "Alt+", shift: "Shift+", equal: "=", minus: "-", plus: "+", backquote: "`", up: "↑", down: "↓", left: "←", right: "→", space: "Space", enter: "Enter", escape: "Esc", tab: "Tab", comma: ",", period: ".", slash: "/", semicolon: ";", quote: "'", bracketleft: "[", bracketright: "]", backslash: "\\" };

/** 展示用文案：macOS 用 ⌘⌥⇧ 符号，其他平台用 Ctrl / Alt / Shift。 */
export function formatAccelerator(accelerator: string): string {
  if (!accelerator) return t("shortcuts.unassigned");
  const parts = accelerator.split("+");
  if (isMac) {
    return parts.map((part) => SYMBOLS[part] ?? (part.length === 1 ? part.toUpperCase() : capitalize(part))).join("");
  }
  return parts.map((part) => SYMBOLS[part] ?? (part.length === 1 ? part.toUpperCase() : capitalize(part))).join("+");
}

function capitalize(value: string): string {
  return value.length === 0 ? value : value[0].toUpperCase() + value.slice(1);
}

function modifiersMatch(parts: string[], event: KeyboardEvent): boolean {
  const wants = new Set(parts.filter((part) => MODIFIER_TOKENS.has(part)).map((part) => (part === "mod" ? (isMac ? "meta" : "ctrl") : part)));
  const has = new Set<string>();
  if (event.metaKey) has.add("meta");
  if (event.ctrlKey) has.add("ctrl");
  if (event.altKey) has.add("alt");
  if (event.shiftKey) has.add("shift");
  if (wants.size !== has.size) return false;
  for (const modifier of wants) if (!has.has(modifier)) return false;
  return true;
}

function keysMatch(expected: string, actual: string): boolean {
  if (expected === actual) return true;
  // `=` 与 `+`、`-` 与 `_` 在键盘上是同一个物理键，按名字归一后允许互相匹配。
  const equivalents: Record<string, string> = { plus: "equal", equal: "plus" };
  return equivalents[expected] === actual || equivalents[actual] === expected;
}

/** 判断事件是否命中某个加速键。 */
export function matchesAccelerator(event: KeyboardEvent, accelerator: string): boolean {
  if (!accelerator) return false;
  const parts = accelerator.split("+");
  const expectedKey = parts.filter((part) => !MODIFIER_TOKENS.has(part)).pop() ?? "";
  if (!expectedKey) return false;
  const actual = keyToken(event);
  if (!keysMatch(expectedKey, actual)) return false;
  // `=`/`+` 在多数键盘上需要 Shift：命中 `equal` 时忽略 Shift 差异。
  const relaxed = expectedKey === "equal" || expectedKey === "plus";
  if (relaxed) {
    const withoutShift = new Set(parts.filter((part) => part !== "shift"));
    return modifiersMatch([...withoutShift], event);
  }
  return modifiersMatch(parts, event);
}

/** 命中的动作（没有命中返回 `null`）。 */
export function matchShortcut(event: KeyboardEvent): ShortcutId | null {
  for (const def of shortcutDefs) {
    if (matchesAccelerator(event, acceleratorOf(def.id))) return def.id;
  }
  return null;
}

/** 供设置页“按下新快捷键”使用的展示文案。 */
export function describeEvent(event: KeyboardEvent): string | null {
  const captured = captureAccelerator(event);
  return captured ? formatAccelerator(captured) : null;
}
