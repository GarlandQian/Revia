#!/usr/bin/env node
/**
 * 从 VS Code 主题文件生成 `src/themes/themes.css`（M4 F01、Q132）。
 *
 * 主题按 VS Code 的格式书写：`include` 继承父主题，`colors` 覆盖界面颜色，
 * `tokenColors` 提供语法颜色。这里按继承链合并，取界面用到的键，并给
 * 四套内置主题各生成一个 `[data-theme="<id>"]` 块。
 *
 * 原始主题文件在 `src/themes/sources/`，许可见 `src/themes/licenses/`。
 * 重新生成：`node tools/gen-themes.mjs`
 */

import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const sources = join(root, "src/themes/sources");
const output = join(root, "src/themes/themes.css");

/** 去掉 JSONC 注释与尾逗号（VS Code 主题文件是 JSONC）。 */
function parseJsonc(text) {
  return JSON.parse(
    text
      .replace(/^\s*\/\/.*$/gm, "")
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/,(\s*[}\]])/g, "$1"),
  );
}

function load(path) {
  const data = parseJsonc(readFileSync(path, "utf8"));
  let colors = {};
  let tokens = [];
  if (data.include) {
    const parent = load(join(dirname(path), data.include));
    colors = { ...parent.colors };
    tokens = parent.tokens;
  }
  for (const [key, value] of Object.entries(data.colors ?? {})) colors[key.toLowerCase()] = value;
  if (Array.isArray(data.tokenColors)) tokens = tokens.concat(data.tokenColors);
  return { colors, tokens };
}

