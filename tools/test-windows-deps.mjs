import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import test from "node:test";
import { fileURLToPath } from "node:url";

const script = fileURLToPath(new URL("./check-windows-deps.mjs", import.meta.url));

function check(text) {
  const dir = mkdtempSync(join(tmpdir(), "revia-deps-"));
  const file = join(dir, "deps.txt");
  writeFileSync(file, text);
  return spawnSync(process.execPath, [script, file], { encoding: "utf8" });
}

test("静态 CRT 与静态 WebView2 loader 的系统依赖通过", () => {
  const result = check(`
    KERNEL32.dll
    USER32.dll
    ADVAPI32.dll
    OLE32.dll
    SHELL32.dll
    WebView2LoaderStatic.lib
  `);
  assert.equal(result.status, 0, result.stderr);
  assert.match(result.stdout, /no dynamic CRT or WebView2 loader/);
});

test("动态 CRT 被拒绝", () => {
  const result = check("KERNEL32.dll\nVCRUNTIME140.dll\nMSVCP140.dll\n");
  assert.equal(result.status, 1);
  assert.match(result.stderr, /VCRUNTIME140\.dll/);
  assert.match(result.stderr, /MSVCP140\.dll/);
});

test("动态 WebView2Loader.dll 被拒绝", () => {
  const result = check("KERNEL32.dll\nWebView2Loader.dll\n");
  assert.equal(result.status, 1);
  assert.match(result.stderr, /WebView2Loader\.dll/);
});
