#!/usr/bin/env node
/**
 * 生成只用于构建的 updater 配置覆盖文件（M7 任务 12）。
 *
 * tauri.conf.json 里不带公钥和检查地址：没有配置时应用明确显示“不可更新”，
 * 而不是拿占位值冒充已上线（规格 H）。发布构建时由这个脚本把 CI 注入的公钥
 * 与地址写进 `src-tauri/tauri.release.conf.json`（构建产物，不进版本库），
 * 再用 `--config` 覆盖过去。
 *
 * 输入（环境变量）：
 *   REVIA_UPDATE_PUBKEY     updater 公钥。接受两种格式（F15 修正）：
 *                           - minisign 公钥文件全文（untrusted comment + base64 两行）；
 *                           - 纯 STANDARD base64 串（tauri-plugin-updater 实际要求的编码）。
 *   REVIA_UPDATE_ENDPOINT   latest.json 的完整 URL
 *   RELEASE_VERSION         应用版本（用于确认产物名对得上）
 *
 * 私钥只在构建环境里通过 TAURI_SIGNING_PRIVATE_KEY 提供，绝不写进这个文件。
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const pubkeyRaw = (process.env.REVIA_UPDATE_PUBKEY ?? "").trim();
const endpoint = (process.env.REVIA_UPDATE_ENDPOINT ?? "").trim();

if (!pubkeyRaw) fail("REVIA_UPDATE_PUBKEY is required");
if (!/^https:\/\//.test(endpoint)) fail("REVIA_UPDATE_ENDPOINT must be an https URL");

/**
 * F15：tauri-plugin-updater（锁定 2.14.1）在验签前先对配置里的 pubkey 做
 * STANDARD base64 解码，再交给 minisign `PublicKey::decode`。所以配置里必须写
 * base64 编码的公钥正文，而不是带注释的 minisign 原文。这里同时接受两种输入，
 * 统一转换成 base64，写错格式在 CI 上立即失败而不是等客户端验签时才发现。
 */
function normalizePubkey(input) {
  const lines = input.split("\n").map((line) => line.trim()).filter(Boolean);
  if (lines[0]?.startsWith("untrusted comment:")) {
    // minisign 公钥文件全文：第二行是 base64。解码后校验它确实是 minisign 公钥
    //（以 "untrusted comment: " 开头，第二行才是 RW 开头的密钥体），再重新编码。
    if (lines.length < 2) fail("REVIA_UPDATE_PUBKEY minisign key file is missing the key line");
    const keyLine = lines[1];
    const decoded = Buffer.from(keyLine, "base64");
    const text = decoded.toString("utf8");
    if (!text.startsWith("untrusted comment:")) {
      fail("REVIA_UPDATE_PUBKEY second line does not decode to a minisign public key");
    }
    return Buffer.from(text, "utf8").toString("base64");
  }
  // 纯 base64 输入：解码确认是 minisign 公钥再原样通过。
  const decoded = Buffer.from(input, "base64");
  if (!decoded.toString("utf8").startsWith("untrusted comment:")) {
    fail("REVIA_UPDATE_PUBKEY does not decode to a minisign public key");
  }
  return input.replace(/\s+/g, "");
}

const pubkey = normalizePubkey(pubkeyRaw);

const conf = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"));
const version = process.env.RELEASE_VERSION?.trim();
if (version && version !== conf.version) {
  fail(`RELEASE_VERSION ${version} does not match tauri.conf.json ${conf.version}`);
}

const overlay = {
  plugins: {
    updater: {
      pubkey,
      endpoints: [endpoint],
      // 稳定版默认；预发布由标签里的后缀决定，用户仍需在设置里主动开启。
      windows: { installMode: "passive" },
    },
  },
  bundle: {
    // updater 产物（.app.tar.gz + .sig）与 dmg 一起生成。
    createUpdaterArtifacts: true,
  },
};

const target = join(root, "src-tauri/tauri.release.conf.json");
writeFileSync(target, `${JSON.stringify(overlay, null, 2)}\n`);
console.log(`wrote ${target}`);
console.log(`  version ${conf.version}`);
console.log(`  endpoint ${endpoint}`);
console.log(`  pubkey base64 ${pubkey.slice(0, 16)}…`);

function fail(message) {
  console.error(`updater-config: ${message}`);
  process.exit(1);
}
