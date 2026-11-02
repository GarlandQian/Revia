#!/usr/bin/env node
/**
 * 核对 Windows 便携单 exe 的最终依赖（M8 任务 06）。
 *
 * Rust MSVC 默认动态 CRT；仓库通过 `.cargo/config.toml` 的
 * `-C target-feature=+crt-static` 静态链接 CRT，让便携版只依赖系统 WebView2。
 * 这个脚本拒绝需要额外安装的 VC++ 运行库与动态 WebView2Loader。
 * Windows 10/11 的系统 UCRT（ucrtbase / api-ms-win-crt）属于 OS，本身不是额外安装依赖。
 * https://learn.microsoft.com/en-us/cpp/windows/universal-crt-deployment?view=msvc-170
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

if (dlls.length === 0) {
  console.error("windows-deps: no imported DLLs found; expected successful dumpbin /dependents output");
  process.exit(1);
}

// 保持拒绝额外 VC++ Redistributable（含 _1、debug、atomic_wait 等变体）。系统 UCRT 允许。
const forbidden = dlls.filter((name) =>
  /^(?:(?:vcruntime|msvcp|msvcr)\d.*|webview2loader)\.dll$/i.test(name),
);

if (forbidden.length > 0) {
  console.error(`windows-deps: portable executable still depends on: ${[...new Set(forbidden)].join(", ")}`);
  process.exit(1);
}

console.log(`windows-deps: ${dlls.length} imported DLL(s), no extra VC++ runtime or WebView2 loader`);
