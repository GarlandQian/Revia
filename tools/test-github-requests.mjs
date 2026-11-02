import assert from "node:assert/strict";
import test from "node:test";
import { GithubRequests, githubResponse } from "../src/lib/m7/githubRequests.ts";
const deferred = () => { let resolve, reject; const promise = new Promise((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
for (const change of ["worktree", "account", "remote", "repository-url", "PR"]) {
  test(`${change}切换后，旧成功/错误/finally都不能覆盖新目标`, async () => {
    const scope = new GithubRequests(); scope.select("A");
    const ok = deferred(), failed = deferred(), calls = [];
    const a = githubResponse(scope.begin("success"), () => ok.promise, () => calls.push("old-success"), () => calls.push("old-error"), () => calls.push("old-finish"));
    const b = githubResponse(scope.begin("failure"), () => failed.promise, () => calls.push("old-success"), () => calls.push("old-error"), () => calls.push("old-finish"));
    scope.select(change);
    await githubResponse(scope.begin("success"), async () => "new", (result) => calls.push(result), () => calls.push("new-error"), () => calls.push("new-finish"));
    ok.resolve("old"); failed.reject(Error("old")); await Promise.all([a, b]);
    assert.deepEqual(calls, ["new", "new-finish"]);
  });
}
test("同目标同类请求只接受最新，不阻塞独立的评论和diff通道", async () => {
  const scope = new GithubRequests(); scope.select("account/remote/PR"); const late = deferred(), calls = [];
  const old = githubResponse(scope.begin("diff"), () => late.promise, () => calls.push("old"), () => calls.push("old-error"), () => calls.push("old-finish"));
  await githubResponse(scope.begin("comments"), async () => "comments", (v) => calls.push(v), assert.fail);
  await githubResponse(scope.begin("diff"), async () => "new-diff", (v) => calls.push(v), assert.fail);
  late.resolve("old"); await old; assert.deepEqual(calls, ["comments", "new-diff"]);
});
test("刷新同一PR版本与卸载使已有后续读/写回失效，A→B→A也不认领旧A", async () => {
  const scope = new GithubRequests(); scope.select("A"); const original = scope.begin("review");
  scope.select("B"); scope.select("A"); assert.equal(original.current(), false);
  const refresh = scope.begin("detail"); scope.invalidate(); assert.equal(refresh.current(), false);
  const unmount = scope.begin("comment"); scope.invalidate(); let called = false; unmount.apply(() => { called = true; }); assert.equal(called, false);
});
test("当前失败仍可见，finished只执行一次；回调抛错也经过当前错误路径", async () => {
  const scope = new GithubRequests(), calls = [];
  await githubResponse(scope.begin("delete"), async () => { throw Error("fixture IO"); }, assert.fail, (e) => calls.push(e.message), () => calls.push("finish"));
  assert.deepEqual(calls, ["fixture IO", "finish"]);
});
