/**
 * 界面语言的运行时（M6 L01/L02，Q163–Q164、Q171）。
 *
 * 文案资源是 `locales/zh-CN.json` 与 `locales/en.json`，由 x18n 维护；
 * 组件里不写死中文，也不拼接键名（动态查表一律用完整字面量键）。
 *
 * - `messages` 是随语言切换的只读代理：已有 `messages.toolbar.chooseRepo`
 *   写法不用改，切换语言后重新渲染即读到新语言。
 * - `t(key, params)` 用于带参数的文案；参数占位符形如 `{count}`。
 * - 语言设置存 Rust 设置（`language`），`auto` 首次按系统语言选择。
 */
import type { Messages } from "./types";
import { buildMessages } from "./build";
import zhCNRaw from "../../../locales/zh-CN.json" with { type: "json" };
import enRaw from "../../../locales/en.json" with { type: "json" };
import { format } from "./format";

export type Locale = "zh-CN" | "en";
export type LanguagePreference = "auto" | Locale;

/** 语言变化通知：App 用它触发整棵界面重新渲染。 */
const listeners = new Set<() => void>();
let current: Locale = "zh-CN";
let systemOverride: Locale | null = null;

const dictionaries: Record<Locale, Messages> = {
  "zh-CN": buildMessages(zhCNRaw as never, "zh-CN"),
  en: buildMessages(enRaw as never, "en"),
};

/** 按系统语言选择首次界面语言（Q164：非中文系统用英文）。 */
export function systemLocale(): Locale {
  const languages =
    typeof navigator !== "undefined"
      ? [...(navigator.languages ?? []), navigator.language ?? ""]
      : [];
  const first = languages.find((value) => value && value.length > 0) ?? "zh-CN";
  return first.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}

/** 平台层（Rust）检测到的系统语言；首次启动优先于浏览器语言。 */
export function setSystemLanguage(tag: string): void {
  systemOverride = tag.toLowerCase().startsWith("zh") ? "zh-CN" : "en";
}

export function resolveLocale(preference: LanguagePreference): Locale {
  if (preference !== "auto") return preference;
  return systemOverride ?? systemLocale();
}

export function getLocale(): Locale {
  return current;
}

/** 应用语言；相同语言不重复通知。 */
export function setLocale(next: Locale): void {
  if (next === current) return;
  current = next;
  if (typeof document !== "undefined") document.documentElement.lang = next;
  for (const listener of listeners) listener();
}

export function subscribeLocale(listener: () => void): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** 当前语言的字典（用于需要整棵树的场合）。 */
export function messagesFor(locale: Locale): Messages {
  return dictionaries[locale];
}

/** 按完整字面量键读取文案，例如 `t("log.dateLabels.today")`。 */
export function t(key: string, params?: Record<string, string | number>): string {
  return translateIn(dictionaries[current], key, params, current);
}

export function translateIn(
  dictionary: Messages,
  key: string,
  params?: Record<string, string | number>,
  locale: Locale = current,
): string {
  let cursor: unknown = dictionary;
  for (const segment of key.split(".")) {
    if (cursor === null || typeof cursor !== "object") return key;
    cursor = (cursor as Record<string, unknown>)[segment];
  }
  if (typeof cursor === "string") return params ? format(cursor, params, locale) : cursor;
  if (typeof cursor === "function" && params) {
    return (cursor as (...args: unknown[]) => string)(...Object.values(params));
  }
  return key;
}

/** 当前语言的字典；用代理让切换语言后的重新渲染读到新语言。 */
export const messages: Messages = new Proxy({} as Messages, {
  get(_target, property) {
    return dictionaries[current][property as keyof Messages];
  },
  has(_target, property) {
    return property in dictionaries[current];
  },
  ownKeys() {
    return Reflect.ownKeys(dictionaries[current]);
  },
  getOwnPropertyDescriptor(_target, property) {
    return {
      configurable: true,
      enumerable: true,
      value: dictionaries[current][property as keyof Messages],
    };
  },
});

export { format };
