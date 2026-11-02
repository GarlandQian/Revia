import assert from "node:assert/strict";
import test from "node:test";
import { ReviewDraftSessions } from "../src/lib/m7/reviewDraftSessions.ts";
const deferred=()=>{let resolve,reject;const promise=new Promise((yes,no)=>{resolve=yes;reject=no;});return {promise,resolve,reject};};
const tick=()=>new Promise((resolve)=>setImmediate(resolve));
const bundle=(key="A",headSha="h1")=>({scope:{key:`${key}/${headSha}`,targetKey:key,worktreeId:key,login:"fixture",remote:"origin",number:1,headSha},drafts:[],older:[]});
function fixture(){
 const rows=new Map(),calls=[];let effect=async()=>{};
 const deps={save:async(scope,draft)=>{calls.push({scope,draft});await effect(scope,draft);const saved={...draft,id:`${scope.key}/${draft.kind}/${draft.kind==="inline"?`${draft.path}/${draft.line}`:""}`,scopeKey:scope.key,targetKey:scope.targetKey,headSha:scope.headSha,number:scope.number,updatedAtMs:draft.updatedAtMs+1};rows.set(saved.id,saved);return saved;},remove:async(scope,id)=>{assert.equal(rows.get(id)?.scopeKey,scope.key);rows.delete(id);}};
 return {sessions:new ReviewDraftSessions(deps),deps,rows,calls,setSave(fn){effect=fn;}};
}
test("评论、审阅正文、行内表单切PR仍在，重建会话读回实际存稿",async()=>{
 const f=fixture(),a=f.sessions.open(bundle());a.change("commentBody",{body:"comment A"});a.change("reviewBody",{body:"review A",side:"approve"});a.change("inlineForm",{body:"inline A",path:"a.ts",line:12});
 const b=f.sessions.open(bundle("B"));b.change("reviewBody",{body:"review B"});await f.sessions.flush();
 assert.equal(f.sessions.open(bundle()),a);assert.equal(a.snapshot().forms.commentBody.body,"comment A");assert.equal(a.snapshot().forms.inlineForm.line,12);assert.equal(b.snapshot().forms.reviewBody.body,"review B");
 const restarted=new ReviewDraftSessions(f.deps).open({...bundle(),drafts:[...f.rows.values()].filter(d=>d.scopeKey==="A/h1")});assert.equal(restarted.snapshot().forms.reviewBody.side,"approve");assert.equal(restarted.snapshot().forms.inlineForm.body,"inline A");
});
test("同PR不同head分开恢复，旧稿只能查看不进入新inline发送集合",async()=>{
 const f=fixture(),old=f.sessions.open(bundle());old.change("reviewBody",{body:"old body"});old.change("inlineForm",{body:"old anchor",path:"a.ts"});await old.addInline();await old.flush();
 const current=f.sessions.open({...bundle("A","h2"),older:[...f.rows.values()]});assert.equal(current.snapshot().forms.reviewBody.body,"");assert.equal(current.snapshot().inline.length,0);assert.ok(current.snapshot().older.some(d=>d.body==="old body"));assert.equal(f.sessions.open(bundle()),old);
});
test("串行保存防止旧输入晚到覆盖新正文，隐藏会话仍保护退出直到落盘",async()=>{
 const f=fixture(),a=f.sessions.open(bundle()),gate=deferred();let first=true;f.setSave(async()=>{if(first){first=false;await gate.promise;}});
 a.change("reviewBody",{body:"one"});a.change("reviewBody",{body:"two"});await tick();assert.equal(f.calls.length,1);assert.equal(f.sessions.hasWork,true);
 let closed=false;const closing=f.sessions.flush().then(()=>{closed=true;});await tick();assert.equal(closed,false);gate.resolve();await closing;
 assert.equal(f.calls.length,2);assert.equal(a.snapshot().forms.reviewBody.body,"two");assert.equal(f.sessions.hasWork,false);assert.equal([...f.rows.values()][0].body,"two");
});
test("存储失败保留原文并阻止全局冲刷，修复后显式冲刷当前稿",async()=>{
 const f=fixture(),a=f.sessions.open(bundle());f.setSave(async()=>{throw Error("fixture readonly");});a.change("commentBody",{body:"do not lose"});await tick();assert.equal(a.snapshot().forms.commentBody.body,"do not lose");assert.ok(a.snapshot().error);assert.equal(f.sessions.hasWork,true);await assert.rejects(f.sessions.flush());
 f.setSave(async()=>{});await f.sessions.flush();assert.equal(f.sessions.hasWork,false);assert.equal([...f.rows.values()][0].body,"do not lose");
});
test("发布期间新输入不被成功回包清空，只清实际发送的原快照",async()=>{
 const f=fixture(),a=f.sessions.open(bundle());a.change("reviewBody",{body:"sent"});await a.flush();const sent=a.snapshot().forms.reviewBody;
 a.change("reviewBody",{body:"new while sending"});await a.published("reviewBody",sent);assert.equal(a.snapshot().forms.reviewBody.body,"new while sending");
 const current=a.snapshot().forms.reviewBody;await a.published("reviewBody",current);assert.equal(a.snapshot().forms.reviewBody.body,"");assert.equal([...f.rows.values()][0].body,"");
});
test("异步添加行内草稿只清它对应的表单快照，不丢期间新输入",async()=>{
 const f=fixture(),a=f.sessions.open(bundle());a.change("inlineForm",{body:"first",path:"a.ts"});await a.flush();const gate=deferred();f.setSave(async(_,d)=>{if(d.kind==="inline")await gate.promise;});
 const adding=a.addInline();await tick();a.change("inlineForm",{body:"second"});gate.resolve();await adding;await a.flush();assert.equal(a.snapshot().inline[0].body,"first");assert.equal(a.snapshot().forms.inlineForm.body,"second");await a.removeInline(a.snapshot().inline[0].id);assert.equal(a.snapshot().inline.length,0);
});
test("装入旧存稿不会先写空正文；全局原生退出保护随队列通知",async()=>{
 const f=fixture(),seen=[];f.sessions.subscribe(()=>seen.push(f.sessions.hasWork));const a=f.sessions.open(bundle());assert.equal(f.calls.length,0);a.change("reviewBody",{body:"text"});await f.sessions.flush();assert.ok(seen.includes(true));assert.equal(seen.at(-1),false);
});

test("较早自动保存失败后，新输入成功即覆盖旧失败保护，不会永久卡住退出",async()=>{
 const f=fixture(),a=f.sessions.open(bundle());let first=true;f.setSave(async()=>{if(first){first=false;throw Error("first failed");}});
 a.change("reviewBody",{body:"one"});a.change("reviewBody",{body:"latest"});await tick();
 assert.equal(a.snapshot().forms.reviewBody.body,"latest");assert.equal(f.sessions.hasWork,false);assert.equal([...f.rows.values()][0].body,"latest");await f.sessions.flush();assert.equal(f.calls.length,2);
});
