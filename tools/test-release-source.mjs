import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { mkdtempSync, realpathSync, writeFileSync } from "node:fs";
import { devNull, tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import test from "node:test";

const script = fileURLToPath(new URL("./check-release-source.mjs", import.meta.url));
function fixture(t) {
  const cwd = realpathSync(mkdtempSync(join(tmpdir(), "revia-release-source-")));
  t.diagnostic(`fixture: ${cwd}`);
  const env = { ...process.env, GIT_CONFIG_NOSYSTEM: "1", GIT_CONFIG_GLOBAL: devNull };
  const git = (...args) => {
    const result = spawnSync("git", args, { cwd, env, encoding: "utf8" });
    assert.equal(result.status, 0, result.stderr);
    return result.stdout.trim();
  };
  const commit = (text) => {
    writeFileSync(join(cwd, "fixture.txt"), text);
    git("add", "fixture.txt");
    git("-c", "user.name=Revia Fixture", "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false", "commit", "-m", text);
    return git("rev-parse", "HEAD");
  };
  git("init", "-b", "main");
  const base = commit("baseline");
  return { git, commit, base, check: () => spawnSync(process.execPath, [script], { cwd, env, encoding: "utf8" }) };
}

test("main tag is accepted; a side-branch tag is rejected", (t) => {
  const f = fixture(t);
  f.git("update-ref", "refs/remotes/origin/main", f.base);
  f.git("tag", "v0.0.2");
  f.git("checkout", "--detach", "v0.0.2");
  assert.equal(f.check().status, 0);
  f.git("switch", "-c", "m7");
  f.commit("unmerged change");
  const rejected = f.check();
  assert.equal(rejected.status, 1);
  assert.match(rejected.stderr, /not in origin\/main/);
});

test("a tagged main ancestor stays valid after main advances", (t) => {
  const f = fixture(t);
  const latest = f.commit("later main change");
  f.git("update-ref", "refs/remotes/origin/main", latest);
  f.git("checkout", "--detach", f.base);
  assert.equal(f.check().status, 0);
});

test("missing main history fails closed", (t) => {
  const f = fixture(t);
  const rejected = f.check();
  assert.equal(rejected.status, 1);
  assert.match(rejected.stderr, /origin\/main is missing/);
});
