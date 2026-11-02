import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("./generate-latest-json.mjs", import.meta.url));
const signature = Buffer.from("untrusted comment: fixture\nRWRldmlh").toString("base64");

function fixture({ windows = true } = {}) {
  const root = mkdtempSync(join(tmpdir(), "revia-manifest-"));
  const mac = join(root, "mac");
  const win = join(root, "win");
  mkdirSync(mac);
  mkdirSync(win);
  writeFileSync(join(mac, "Revia.app.tar.gz"), "mac package");
  writeFileSync(join(mac, "Revia.app.tar.gz.sig"), signature);
  if (windows) {
    writeFileSync(join(win, "Revia_1.2.3_x64-setup.exe"), "windows package");
    writeFileSync(join(win, "Revia_1.2.3_x64-setup.exe.sig"), signature);
  }
  return { root, mac, win };
}

function run(args) {
  return spawnSync(process.execPath, [script, ...args], { encoding: "utf8" });
}

test("同时发布 macOS 与 Windows 时生成包含两个平台的清单", () => {
  const { mac, win } = fixture();
  const result = run([
    "--artifacts", mac,
    "--windows-artifacts", win,
    "--require-windows",
    "--download-base", "https://github.com/owner/repo/releases/download/v1.2.3",
    "--version", "1.2.3",
  ]);
  assert.equal(result.status, 0, result.stderr);
  const manifest = JSON.parse(result.stdout);
  assert.equal(manifest.version, "1.2.3");
  assert.deepEqual(Object.keys(manifest.platforms).sort(), ["darwin-aarch64", "windows-x86_64"]);
  assert.match(manifest.platforms["windows-x86_64"].url, /Revia_1\.2\.3_x64-setup\.exe$/);
  assert.equal(manifest.platforms["windows-x86_64"].signature, signature);
  assert.match(manifest.platforms["darwin-aarch64"].url, /Revia\.app\.tar\.gz$/);
});

test("要求 Windows 产物但缺失时失败，不产出不完整清单", () => {
  const { mac, win } = fixture({ windows: false });
  const result = run([
    "--artifacts", mac,
    "--windows-artifacts", win,
    "--require-windows",
    "--download-base", "https://github.com/owner/repo/releases/download/v1.2.3",
    "--version", "1.2.3",
  ]);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Windows NSIS updater package and signature are required/);
});

test("Windows 安装包版本与标签不一致时失败", () => {
  const { mac, win } = fixture();
  const result = run([
    "--artifacts", mac,
    "--windows-artifacts", win,
    "--require-windows",
    "--download-base", "https://github.com/owner/repo/releases/download/v9.9.9",
    "--version", "9.9.9",
  ]);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /does not contain version 9\.9\.9/);
});

test("Windows 签名不是 Tauri 生成的 base64 时失败", () => {
  const { mac, win } = fixture();
  writeFileSync(join(win, "Revia_1.2.3_x64-setup.exe.sig"), "not-a-signature");
  const result = run([
    "--artifacts", mac,
    "--windows-artifacts", win,
    "--require-windows",
    "--download-base", "https://github.com/owner/repo/releases/download/v1.2.3",
    "--version", "1.2.3",
  ]);
  assert.notEqual(result.status, 0);
  assert.match(result.stderr, /Windows signature file must contain the base64 signature/);
});
