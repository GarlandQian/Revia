// 用与 WebView worker 相同的 Shiki 配置（Oniguruma WASM 引擎 + 同一套语法）输出 token 颜色，
// 便于与 Rust 侧 syntect 的结果逐行对照。
import { readFile } from "node:fs/promises";
import { createHighlighterCore } from "shiki/core";
import { createOnigurumaEngine } from "shiki/engine/oniguruma";

const file = process.argv[2];
const lang = process.argv[3] ?? "txt";
const limit = Number(process.argv[4] ?? 25);

const highlighter = await createHighlighterCore({
  themes: [import("shiki/themes/github-light.mjs")],
  langs: [import(`shiki/langs/${lang}.mjs`)],
  engine: createOnigurumaEngine(import("shiki/wasm")),
});

const content = await readFile(file, "utf8");
const started = performance.now();
const result = highlighter.codeToTokens(content, { lang, theme: "github-light" });
console.log(`syntax=${lang} elapsed=${Math.round(performance.now() - started)}ms lines=${result.tokens.length}`);
result.tokens.slice(0, limit).forEach((tokens, index) => {
  const text = tokens
    .map((token) => `${token.content.replace(/\n$/, "")}[${token.color ?? ""}]`)
    .join("");
  console.log(String(index + 1).padStart(4), text);
});
