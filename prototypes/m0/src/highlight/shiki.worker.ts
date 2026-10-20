/**
 * Shiki 高亮（核心包 + 按需语法）：在 Web Worker 里运行，避免阻塞渲染线程。
 *
 * 用 `shiki/core` 而不是整包，避免把全部语法打进 bundle（整包约 10 MB）。
 * 高亮对象是完整文件内容（与 Rust 侧 syntect 保持同一粒度），前端只按行号取用。
 */

import { createHighlighterCore, type HighlighterCore } from "shiki/core";
import { createOnigurumaEngine } from "shiki/engine/oniguruma";
import type { HighlightLine } from "../diff/types";

interface WorkerRequest {
  id: number;
  language: string;
  content: string;
}

interface WorkerResponse {
  id: number;
  ok: boolean;
  log?: string;
  error?: string;
  tookMs?: number;
  lines?: HighlightLine[];
  /** 实际使用的语法名与主题名，用于和 Rust 侧对照。 */
  syntax?: string;
  theme?: string;
  /** 分片返回时的起始行号（0 基）。 */
  from?: number;
  /** 是否还有后续分片。 */
  more?: boolean;
  supported?: boolean;
}


/** 只注册原型用到的语法；新增语言时在这里补一行。 */
const languageLoaders: Record<string, () => Promise<unknown>> = {
  ts: () => import("shiki/langs/typescript.mjs"),
  js: () => import("shiki/langs/javascript.mjs"),
  jsx: () => import("shiki/langs/jsx.mjs"),
  tsx: () => import("shiki/langs/tsx.mjs"),
  vue: () => import("shiki/langs/vue.mjs"),
  swift: () => import("shiki/langs/swift.mjs"),
  json: () => import("shiki/langs/json.mjs"),
  yaml: () => import("shiki/langs/yaml.mjs"),
  markdown: () => import("shiki/langs/markdown.mjs"),
  shellscript: () => import("shiki/langs/shellscript.mjs"),
  rust: () => import("shiki/langs/rust.mjs"),
  python: () => import("shiki/langs/python.mjs"),
  css: () => import("shiki/langs/css.mjs"),
  html: () => import("shiki/langs/html.mjs"),
};

const themeLoader = () => import("shiki/themes/github-light.mjs");

let highlighterPromise: Promise<HighlighterCore> | null = null;
const loaded = new Set<string>();
const missing = new Set<string>();

export function getHighlighter(): Promise<HighlighterCore> {
  if (!highlighterPromise) {
    highlighterPromise = createHighlighterCore({
      themes: [themeLoader()],
      langs: [],
      // 用 Shiki 默认的 Oniguruma(WASM) 引擎：实测 JS 正则引擎在 WKWebView 里
      // 对数千行文件会慢到不可用（见报告）。
      engine: createOnigurumaEngine(import("shiki/wasm")),
    });
  }
  return highlighterPromise;
}

async function ensureLanguage(language: string): Promise<string | null> {
  if (!language || language === "txt" || language === "text") return null;
  if (missing.has(language)) return null;
  const loader = languageLoaders[language];
  if (!loader) {
    missing.add(language);
    return null;
  }
  if (loaded.has(language)) return language;
  const highlighter = await getHighlighter();
  await highlighter.loadLanguage((await loader()) as never);
  loaded.add(language);
  return language;
}

self.onmessage = async (event: MessageEvent<WorkerRequest>) => {
  const { id, language, content } = event.data;
  const started = performance.now();
  self.postMessage({ id, ok: true, log: `worker 收到请求：${language}，${content.length} 字节` });
  try {
    const resolved = await ensureLanguage(language);
    self.postMessage({ id, ok: true, log: `worker 语言解析：${resolved ?? "(无)"}，${Math.round(performance.now() - started)} ms` });
    if (!resolved) {
      // 没有对应语法时返回空表，界面按纯文本渲染。
      const response: WorkerResponse = {
        id,
        ok: true,
        tookMs: performance.now() - started,
        lines: content.split("\n").map(() => null),
        from: 0,
        more: false,
        supported: false,
      };
      self.postMessage(response);
      return;
    }
    const highlighter = await getHighlighter();
    self.postMessage({ id, ok: true, log: `worker 高亮开始，${Math.round(performance.now() - started)} ms` });
    const result = highlighter.codeToTokens(content, { lang: resolved as never, theme: "github-light" });
    self.postMessage({ id, ok: true, log: `worker 高亮完成，${Math.round(performance.now() - started)} ms，共 ${result.tokens.length} 行` });
    const toOutcome = (tokens: (typeof result.tokens)[number]): HighlightLine => {
      const pieces = tokens
        .map((token) => {
          const fontStyle = token.fontStyle ?? 0;
          return {
            text: token.content.replace(/\n$/, ""),
            color: typeof token.color === "string" ? token.color : "",
            bold: (fontStyle & 1) !== 0,
            italic: (fontStyle & 2) !== 0,
            underline: (fontStyle & 4) !== 0,
          };
        })
        .filter((piece) => piece.text.length > 0);
      return pieces.length > 0 ? pieces : null;
    };
    // 一次性返回：结构化克隆比逐片投递更快，也不依赖定时器/帧调度。
    const lines: HighlightLine[] = result.tokens.map(toOutcome);
    const response: WorkerResponse = {
      id,
      ok: true,
      tookMs: performance.now() - started,
      lines,
      syntax: resolved,
      theme: "github-light",
      from: 0,
      more: false,
      supported: true,
    };
    self.postMessage(response);
    self.postMessage({ id, ok: true, log: `worker 已投递结果，${Math.round(performance.now() - started)} ms` });
  } catch (error) {
    const response: WorkerResponse = {
      id,
      ok: false,
      error: error instanceof Error ? error.message : String(error),
    };
    self.postMessage(response);
  }
};
