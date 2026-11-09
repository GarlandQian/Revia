import {readFileSync,existsSync,writeFileSync} from 'node:fs';
import {join,basename} from 'node:path';
import {createHash,createPublicKey,verify} from 'node:crypto';
import assert from 'node:assert/strict';
const [directory,keyFile,version]=process.argv.slice(2);
const decode=(text)=>text.startsWith('untrusted comment:')?text:Buffer.from(text.trim(),'base64').toString('utf8');
const pkLines=decode(readFileSync(keyFile,'utf8')).trim().split(/\r?\n/);
const pk=Buffer.from(pkLines[1],'base64');assert.equal(pk.length,42);
const key=createPublicKey({key:Buffer.concat([Buffer.from('302a300506032b6570032100','hex'),pk.subarray(10)]),format:'der',type:'spki'});
const results=[];
const sha=(name)=>createHash('sha256').update(readFileSync(join(directory,name))).digest('hex');
for(const sums of ['macos-SHA256SUMS.txt','windows-SHA256SUMS.txt']){
 if(!existsSync(join(directory,sums)))continue;
 for(const line of readFileSync(join(directory,sums),'utf8').trim().split(/\r?\n/)){
  const bsd=/^SHA256 \((.+)\) = ([0-9a-f]{64})$/.exec(line);
  const gnu=/^([0-9a-f]{64}) [ *](.+)$/.exec(line);
  assert.ok(bsd||gnu,'checksum line syntax');
  const name=bsd?bsd[1]:gnu[2],digest=bsd?bsd[2]:gnu[1];
  assert.equal(basename(name),name);assert.equal(sha(name),digest,name);
  results.push({name,sha256:digest});
 }
}
const manifest=JSON.parse(readFileSync(join(directory,'latest.json'),'utf8'));
assert.equal(manifest.version,version);
for(const [platform,entry] of Object.entries(manifest.platforms)){
 const url=new URL(entry.url);assert.equal(url.origin,'https://github.com');
 assert.ok(url.pathname.startsWith('/GarlandQian/Revia/releases/download/v'+version+'/'));
 const name=decodeURIComponent(basename(url.pathname));assert.equal(basename(name),name);
 const encoded=readFileSync(join(directory,name+'.sig'),'utf8').trim();
 assert.equal(entry.signature,encoded);
 const lines=decode(encoded).trim().split(/\r?\n/);assert.ok(lines[2].startsWith('trusted comment: '));
 assert.ok(lines[2].slice(17).split('\t').includes('version:'+version),'signed version must match manifest');
 const sig=Buffer.from(lines[1],'base64');assert.equal(sig.length,74);
 assert.deepEqual(sig.subarray(2,10),pk.subarray(2,10));
 const algorithm=sig.subarray(0,2).toString('ascii');assert.ok(['Ed','ED'].includes(algorithm));
 const bytes=readFileSync(join(directory,name));
 const signed=algorithm==='ED'?createHash('blake2b512').update(bytes).digest():bytes;
 assert.equal(verify(null,signed,key,sig.subarray(10)),true,'payload signature: '+name);
 assert.equal(verify(null,Buffer.concat([sig.subarray(10),Buffer.from(lines[2].slice(17))]),key,Buffer.from(lines[3],'base64')),true,'trusted comment signature: '+name);
 results.push({platform,name,payloadSignature:true,trustedCommentSignature:true,trustedComment:lines[2].slice(17)});
}
if(version==="0.0.3"){
 assert.deepEqual(Object.keys(manifest.platforms).sort(),['darwin-aarch64','windows-x86_64']);
 assert.deepEqual(readFileSync(join(directory,'latest-prerelease.json')),readFileSync(join(directory,'latest.json')));
}
writeFileSync(join(directory,'verification.json'),JSON.stringify(results,null,2));
console.log(JSON.stringify(results,null,2));
