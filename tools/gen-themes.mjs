#!/usr/bin/env node
/**
 * 从 VS Code 主题文件生成 `src/themes/themes.css`（M4 Q132、M13 ADR-0016）。
 *
 * M13 起界面外壳改用 Revia 语义 Design Tokens（`src/styles/tokens.css`），
 * 这里只提取代码 / diff / 三方合并 / 语法高亮所需的色键，给四套内置代码颜色主题
 * 各生成一个 `[data-theme="<id>"]` 块。
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

/** 代码表面色键缺省值（只包含编辑器 / diff / 滚动条色键，外壳不读主题，见 ADR-0016）。 */
const FALLBACK = {
  dark: {
    "editor.background": "#1e1e1e",
    "editor.foreground": "#d4d4d4",
    "editor.linehighlightbackground": "#00000000",
    "editorgutter.background": "#1e1e1e",
    "editorlinenumber.foreground": "#858585",
    "editorlinenumber.activeforeground": "#c6c6c6",
    "diffeditor.insertedtextbackground": "#9ccc2c33",
    "diffeditor.removedtextbackground": "#ff000033",
    "diffeditor.insertedlinebackground": "#9bb95533",
    "diffeditor.removedlinebackground": "#ff000033",
    "diffeditor.diagonalfill": "#cccccc33",
    "diffeditorgutter.insertedlinebackground": "#9bb95533",
    "diffeditorgutter.removedlinebackground": "#ff000033",
    "scrollbarslider.background": "#79797966",
  },
  light: {
    "editor.background": "#ffffff",
    "editor.foreground": "#000000",
    "editor.linehighlightbackground": "#00000000",
    "editorgutter.background": "#ffffff",
    "editorlinenumber.foreground": "#237893",
    "editorlinenumber.activeforeground": "#0b216f",
    "diffeditor.insertedtextbackground": "#9ccc2c40",
    "diffeditor.removedtextbackground": "#ff000033",
    "diffeditor.insertedlinebackground": "#9bb95533",
    "diffeditor.removedlinebackground": "#ff000033",
    "diffeditor.diagonalfill": "#22222233",
    "diffeditorgutter.insertedlinebackground": "#9bb95533",
    "diffeditorgutter.removedlinebackground": "#ff000033",
    "scrollbarslider.background": "#64646466",
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
  if (!("editorgutter.background" in colors)) merged["editorgutter.background"] = merged["editor.background"];
  const lines = [`[data-theme="${id}"] {`, `  color-scheme: ${kind};`];
  for (const key of Object.keys(FALLBACK[kind]).sort()) {
    const name = key.replaceAll(".", "-");
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
