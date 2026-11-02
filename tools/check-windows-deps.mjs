#!/usr/bin/env node
/**
 * 核对 Windows 便携单 exe 的最终依赖（M8 任务 06）。
 *
 * Rust MSVC 默认动态 CRT；仓库通过 `.cargo/config.toml` 的
 * `-C target-feature=+crt-static` 静态链接 CRT，让便携版只依赖系统 WebView2。
 * 这个脚本读取 `dumpbin /dependents` 的输出，发现 VC++ 运行库或动态
 * WebView2Loader 就失败；Windows runner 上预装开发工具不能当作普通用户环境。
 *
 * 用法：dumpbin /dependents Revia.exe > deps.txt && node tools/check-windows-deps.mjs deps.txt
 */
import { readFileSync } from "node:fs";

const path = process.argv[2];
if (!path) {
  console.error("windows-deps: expected a dumpbin /dependents output file");
  process.exit(2);
}

const text = readFileSync(path, "utf8");
const dlls = [...text.matchAll(/\b([A-Za-z0-9_.-]+\.dll)\b/gi)].map((match) => match[1]);

// 动态 CRT 与动态 WebView2 loader 都会破坏“解压即用、只依赖系统 WebView2”。
const forbidden = dlls.filter((name) =>
  /^(vcruntime\d+|msvcp\d+|msvcr\d+|ucrtbase|api-ms-win-crt-[a-z0-9-]+|webview2loader)\.dll$/i.test(name),
);

if (forbidden.length > 0) {
  console.error(`windows-deps: portable executable still depends on: ${[...new Set(forbidden)].join(", ")}`);
  process.exit(1);
}

console.log(`windows-deps: ${dlls.length} imported DLL(s), no dynamic CRT or WebView2 loader`);
