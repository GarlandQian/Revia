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
 *   REVIA_UPDATE_PUBKEY     updater 公钥（minisign 公钥文本）
 *   REVIA_UPDATE_ENDPOINT   latest.json 的完整 URL
 *   RELEASE_VERSION         应用版本（用于确认产物名对得上）
 *
 * 私钥只在构建环境里通过 TAURI_SIGNING_PRIVATE_KEY 提供，绝不写进这个文件。
 */
import { readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));
const pubkey = (process.env.REVIA_UPDATE_PUBKEY ?? "").trim();
const endpoint = (process.env.REVIA_UPDATE_ENDPOINT ?? "").trim();

if (!pubkey) fail("REVIA_UPDATE_PUBKEY is required");
if (!/^https:\/\//.test(endpoint)) fail("REVIA_UPDATE_ENDPOINT must be an https URL");
if (!pubkey.includes("untrusted comment:")) {
  fail("REVIA_UPDATE_PUBKEY does not look like a minisign public key");
}

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
console.log(`  pubkey ${pubkey.split("\n")[1]?.slice(0, 16) ?? "(unreadable)"}…`);

function fail(message) {
  console.error(`updater-config: ${message}`);
  process.exit(1);
}