/** 主题缺键时的缺省值（照 VS Code 默认主题）。 */
const FALLBACK = {
  dark: {
    foreground: "#cccccc", descriptionforeground: "#ccccccb3", "icon.foreground": "#c5c5c5",
    focusborder: "#007fd4", "titlebar.activebackground": "#3c3c3c", "titlebar.activeforeground": "#cccccc",
    "titlebar.border": "#00000000", "activitybar.background": "#333333", "activitybar.foreground": "#ffffff",
    "activitybar.inactiveforeground": "#ffffff66", "activitybar.activeborder": "#ffffff",
    "activitybar.border": "#00000000", "activitybarbadge.background": "#0078d4", "activitybarbadge.foreground": "#ffffff",
    "sidebar.background": "#252526", "sidebar.foreground": "#cccccc",
    "sidebar.border": "#00000000", "sidebarsectionheader.background": "#00000000",
    "sidebarsectionheader.foreground": "#cccccc", "sidebarsectionheader.border": "#cccccc33",
    "sidebartitle.foreground": "#bbbbbb", "list.activeselectionbackground": "#04395e",
    "list.activeselectionforeground": "#ffffff", "list.inactiveselectionbackground": "#37373d",
    "list.hoverbackground": "#2a2d2e", "list.focusoutline": "#007fd4", "editor.background": "#1e1e1e",
    "editor.foreground": "#d4d4d4", "editorlinenumber.foreground": "#858585",
    "editorlinenumber.activeforeground": "#c6c6c6", "editorgroupheader.tabsbackground": "#252526",
    "editorgroupheader.tabsborder": "#00000000", "tab.activebackground": "#1e1e1e",
    "tab.activeforeground": "#ffffff", "tab.inactivebackground": "#2d2d2d",
    "tab.inactiveforeground": "#ffffff80", "tab.border": "#252526", "tab.activebordertop": "#00000000",
    "tab.activeborder": "#00000000", "statusbar.background": "#007acc", "statusbar.foreground": "#ffffff",
    "statusbar.border": "#00000000", "button.background": "#0e639c", "button.foreground": "#ffffff",
    "button.secondarybackground": "#3a3d41", "button.secondaryforeground": "#ffffff",
    "input.background": "#3c3c3c", "input.foreground": "#cccccc", "input.border": "#00000000",
    "input.placeholderforeground": "#a6a6a6", "checkbox.background": "#3c3c3c", "checkbox.border": "#6b6b6b",
    "badge.background": "#4d4d4d", "badge.foreground": "#ffffff", "panel.border": "#80808059",
    "toolbar.hoverbackground": "#5a5d5e50", "diffeditor.insertedtextbackground": "#9ccc2c33",
    "diffeditor.removedtextbackground": "#ff000033", "diffeditor.insertedlinebackground": "#9bb95533",
    "diffeditor.removedlinebackground": "#ff000033", "diffeditor.diagonalfill": "#cccccc33",
    "diffeditorgutter.insertedlinebackground": "#9bb95533",
    "diffeditorgutter.removedlinebackground": "#ff000033",
    "gitdecoration.modifiedresourceforeground": "#e2c08d", "gitdecoration.addedresourceforeground": "#81b88b",
    "gitdecoration.deletedresourceforeground": "#c74e39",
    "gitdecoration.untrackedresourceforeground": "#73c991",
    "gitdecoration.conflictingresourceforeground": "#e4676b",
    "gitdecoration.renamedresourceforeground": "#73c991",
    "gitdecoration.stagemodifiedresourceforeground": "#e2c08d", "editorwidget.background": "#252526",
    "widget.border": "#00000000", "notifications.background": "#252526", "notifications.border": "#00000000",
    "textlink.foreground": "#3794ff", "editorinfo.foreground": "#3794ff",
    "editorwarning.foreground": "#cca700", "breadcrumb.foreground": "#cccccccc",
    "scrollbarslider.background": "#79797966", "editorgutter.background": "#1e1e1e",
    "editor.linehighlightbackground": "#00000000",
  },
  light: {
    foreground: "#616161", descriptionforeground: "#717171", "icon.foreground": "#424242",
    focusborder: "#0090f1", "titlebar.activebackground": "#dddddd", "titlebar.activeforeground": "#333333",
    "titlebar.border": "#00000000", "activitybar.background": "#2c2c2c", "activitybar.foreground": "#ffffff",
    "activitybar.inactiveforeground": "#ffffff66", "activitybar.activeborder": "#ffffff",
    "activitybar.border": "#00000000", "activitybarbadge.background": "#0078d4", "activitybarbadge.foreground": "#ffffff",
    "sidebar.background": "#f3f3f3", "sidebar.foreground": "#616161",
    "sidebar.border": "#00000000", "sidebarsectionheader.background": "#00000000",
    "sidebarsectionheader.foreground": "#616161", "sidebarsectionheader.border": "#61616133",
    "sidebartitle.foreground": "#6f6f6f", "list.activeselectionbackground": "#0060c0",
    "list.activeselectionforeground": "#ffffff", "list.inactiveselectionbackground": "#e4e6f1",
    "list.hoverbackground": "#f0f0f0", "list.focusoutline": "#0090f1", "editor.background": "#ffffff",
    "editor.foreground": "#000000", "editorlinenumber.foreground": "#237893",
    "editorlinenumber.activeforeground": "#0b216f", "editorgroupheader.tabsbackground": "#f3f3f3",
    "editorgroupheader.tabsborder": "#00000000", "tab.activebackground": "#ffffff",
    "tab.activeforeground": "#333333", "tab.inactivebackground": "#ececec",
    "tab.inactiveforeground": "#33333380", "tab.border": "#f3f3f3", "tab.activebordertop": "#00000000",
    "tab.activeborder": "#00000000", "statusbar.background": "#007acc", "statusbar.foreground": "#ffffff",
    "statusbar.border": "#00000000", "button.background": "#007acc", "button.foreground": "#ffffff",
    "button.secondarybackground": "#5f6a79", "button.secondaryforeground": "#ffffff",
    "input.background": "#ffffff", "input.foreground": "#616161", "input.border": "#cecece",
    "input.placeholderforeground": "#767676", "checkbox.background": "#ffffff", "checkbox.border": "#919191",
    "badge.background": "#c4c4c4", "badge.foreground": "#333333", "panel.border": "#80808059",
    "toolbar.hoverbackground": "#b8b8b850", "diffeditor.insertedtextbackground": "#9ccc2c40",
    "diffeditor.removedtextbackground": "#ff000033", "diffeditor.insertedlinebackground": "#9bb95533",
    "diffeditor.removedlinebackground": "#ff000033", "diffeditor.diagonalfill": "#22222233",
    "diffeditorgutter.insertedlinebackground": "#9bb95533",
    "diffeditorgutter.removedlinebackground": "#ff000033",
    "gitdecoration.modifiedresourceforeground": "#895503", "gitdecoration.addedresourceforeground": "#587c0c",
    "gitdecoration.deletedresourceforeground": "#ad0707",
    "gitdecoration.untrackedresourceforeground": "#007100",
    "gitdecoration.conflictingresourceforeground": "#ad0707",
    "gitdecoration.renamedresourceforeground": "#007100",
    "gitdecoration.stagemodifiedresourceforeground": "#895503", "editorwidget.background": "#f3f3f3",
    "widget.border": "#00000000", "notifications.background": "#f3f3f3", "notifications.border": "#00000000",
    "textlink.foreground": "#006ab1", "editorinfo.foreground": "#1a85ff",
    "editorwarning.foreground": "#bf8803", "breadcrumb.foreground": "#616161cc",
    "scrollbarslider.background": "#64646466", "editorgutter.background": "#ffffff",
    "editor.linehighlightbackground": "#00000000",
  },
};

