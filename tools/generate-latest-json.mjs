#!/usr/bin/env node
/**
 * 生成 tauri-plugin-updater 使用的 latest.json 更新清单（M7 任务 12、F14）。
 *
 * Tauri updater 请求配置的 endpoint，按 Static Format 读取：
 *   { version, notes, pub_date, platforms: { "darwin-aarch64": { url, signature } } }
 * signature 是 .sig 文件内容（minisign 全文）再做 STANDARD base64 编码——插件验签前
 * 先 base64 解码 signature 与 pubkey（见锁定插件 updater.rs 的 verify_signature）。
 *
 * 输入：
 *   --artifacts <dir>        收集好的产物目录（.app.tar.gz 与 .app.tar.gz.sig）
 *   --download-base <url>    产物在 Release 里的下载地址前缀（https）
 *   --version <v>            应用版本（不带 v 前缀）
 *   --notes <file|->         Release notes 文本或路径（可选）
 * 输出：stdout 打印 latest.json 内容；由 CI 写入文件并上传到 Release。
 */
import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";

function arg(name) {
  const index = process.argv.indexOf(`--${name}`);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

const artifactsDir = arg("artifacts");
const downloadBase = (arg("download-base") ?? "").replace(/\/$/, "");
const version = arg("version") ?? "";

if (!artifactsDir || !version) fail("--artifacts and --version are required");
if (!downloadBase.startsWith("https://")) fail("--download-base must be an https URL");

const files = readdirSync(artifactsDir);
const updaterPackage = files.find((name) => name.endsWith(".app.tar.gz") && !name.endsWith(".sig"));
const signatureFile = updaterPackage && files.find((name) => name === `${updaterPackage}.sig`);
if (!updaterPackage || !signatureFile) {
  fail("artifacts directory must contain *.app.tar.gz and its .sig");
}

// minisign .sig 文件全文 → STANDARD base64（插件 base64_to_string 后交给 Signature::decode）。
const signatureText = readFileSync(join(artifactsDir, signatureFile), "utf8").trim();
const signatureBase64 = Buffer.from(signatureText, "utf8").toString("base64");

const notesArg = arg("notes");
const notes = notesArg && notesArg !== "-" ? readFileSync(notesArg, "utf8") : null;

// Tauri macOS Apple Silicon 的 updater target（wry/tauri 约定 os-arch）。
const manifest = {
  version,
  notes,
  pub_date: new Date().toISOString(),
  platforms: {
    "darwin-aarch64": {
      // GitHub Release 资产的 latest/download/<名称> 地址；名称必须与上传产物一致。
      signature: signatureBase64,
      url: `${downloadBase}/${encodeURIComponent(updaterPackage)}`,
    },
  },
};

process.stdout.write(`${JSON.stringify(manifest, null, 2)}\n`);

function fail(message) {
  console.error(`latest-json: ${message}`);
  process.exit(1);
}
