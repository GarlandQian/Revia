import { createHighlighterCore } from "shiki/core";
import { createJavaScriptRegexEngine } from "shiki/engine/javascript";

const file = process.argv[2] ?? "/tmp/revia-m0-fixtures/diffs/src/generated.ts";
const content = await (await import("node:fs/promises")).readFile(file, "utf8");
const lineCount = content.split("\n").length;

const highlighter = await createHighlighterCore({
  themes: [import("shiki/themes/github-light.mjs")],
  langs: [import("shiki/langs/typescript.mjs")],
  engine: createJavaScriptRegexEngine({ forgiving: true }),
});

for (const [label, text] of [["完整文件", content], ["只取前 200 行", content.split("\n").slice(0, 200).join("\n")]]) {
  const started = performance.now();
  const result = highlighter.codeToTokens(text, { lang: "typescript", theme: "github-light" });
  const tokens = result.tokens.reduce((total, line) => total + line.length, 0);
  const elapsed = performance.now() - started;
  console.log(`${label}：${elapsed.toFixed(1)} ms，行 ${result.tokens.length}，token ${tokens}`);
}

// 与 syntect 相同粒度的“两个版本”耗时
const startedBoth = performance.now();
highlighter.codeToTokens(content, { lang: "typescript", theme: "github-light" });
console.log(`文件行数 ${lineCount}；JSON 化 token 大小约 ${(JSON.stringify(highlighter.codeToTokens(content, { lang: "typescript", theme: "github-light" }).tokens).length / 1024 / 1024).toFixed(1)} MB`);
console.log(`两次全量高亮合计：${(performance.now() - startedBoth).toFixed(1)} ms`);
