// ts-rs 生成的类型会在行尾留下空格（例如 `path: string, ` 这类多行注释排版）。
// 生成物要提交进仓库，行尾空白会让 `git diff --check` 报错，所以在生成后统一去掉。
//
// 只删行尾空白，不动其它内容；用现有 `pnpm bindings` 流程串起来。
import { readdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";

const directory = path.resolve(import.meta.dirname, "../src/types/bindings");
let changed = 0;
for (const name of await readdir(directory)) {
  if (!name.endsWith(".ts")) continue;
  const file = path.join(directory, name);
  const before = await readFile(file, "utf8");
  const after = before.replace(/[ \t]+$/gm, "");
  if (after !== before) {
    await writeFile(file, after);
    changed += 1;
  }
}
console.log(`已清理 ${changed} 个绑定文件的行尾空白`);
