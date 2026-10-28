#!/usr/bin/env node
/**
 * 从本机安装的 Material Icon Theme 扩展同步文件类型图标（M4 F01.3、Q130）。
 *
 * 只复制文件图标实际用到的 SVG（`fileExtensions`、`fileNames` 与默认 `file`），
 * 并生成 `manifest.json`。扩展目录可由第一个参数指定。
 *
 * 来源：VS Code 扩展 pkief.material-icon-theme 5.39.0（MIT）。
 * 许可文件复制到 `src/assets/material-icons/LICENSE.txt`。
 *
 * 重新生成：`node tools/sync-material-icons.mjs [扩展目录]`
 */

import { copyFileSync, existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const extension =
  process.argv[2] ?? join(homedir(), ".vscode/extensions/pkief.material-icon-theme-5.39.0");
const source = join(extension, "dist/material-icons.json");
if (!existsSync(source)) {
  console.error(`找不到 Material Icon Theme 的映射文件：${source}`);
  process.exit(1);
}
const data = JSON.parse(readFileSync(source, "utf8"));
const definitions = data.iconDefinitions ?? {};
const manifest = {
  source: "pkief.material-icon-theme 5.39.0",
  default: data.file ?? "file",
  fileExtensions: data.fileExtensions ?? {},
  fileNames: data.fileNames ?? {},
};
const used = new Set([
  ...Object.values(manifest.fileExtensions),
  ...Object.values(manifest.fileNames),
  manifest.default,
]);
const target = join(root, "src/assets/material-icons");
mkdirSync(join(target, "icons"), { recursive: true });
let copied = 0;
let missing = 0;
for (const name of used) {
  if (!(name in definitions)) continue;
  const from = join(extension, "icons", `${name}.svg`);
  if (!existsSync(from)) {
    missing += 1;
    continue;
  }
  copyFileSync(from, join(target, "icons", `${name}.svg`));
  copied += 1;
}
writeFileSync(join(target, "manifest.json"), `${JSON.stringify(manifest, null, 2)}\n`);
const license = join(extension, "LICENSE.txt");
if (existsSync(license)) copyFileSync(license, join(target, "LICENSE.txt"));
console.log(
  `已复制 ${copied} 个图标（${used.size} 个引用，${missing} 个缺少 SVG 时回退默认图标）→ ${target}`,
);
