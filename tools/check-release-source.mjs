#!/usr/bin/env node
/** Release tags must point to commits in main's history (Q203). Checkout must fetch main. */
import { spawnSync } from "node:child_process";

function git(...args) {
  const result = spawnSync("git", args, { encoding: "utf8" });
  if (result.error) fail(result.error.message);
  return result;
}

const commit = git("rev-parse", "--verify", "HEAD^{commit}");
if (commit.status !== 0) fail("Could not resolve the release commit");
const main = git("rev-parse", "--verify", "refs/remotes/origin/main^{commit}");
if (main.status !== 0) fail("origin/main is missing; fetch full history before checking release source");
const source = commit.stdout.trim();
const mainHead = main.stdout.trim();
if (git("merge-base", "--is-ancestor", source, mainHead).status !== 0) {
  fail(`Release commit ${source} is not in origin/main; merge to main and tag the main commit`);
}
console.log(`Release source ${source} is in origin/main (${mainHead})`);

function fail(message) {
  console.error(`release-source: ${message}`);
  process.exit(1);
}