/** 语法类别 → TextMate scope。取最长匹配的规则（与 gen-themes.py 相同）。 */
const SCOPES = {
  comment: "comment",
  keyword: "keyword",
  storage: "storage.type",
  modifier: "storage.modifier",
  string: "string.quoted",
  number: "constant.numeric",
  constant: "constant.language",
  func: "entity.name.function",
  type: "entity.name.type",
  support: "support.type",
  variable: "variable.other.readwrite",
  param: "variable.parameter",
  property: "variable.other.property",
  operator: "keyword.operator",
  punct: "punctuation.separator",
  tag: "entity.name.tag",
  attr: "entity.other.attribute-name",
};

function tokenStyle(tokens, target) {
  let best = null;
  let bestLength = -1;
  for (const rule of tokens) {
    const scope = rule.scope;
    const settings = rule.settings ?? {};
    if (!scope || (!("foreground" in settings) && !("fontStyle" in settings))) continue;
    for (const selector of Array.isArray(scope) ? scope : [scope]) {
      for (const part of selector.split(",")) {
        const trimmed = part.trim();
        if (!trimmed || trimmed.includes(" ")) continue;
        if ((target === trimmed || target.startsWith(`${trimmed}.`)) && trimmed.length >= bestLength) {
          best = settings;
          bestLength = trimmed.length;
        }
      }
    }
  }
  return best ?? {};
}

const THEMES = [
  ["light-2026", "2026-light.json", "light"],
  ["one-light", "OneLight.json", "light"],
  ["dark-2026", "2026-dark.json", "dark"],
  ["one-dark-pro", "OneDark-Pro.json", "dark"],
];

const blocks = [];
for (const [id, file, kind] of THEMES) {
  const { colors, tokens } = load(join(sources, file));
  const merged = { ...FALLBACK[kind], ...colors };
  if (!("tab.activebackground" in colors)) merged["tab.activebackground"] = merged["editor.background"];
  if (!("editorgutter.background" in colors)) merged["editorgutter.background"] = merged["editor.background"];
  const lines = [`[data-theme="${id}"] {`, `  color-scheme: ${kind};`];
  for (const key of Object.keys(FALLBACK[kind]).sort()) {
    // `foreground` 与 shadcn 的语义色同名，主题侧改名为 `ui-foreground`，避免自引用。
    const name = key === "foreground" ? "ui-foreground" : key.replaceAll(".", "-");
    lines.push(`  --${name}: ${merged[key]};`);
  }
  lines.push(`  --syn-default: ${merged["editor.foreground"]};`);
  for (const [name, scope] of Object.entries(SCOPES)) {
    const style = tokenStyle(tokens, scope);
    lines.push(`  --syn-${name}: ${style.foreground ?? merged["editor.foreground"]};`);
    if ((style.fontStyle ?? "").includes("italic")) lines.push(`  --syn-${name}-style: italic;`);
  }
  lines.push("}");
  blocks.push(lines.join("\n"));
  console.log(`${id}: ${Object.keys(colors).length} 个自有颜色键，${tokens.length} 条语法规则`);
}

writeFileSync(output, `${blocks.join("\n\n")}\n`);
console.log(`已写入 ${output}`);
