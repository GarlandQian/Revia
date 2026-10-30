#!/usr/bin/env node
/**
 * 校验版本标签与源码版本一致（M7 任务 12）。
 *
 * 应用版本出现在三处：package.json、src-tauri/tauri.conf.json、src-tauri/Cargo.toml。
 * 标签、产物名、更新 URL 都以这个版本为准；不一致时构建出来的包会指向错误版本，
 * 所以发布前先在这里失败，而不是等 updater 拿不到包才发现。
 *
 * 用法（CI 或本地）：
 *   RELEASE_TAG=v0.1.0 RELEASE_REPOSITORY=owner/repo node tools/check-release-version.mjs
 *   node tools/check-release-version.mjs --tag v0.1.0-beta.1 --repository owner/repo
 *
 * 输出（写入 $GITHUB_OUTPUT，未在 Actions 中时打印到标准输出）：
 *   version  0.1.0           应用版本（不含 v）
 *   channel  stable|prerelease
 *   endpoint updater 检查地址
 */
import { appendFileSync, readFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = dirname(dirname(fileURLToPath(import.meta.url)));

function arg(name) {
  const index = process.argv.indexOf(`--${name}`);
  return index >= 0 ? process.argv[index + 1] : undefined;
}

const tag = process.env.RELEASE_TAG ?? arg("tag") ?? "";
const repository = process.env.RELEASE_REPOSITORY ?? arg("repository") ?? "";

if (!tag) fail("RELEASE_TAG is required (for example v0.1.0)");
if (!repository || !repository.includes("/")) fail("RELEASE_REPOSITORY must look like owner/repo");

const pkg = JSON.parse(readFileSync(join(root, "package.json"), "utf8"));
const tauri = JSON.parse(readFileSync(join(root, "src-tauri/tauri.conf.json"), "utf8"));
const cargo = readFileSync(join(root, "src-tauri/Cargo.toml"), "utf8");
const cargoVersion = cargo.match(/^version\s*=\s*"([^"]+)"/m)?.[1];

const tagVersion = tag.replace(/^v/, "");
const versions = { tag: tagVersion, package: pkg.version, tauri: tauri.version, cargo: cargoVersion };
for (const [source, value] of Object.entries(versions)) {
  if (!value) fail(`Could not read the version from ${source}`);
}
const unique = new Set(Object.values(versions));
if (unique.size !== 1) {
  fail(`Version mismatch: ${Object.entries(versions).map(([source, value]) => `${source}=${value}`).join(", ")}`);
}
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(tagVersion)) fail(`Tag ${tag} is not a semver version`);

// 带预发布后缀的标签进预发布渠道；两套渠道各自指向自己的更新清单。
const channel = tagVersion.includes("-") ? "prerelease" : "stable";
const endpoint =
  channel === "stable"
    ? `https://github.com/${repository}/releases/latest/download/latest.json`
    : `https://github.com/${repository}/releases/download/${tag}/latest.json`;

const outputs = { version: tagVersion, channel, endpoint };
const githubOutput = process.env.GITHUB_OUTPUT;
if (githubOutput) {
  appendFileSync(githubOutput, Object.entries(outputs).map(([key, value]) => `${key}=${value}\n`).join(""));
} else {
  for (const [key, value] of Object.entries(outputs)) console.log(`${key}=${value}`);
}
console.log(`Version ${tagVersion} (${channel}) -> ${endpoint}`);

function fail(message) {
  console.error(`release-version: ${message}`);
  process.exit(1);
}
