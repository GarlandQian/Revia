import assert from "node:assert/strict";
import test from "node:test";
import { CloseCoordinator, CloseDriver } from "../src/lib/refreshGate.ts";
import { MergeBufferGuards } from "../src/lib/m7/mergeBuffers.ts";
import { updateCandidate, UpdateInstallConfirmation } from "../src/lib/m7/updateState.ts";
const tick=()=>new Promise((resolve)=>setImmediate(resolve));
const deferred=()=>{let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no;});return{promise,resolve,reject};};
function fixture(){
 const coordinator=new CloseCoordinator(),calls=[],errors=[],asked=[];let final=async(kind)=>{calls.push(kind);};
 const driver=new CloseDriver({coordinator,releaseExitGuard:async()=>{calls.push("prepare");},closeWindow:()=>final("close"),exitApp:()=>final("exit"),installUpdate:()=>final("install"),restartUpdate:()=>final("restart"),onError:e=>errors.push(e),onAsk:(intent,writing,pending)=>asked.push({intent,writing,pending}),onSettled:()=>{}});
 return{coordinator,driver,calls,errors,asked,setFinal(fn){final=fn;}};
}
for(const [intent,action]of[["installUpdate","install"],["restartUpdate","restart"]]){
 test(`${action}复用原写门，等待既有Git写，不会投递close/exit`,async()=>{
  const f=fixture();f.coordinator.beginWrite("A");f.driver.handleRequest(intent);assert.equal(f.asked.at(-1).intent,intent);f.driver.handleChoice("wait");await tick();assert.deepEqual(f.calls,[]);
  f.coordinator.endWrite("A",false);f.driver.notifyStateChanged();await tick();assert.deepEqual(f.calls,["prepare",action]);assert.equal(f.driver.currentPhase(),intent==="installUpdate"?"idle":"closing");assert.equal(f.driver.canStartWrite(),intent==="installUpdate");
 });
 test(`${action}选择留在应用不继续，较早请求不会因后来写完复活`,async()=>{
  const f=fixture();f.coordinator.beginWrite("A");f.driver.handleRequest(intent);f.driver.handleChoice("stay");f.coordinator.endWrite("A",false);f.driver.notifyStateChanged();await tick();assert.deepEqual(f.calls,[]);assert.equal(f.driver.currentPhase(),"idle");
 });
}
test("安装执行期间锁住所有新写入口，成功后回到应用，重启仍需独立请求",async()=>{
 const f=fixture(),gate=deferred();f.setFinal(async(kind)=>{f.calls.push(kind);await gate.promise;});f.driver.handleRequest("installUpdate");await tick();assert.equal(f.driver.currentPhase(),"closing");assert.equal(f.driver.canStartWrite(),false);f.driver.handleRequest("restartUpdate");assert.deepEqual(f.calls,["prepare","install"]);gate.resolve();await tick();assert.equal(f.driver.currentPhase(),"idle");assert.equal(f.driver.canStartWrite(),true);f.driver.handleRequest("restartUpdate");await tick();assert.deepEqual(f.calls,["prepare","install","prepare","restart"]);
});
test("安装失败保留应用并解除本次生命周期；不自动重发安装",async()=>{
 const f=fixture();f.setFinal(async()=>{throw Error("fixture install failed");});f.driver.handleRequest("installUpdate");await tick();assert.equal(f.errors.length,1);assert.equal(f.driver.currentPhase(),"idle");f.driver.notifyStateChanged();await tick();assert.equal(f.errors.length,1);
});
test("结果待核对时安装不发生，必须真实核对后继续原请求",async()=>{
 const f=fixture();f.coordinator.beginWrite("A");f.coordinator.endWrite("A",true);f.driver.handleRequest("installUpdate");f.driver.handleChoice("wait");await tick();assert.deepEqual(f.calls,[]);assert.deepEqual(f.asked.at(-1).pending,["A"]);
 const generation=f.coordinator.pendingGeneration("A");assert.equal(f.coordinator.acknowledgePending("A",generation),true);f.driver.handlePendingContinue();await tick();assert.deepEqual(f.calls,["prepare","install"]);
});
test("旧安装回执不遮新可用版本；真实缓存恢复下载态，重启后的旧回执不再要求重启",()=>{
 const base={available:null,downloadedVersion:null,receipt:{version:"1.0.0"},restartRequired:false};
 assert.deepEqual(updateCandidate({...base,available:{version:"2.0.0"}}),{version:"2.0.0",downloaded:false,pending:false,restartVersion:null});
 assert.deepEqual(updateCandidate({...base,available:{version:"2.0.0"},downloadedVersion:"2.0.0"}),{version:"2.0.0",downloaded:true,pending:false,restartVersion:null});
 assert.equal(updateCandidate({...base,downloadedVersion:"2.0.0"}).downloaded,true);
 assert.equal(updateCandidate({...base,available:{version:"3.0.0"},downloadedVersion:"2.0.0"}).downloaded,false);
 assert.equal(updateCandidate({...base,restartRequired:true}).restartVersion,"1.0.0");
 assert.equal(updateCandidate({...base,available:{version:"2.0.0"},pendingInstall:{version:"2.0.0",startedAtMs:1}}).pending,true);
});

test("合并未应用内容按原编辑器阻止生命周期，释放另一编辑器不清掉它",()=>{
 const guards=new MergeBufferGuards(),observed=[];guards.subscribe(()=>observed.push(guards.hasWork));guards.mark("A/path/fp1");guards.mark("B/path/fp2");guards.clear("B/path/fp2");assert.equal(guards.hasWork,true);guards.clear("A/path/fp1");assert.equal(guards.hasWork,false);assert.deepEqual(observed,[true,true,true,false]);
});

test("Windows 安装先确认安装并重启，取消不进入生命周期；macOS 安装后仍单独确认重启", () => {
  const windows = new UpdateInstallConfirmation(true);
  assert.deepEqual(windows.request("installUpdate"), { kind: "confirm", intent: "installUpdate" });
  assert.equal(windows.waiting, true);
  windows.cancel();
  assert.equal(windows.confirm(), null);
  assert.equal(windows.waiting, false);

  assert.deepEqual(windows.request("installUpdate"), { kind: "confirm", intent: "installUpdate" });
  assert.equal(windows.confirm(), "installUpdate");
  assert.equal(windows.waiting, false);
  assert.deepEqual(windows.request("restartUpdate"), { kind: "start", intent: "restartUpdate" });

  const mac = new UpdateInstallConfirmation(false);
  assert.deepEqual(mac.request("installUpdate"), { kind: "start", intent: "installUpdate" });
  assert.deepEqual(mac.request("restartUpdate"), { kind: "start", intent: "restartUpdate" });
  assert.equal(mac.waiting, false);
});

test("Windows 确认后的安装进入既有 CloseDriver 草稿/写入/终端生命周期", async () => {
  const gate = new UpdateInstallConfirmation(true);
  gate.request("installUpdate");
  const intent = gate.confirm();
  assert.equal(intent, "installUpdate");
  const f = fixture();
  f.driver.handleRequest(intent);
  await tick();
  assert.deepEqual(f.calls, ["prepare", "install"]);
  assert.equal(f.driver.currentPhase(), "idle");
  assert.equal(f.driver.canStartWrite(), true);
});
