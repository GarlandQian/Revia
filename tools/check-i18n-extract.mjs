#!/usr/bin/env node
// L04：硬编码中文提取门禁。
//
// `x18n extract --list` 只是列举候选，CLI 本身成功退出不代表界面没有写死
// 中文。这里把候选中的 CJK 文案作为失败条件，保留路径与行号供修复。
// CSS 模板、类名、主题标识等非 CJK 候选不阻断构建。
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const x18nBin = fileURLToPath(new URL("../node_modules/@x18n/cli/bin.mjs", import.meta.url));
const result = spawnSync(process.execPath, [x18nBin, "extract", "--list", "--json"], {
  encoding: "utf8",
  maxBuffer: 64 * 1024 * 1024,
});

if (result.error) {
  console.error(`无法运行 x18n extract：${result.error.message}`);
  process.exit(1);
}
if (result.status !== 0) {
  console.error("x18n extract 失败。");
  if (result.stderr) console.error(result.stderr.trim());
  if (result.stdout) console.error(result.stdout.trim());
  process.exit(result.status ?? 1);
}

let report;
try {
  report = JSON.parse(result.stdout);
} catch (error) {
  console.error(`x18n extract 输出不是有效 JSON：${error.message}`);
  process.exit(1);
}

const items = report?.data?.items;
if (!Array.isArray(items)) {
  console.error("x18n extract 输出缺少 data.items，无法判定硬编码文案。");
  process.exit(1);
}

const cjk = /[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff]/u;
const hasCjk = (item) =>
  [item.text, item.original, item.value, item.replacement].some(
    (value) => typeof value === "string" && cjk.test(value),
  );
const offenders = items.filter(hasCjk);

console.log(`硬编码中文提取：${items.length} 个候选，${offenders.length} 个含中文。`);
for (const item of offenders) {
  console.error(`${item.path ?? item.file}:${item.line ?? "?"}  ${item.text ?? item.value ?? ""}`);
}

if (offenders.length > 0) {
  console.error("L04 失败：产品界面仍有写死中文。请迁入 locales/zh-CN.json 与 locales/en.json，");
  console.error("调试 / 测量日志使用最小范围 x18n-ignore，不要排除整个界面目录。");
  process.exit(1);
}
