/**
 * 外观模式与代码颜色主题（M4 Q132、M13 Q237、ADR-0016）。
 *
 * 界面外壳采用 Revia 精调的中性浅/深色 Design Tokens（随外观模式切换）；
 * 代码 diff、三方合并与 blame 表面跟随所选内置或导入的 VS Code 颜色主题。
 */

import type { AppearanceMode } from "@/lib/ipc";

export type ThemeKind = "light" | "dark";

export interface ThemeInfo {
  id: string;
  label: string;
  kind: ThemeKind;
}

export const THEMES: readonly ThemeInfo[] = [
  { id: "one-light", label: "Atom One Light", kind: "light" },
  { id: "light-2026", label: "VS Code Light 2026", kind: "light" },
  { id: "one-dark-pro", label: "One Dark Pro", kind: "dark" },
  { id: "dark-2026", label: "VS Code Dark 2026", kind: "dark" },
];

export const DEFAULT_LIGHT_THEME = "one-light";
export const DEFAULT_DARK_THEME = "one-dark-pro";

/** 导入的 VS Code 主题（M7 任务 02）：M13 起仅允许覆盖代码、diff 与语法高亮色键（ADR-0016）。 */
const CODE_THEME_VAR_PREFIXES = [
  "editor-background",
  "editor-foreground",
  "editor-linehighlightbackground",
  "editorgutter-",
  "editorlinenumber-",
  "diffeditor-",
  "diffeditorgutter-",
  "syn-",
] as const;

function isCodeThemeVar(name: string): boolean {
  return CODE_THEME_VAR_PREFIXES.some((prefix) => name === prefix || name.startsWith(prefix));
}

interface ImportedThemeInfo extends ThemeInfo {
  colors: Record<string, string>;
}

let importedThemes: ImportedThemeInfo[] = [];
/** 上一次写入的内联变量，切换主题时先清掉，避免残留。 */
const appliedImportedVars = new Set<string>();

/** 更新导入主题列表；由 App 在启动与导入/删除后调用。 */
export function setImportedThemes(list: readonly { id: string; label: string; kind: ThemeKind; colors: Record<string, string> }[]): void {
  importedThemes = list.map((theme) => ({
    id: theme.id,
    label: theme.label,
    kind: theme.kind,
    colors: theme.colors,
  }));
}

export function importedThemeIds(): string[] {
  return importedThemes.map((theme) => theme.id);
}

export function themeById(id: string | null | undefined): ThemeInfo | null {
  const builtin = THEMES.find((theme) => theme.id === id);
  if (builtin) return builtin;
  const imported = importedThemes.find((theme) => theme.id === id);
  return imported ? { id: imported.id, label: imported.label, kind: imported.kind } : null;
}

/** 浅色或深色代码颜色主题候选：内置 + 导入。 */
export function themesOfKind(kind: ThemeKind): ThemeInfo[] {
  return [
    ...THEMES.filter((theme) => theme.kind === kind),
    ...importedThemes.filter((theme) => theme.kind === kind).map(({ id, label, kind: themeKind }) => ({ id, label, kind: themeKind })),
  ];
}

/** 当前系统是否偏好深色。 */
export function systemPrefersDark(): boolean {
  return typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: dark)").matches === true;
}

/** 解析最终生效的深浅外观（M13 Q237）。 */
export function resolveEffectiveDark(appearanceMode: AppearanceMode | undefined, prefersDark: boolean): boolean {
  if (appearanceMode === "dark") return true;
  if (appearanceMode === "light") return false;
  return prefersDark;
}

/** 按最终深浅色选出要应用的代码颜色主题；未知 id 回退到默认主题。 */
export function resolveTheme(
  lightId: string,
  darkId: string,
  prefersDark: boolean,
  appearanceMode: AppearanceMode = "system",
): ThemeInfo {
  const isDark = resolveEffectiveDark(appearanceMode, prefersDark);
  const preferred = themeById(isDark ? darkId : lightId);
  if (preferred && preferred.kind === (isDark ? "dark" : "light")) return preferred;
  return themeById(isDark ? DEFAULT_DARK_THEME : DEFAULT_LIGHT_THEME)!;
}

/**
 * 把外观深浅色与代码颜色主题写到 `<html>` 上。
 * 外壳由 `[data-kind]` 驱动，代码表面由 `[data-theme]`（或导入主题的代码内联色键）驱动。
 */
export function applyTheme(theme: ThemeInfo): void {
  const root = document.documentElement;
  for (const name of appliedImportedVars) root.style.removeProperty(`--${name}`);
  appliedImportedVars.clear();
  const imported = importedThemes.find((candidate) => candidate.id === theme.id);
  root.dataset.theme = imported ? (theme.kind === "dark" ? "dark-2026" : "light-2026") : theme.id;
  root.dataset.kind = theme.kind;
  if (imported) {
    for (const [name, value] of Object.entries(imported.colors)) {
      if (!isCodeThemeVar(name)) continue;
      root.style.setProperty(`--${name}`, value);
      appliedImportedVars.add(name);
    }
  }
}

export function codeFontStack(family: string | null): string {
  const trimmed = family?.trim() ?? "";
  return trimmed
    ? `"${trimmed.replaceAll('"', "")}", "JetBrains Mono", monospace`
    : '"JetBrains Mono", "SF Mono", SFMono-Regular, ui-monospace, Menlo, Consolas, monospace';
}

export function applyCodeFont(family: string | null, size: number): void {
  const root = document.documentElement;
  if (family?.trim()) {
    root.style.setProperty("--code-font-family", codeFontStack(family));
  } else {
    root.style.removeProperty("--code-font-family");
  }
  root.style.setProperty("--code-font-size", `${size}px`);
}

/** 监听系统深浅色变化；返回取消函数。 */
export function watchSystemTheme(onChange: (prefersDark: boolean) => void): () => void {
  if (typeof window === "undefined" || !window.matchMedia) return () => undefined;
  const query = window.matchMedia("(prefers-color-scheme: dark)");
  const listener = (event: MediaQueryListEvent) => onChange(event.matches);
  query.addEventListener("change", listener);
  return () => query.removeEventListener("change", listener);
}
