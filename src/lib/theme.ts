/**
 * 颜色主题（M4 F01、Q132、Q134）。
 *
 * 四套内置主题由 `tools/gen-themes.mjs` 从 VS Code 主题文件生成到
 * `src/themes/themes.css`，每套一个 `[data-theme="<id>"]` 块。
 * 浅色、深色各选一套，随系统深浅色切换；默认 Atom One Light 与 One Dark Pro。
 */

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

/** 导入的 VS Code 主题（M7 任务 02）：颜色以 CSS 变量内联写入，不改内置主题块。 */
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

/** 浅色主题候选（设置里的“浅色”下拉）：内置 + 导入。 */
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

/** 按系统深浅色选出要应用的主题；未知 id 回退到该外观的默认主题。 */
export function resolveTheme(lightId: string, darkId: string, prefersDark: boolean): ThemeInfo {
  const preferred = themeById(prefersDark ? darkId : lightId);
  if (preferred && preferred.kind === (prefersDark ? "dark" : "light")) return preferred;
  return themeById(prefersDark ? DEFAULT_DARK_THEME : DEFAULT_LIGHT_THEME)!;
}

/**
 * 把主题写到 `<html>` 上。主题块只定义变量，切换不重排组件树。
 * 代码字体与字号同时写入 CSS 变量（Q133）；字体本机没有时由 CSS 回退到 JetBrains Mono。
 */
export function applyTheme(theme: ThemeInfo): void {
  const root = document.documentElement;
  for (const name of appliedImportedVars) root.style.removeProperty(`--${name}`);
  appliedImportedVars.clear();
  const imported = importedThemes.find((candidate) => candidate.id === theme.id);
  // 导入主题没有内置 CSS 块：先落到同外观的基础块，再覆盖它自己的变量。
  root.dataset.theme = imported ? (theme.kind === "dark" ? "dark-2026" : "light-2026") : theme.id;
  root.dataset.kind = theme.kind;
  if (imported) {
    for (const [name, value] of Object.entries(imported.colors)) {
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
