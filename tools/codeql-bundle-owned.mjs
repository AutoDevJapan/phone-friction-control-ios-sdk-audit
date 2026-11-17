import fs from 'node:fs';
import crypto from 'node:crypto';
import {spawn} from 'node:child_process';
const jobs=new WeakMap(),leases=new WeakMap(),references=new WeakMap();let current;
const fail=s=>{throw Error(s);};
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const id=s=>({device:String(s.dev),inode:String(s.ino)});
const equal=(a,b)=>a.device===b.device&&a.inode===b.inode;
function held(fd,kind,root=false){const s=fs.fstatSync(fd,{bigint:true});if(!(kind==='directory'?s.isDirectory():s.isFile())||(root&&(s.uid!==0n||(s.mode&0o022n)!==0n)))fail('native_job_object');return {...id(s),fd,kind};}
function checked(token){const j=jobs.get(token);if(!j||j.closed||j.closing)fail('native_job_unavailable');for(const r of j.objects)if(!equal(r,held(r.fd,r.kind,r.root)))fail('native_job_object_changed');return j;}
function readFD(fd,limit){const before=fs.fstatSync(fd,{bigint:true});if(!before.isFile()||before.size<1n||before.size>BigInt(limit))fail('native_input_size');const b=Buffer.alloc(Number(before.size));let n=0;while(n<b.length){const got=fs.readSync(fd,b,n,b.length-n,n);if(!got)fail('native_input_short');n+=got;}if(fs.readSync(fd,Buffer.alloc(1),0,1,n)!==0)fail('native_input_grew');const after=fs.fstatSync(fd,{bigint:true});if(before.dev!==after.dev||before.ino!==after.ino||before.size!==after.size||before.mtimeNs!==after.mtimeNs||before.ctimeNs!==after.ctimeNs)fail('native_input_changed');return b;}
function lineReader(stream){let pending=[],queue=[],text='',ended=false,error;stream.setEncoding('utf8');const flush=()=>{while(pending.length&&(queue.length||ended||error)){const p=pending.shift();clearTimeout(p.timer);if(error)p.reject(error);else if(queue.length)p.resolve(queue.shift());else p.reject(Error('job_input_eof'));}};stream.on('data',part=>{text+=part;if(text.length>32768){error=Error('job_input_size');flush();return;}let n;while((n=text.indexOf('\n'))>=0){const line=text.slice(0,n);text=text.slice(n+1);queue.push(line);}flush();});stream.once('end',()=>{ended=true;flush();});stream.on('error',e=>{error=e;flush();});return()=>new Promise((resolve,reject)=>{const p={resolve,reject,timer:setTimeout(()=>{pending=pending.filter(x=>x!==p);reject(Error('job_input_deadline'));},20*60*1000)};pending.push(p);flush();});}
export async function openJob(){
 if(current)fail('native_job_already_open');if(typeof process==='undefined'||process.versions.node.split('.')[0]!=='24'||typeof process.getuid!=='function'||process.getuid()!==0||process.ppid!==1||process.env.SDK_ROOT_JOB!=='1')fail('native_bootstrap_unavailable');
 const roles=[[3,'directory',false],[5,'file',true],[6,'directory',true],[7,'file',true],[8,'directory',true],[9,'file',true],[10,'directory',true],[12,'file',true],[15,'directory',true]].map(([fd,kind,root])=>({...held(fd,kind,root),root}));
 const exe=fs.openSync('/proc/self/exe',fs.constants.O_RDONLY);try{if(!equal(id(fs.fstatSync(exe,{bigint:true})),roles.find(r=>r.fd===7)))fail('job_node_image_changed');}finally{fs.closeSync(exe);}
 const manifest=JSON.parse(readFD(9,1024*1024).toString('utf8'));
 if(manifest.schema!=='sdk-codeql-root-build/1'||manifest.compilerExit!==0||manifest.testsExit!==0||manifest.databaseCleanupExit!==0||manifest.cleanupScopeDrained!==true||!equal(manifest.cleanupDatabase,roles.find(r=>r.fd===3))||manifest.compiler?.epochsDrained!==true||!equal(manifest.image,roles.find(r=>r.fd===5))||!equal(manifest.database,roles.find(r=>r.fd===3)))fail('job_build_binding');
 const readLine=lineReader(process.stdin),message=JSON.parse(await readLine());if(message.Kind!=='bootstrap'||!/^[0-9a-f]{32}$/.test(message.Nonce)||message.Token)fail('job_bootstrap_protocol');
 const token=Object.freeze({});jobs.set(token,{objects:roles,db:roles[0],image:roles[1],diagnostics:roles[2],source:roles[4],stage:roles[6],official:roles[7],manifest,nonce:message.Nonce,readLine,closed:false,prepared:false,tokenReceived:false,active:new Set(),references:new Set()});current=token;return token;
}
export function currentJob(){if(!current)fail('native_bootstrap_unavailable');checked(current);return current;}
export function databaseObject(job){const j=checked(job);return {fd:j.db.fd,ownerPID:process.pid,device:j.db.device,inode:j.db.inode};}
export function evidenceDirectory(job){checked(job);return '/proc/'+process.pid+'/fd/6';}
export function officialInput(job){const j=checked(job);return {bytes:readFD(j.official.fd,8*1024*1024),filename:fs.realpathSync('/proc/self/fd/12')};}
export function jobManifest(job){return checked(job).manifest;}
export function announcePrepared(job,reference){const j=checked(job);if(!references.has(reference)||references.get(reference).job!==job||j.prepared)fail('native_prepared_reference');j.prepared=true;fs.writeSync(1,JSON.stringify({Kind:'prepared',Nonce:j.nonce})+'\n');}
export async function receivePublicationToken(job){const j=checked(job);if(!j.prepared||j.tokenReceived)fail('job_token_phase');const m=JSON.parse(await j.readLine());if(m.Kind!=='token'||m.Nonce!==j.nonce||typeof m.Token!=='string'||!m.Token||m.Token.length>16384)fail('job_token_protocol');j.tokenReceived=true;return m.Token;}
export function announceFinished(job,publication){const j=checked(job);if(!j.prepared||!j.tokenReceived)fail('job_finish_phase');fs.writeSync(1,JSON.stringify({Kind:'finished',Nonce:j.nonce,Publication:publication===true})+'\n');}
function errorFor(l,reason){const e=Error(reason);e.producer={ownerPID:l.child?.pid??null,rootExitObserved:l.exited,rootCloseObserved:l.closed,exitCode:l.code,signal:l.signal,stdoutEnd:l.outEnd,stdoutClose:l.outClose,stderrEnd:l.errEnd,stderrClose:l.errClose,bytes:l.bytes,phase:l.phase,cleanup:l.cleanup??{Verified:false,status:'native_cleanup_unverified'}};e.producerStderr=Buffer.concat(l.stderr).toString('utf8');return e;}
function parseOutput(l){const b=Buffer.concat(l.chunks);if(b.length<5)fail('native_frame_short');const n=b.readUInt32BE(0);if(n<2||n>16384||b.length<4+n)fail('native_frame_header');const h=JSON.parse(b.subarray(4,4+n).toString('utf8'));if(h.version!==1||h.Nonce!==l.nonce||h.OwnerPID!==l.child.pid||h.RootExit!==0||h.AllReaped!==true||h.LateWriter!==false||h.FileHeld!==true||h.CleanupVerified!==false||h.SourceDevice!==l.root.device||h.SourceInode!==l.root.inode||h.CopyGraphSHA256!==l.graphSHA256||!Number.isSafeInteger(h.Bytes)||h.Bytes<1||h.Bytes>l.limit||b.length!==4+n+h.Bytes||!/^[0-9a-f]{64}$/.test(h.SHA256)||!/^\d+$/.test(h.OutputDevice)||!/^\d+$/.test(h.OutputInode))fail('native_frame_binding');const body=Buffer.from(b.subarray(4+n));if(sha(body)!==h.SHA256)fail('native_frame_hash');return {body,header:h};}
export async function produceBundle({job,database,graphSHA256,limit,deadline}){
 const j=checked(job);if(j.active.size||database!==j.db.fd||!/^[0-9a-f]{64}$/.test(graphSHA256??'')||!Number.isSafeInteger(limit)||limit<1||!Number.isFinite(deadline)||deadline<=Date.now())fail('native_producer_arguments');
 const nonce=crypto.randomBytes(16).toString('hex'),token=Object.freeze({});let acquireResolve,acquireReject,completeResolve;
 const acquired=new Promise((a,b)=>{acquireResolve=a;acquireReject=b;}),complete=new Promise(a=>{completeResolve=a;});
 const l={job,child:null,nonce,root:j.db,graphSHA256,limit,deadline,chunks:[],stderr:[],bytes:0,stderrBytes:0,outEnd:false,outClose:false,errEnd:false,errClose:false,exited:false,closed:false,code:null,signal:null,phase:'launch',acquired:false,acked:false,committed:false,bound:false,error:null,complete,cleanup:null};leases.set(token,l);j.active.add(token);
 l.deadlineTimer=setTimeout(()=>{l.error??=errorFor(l,'native_output_deadline');acquireReject(l.error);},Math.max(1,deadline-Date.now()));
 const finish=()=>{if(!l.closed||!l.outClose||!l.errClose)return;clearTimeout(l.deadlineTimer);const text=Buffer.concat(l.stderr).toString('utf8');const clean=[...text.matchAll(/^native_owner_cleanup=(.+)$/gm)].map(m=>{try{return JSON.parse(m[1]);}catch{return null;}}).filter(x=>x?.Nonce===nonce);if(clean.length===1)l.cleanup=clean[0];if(!l.acquired){l.error??=errorFor(l,'native_owner_before_output');acquireReject(l.error);}completeResolve();};
 const tryAcquire=()=>{if(l.acquired||l.error||!l.outEnd||!l.outClose)return;try{if(l.exited)fail('native_owner_closed_before_ack');const p=parseOutput(l);l.header=p.header;l.acquired=true;l.phase='member-verification';l.chunks=[];acquireResolve({bytes:p.body,evidence:{...p.header,scope:'provisional owned output FD remains held until ACK'},authority:token});}catch(e){l.error=errorFor(l,e.message);acquireReject(l.error);}};
 try{
  l.child=spawn('/proc/self/fd/5',[],{env:{PATH:'/usr/bin:/bin'},windowsHide:true,stdio:['pipe','pipe','pipe',j.db.fd,'ignore',j.image.fd,'ignore','ignore','ignore','ignore',j.stage.fd,'ignore','ignore','ignore','ignore',j.objects.find(r=>r.fd===15).fd]});
  l.child.stdout.on('data',b=>{l.bytes+=b.length;if(l.bytes>limit+16388){l.error??=errorFor(l,'native_frame_size');acquireReject(l.error);}else l.chunks.push(Buffer.from(b));});l.child.stderr.on('data',b=>{l.stderrBytes+=b.length;if(l.stderrBytes>1024*1024){l.error??=errorFor(l,'native_stderr_size');acquireReject(l.error);}else l.stderr.push(Buffer.from(b));});
  l.child.stdout.once('end',()=>{l.outEnd=true;});l.child.stdout.once('close',()=>{l.outClose=true;tryAcquire();finish();});l.child.stderr.once('end',()=>{l.errEnd=true;});l.child.stderr.once('close',()=>{l.errClose=true;finish();});
  for(const s of [l.child.stdout,l.child.stderr])s.on('error',()=>{l.error??=errorFor(l,'native_stream_error');acquireReject(l.error);});
  l.child.once('error',()=>{l.error??=errorFor(l,'native_spawn');acquireReject(l.error);});l.child.once('exit',(code,signal)=>{l.exited=true;l.code=code;l.signal=signal;l.child.stdin.destroy();});l.child.once('close',(code,signal)=>{l.closed=true;l.code=code;l.signal=signal;finish();});
  l.child.stdin.on('error',()=>{});await new Promise((a,b)=>l.child.stdin.write(JSON.stringify({Nonce:nonce,RemainingMillis:Math.min(240000,Math.floor(deadline-Date.now()))})+'\n',e=>e?b(e):a()));
  return await acquired;
 }catch(e){l.error=e.producer?e:errorFor(l,'native_launch_failed');if(l.child)await abortBundle(job,token,l.error);else{clearTimeout(l.deadlineTimer);j.active.delete(token);}throw l.error;}
}
async function ack(l,kind){if(l.acked)return;l.acked=true;await new Promise((a,b)=>l.child.stdin.write(JSON.stringify({Nonce:l.nonce,Kind:kind,SHA256:l.header?.SHA256??''})+'\n',e=>e?b(e):a()));l.child.stdin.end();}
async function waitComplete(l){let timer;try{await Promise.race([l.complete,new Promise((_,reject)=>{timer=setTimeout(()=>reject(errorFor(l,'native_finish_deadline')),Math.max(1,l.deadline-Date.now()));})]);}finally{clearTimeout(timer);}}
export async function finishBundle(job,token){const j=checked(job),l=leases.get(token);if(!l||l.job!==job||!l.acquired||l.error||l.committed||l.phase!=='member-verification'||l.acked)fail('native_lease_unknown');await ack(l,'accept');await waitComplete(l);const finals=[...Buffer.concat(l.stderr).toString('utf8').matchAll(/^native_owner_final=(.+)$/gm)].map(m=>{try{return JSON.parse(m[1]);}catch{return null;}}).filter(x=>x?.Nonce===l.nonce);if(l.error||l.code!==0||l.signal||!l.outEnd||!l.errEnd||!l.exited||finals.length!==1||finals[0].CleanupVerified!==true||finals[0].Accepted!==true)throw errorFor(l,'native_finish_failed');checked(job);l.committed=true;l.phase='committed';j.active.delete(token);return {ownerPID:l.child.pid,header:l.header,cleanup:l.cleanup};}
function leaseDrained(l){return l.exited&&l.closed&&l.outEnd&&l.outClose&&l.errEnd&&l.errClose;}
export async function abortBundle(job,token,primary){
 const j=jobs.get(job),l=leases.get(token);if(!j||!l||l.job!==job)fail('native_lease_unknown');
 if(l.committed){j.active.delete(token);return{drained:true,committed:true,cleanup:l.cleanup};}
 let ackFailure,waitFailure;try{await ack(l,'reject');}catch(e){ackFailure=e;}
 // A broken ACK does not prove termination. Still observe the owned child and
 // all of its own pipe closures before retiring this lease from the owner.
 try{await waitComplete(l);}catch(e){waitFailure=e;}
 const drained=leaseDrained(l);l.phase=drained?'closed-failure':'failure-unreaped';
 const diagnostic=errorFor(l,drained?'native_consumer_failed':'native_abort_unreaped');
 diagnostic.producer.abort={ackFailed:!!ackFailure,waitFailed:!!waitFailure,drained};
 if(primary)Object.assign(primary,{producer:diagnostic.producer,producerStderr:diagnostic.producerStderr,nativeAbort:diagnostic.producer.abort});
 if(drained){j.active.delete(token);return{drained:true,cleanup:l.cleanup,producer:diagnostic.producer};}
 // Ownership and original FDs remain live. The outer PID-namespace supervisor
 // receives an explicit failed Node exit; it must reap before stage cleanup.
 if(!primary)throw diagnostic;return{drained:false,producer:diagnostic.producer};
}
export function bindPrepared(job,token,reference){checked(job);const l=leases.get(token);if(!l||l.job!==job||!l.committed||l.phase!=='committed'||l.bound)fail('native_authority_unissued');l.bound=true;jobs.get(job).references.add(reference);references.set(reference,{job,lease:token});}
export function referenceValid(job,reference){checked(job);const r=references.get(reference),l=r?leases.get(r.lease):null;return r?.job===job&&l?.committed===true&&l.bound===true;}
export function releasePrepared(reference){const r=references.get(reference);if(r)jobs.get(r.job)?.references.delete(reference);references.delete(reference);}
export async function closeJob(job){
 const j=jobs.get(job);if(!j||j.closed)return{drained:true,closed:true};
 const failures=[];for(const token of [...j.active]){try{await abortBundle(job,token);}catch(e){failures.push(e.producer??{status:'unreaped'});}}
 if(j.active.size||j.references.size||failures.length){const e=Error('native_job_unreaped');e.producer={activeLeases:j.active.size,failures,cleanup:{Verified:false,status:'outer_namespace_reap_required'}};throw e;}
 j.closing=true;let closeFailure=false;const closeRecords=[];
 for(const fd of [3,5,6,7,8,9,10,11,12,15]){try{fs.closeSync(fd);closeRecords.push({fd,closed:true});}catch{closeFailure=true;closeRecords.push({fd,closed:false});}}
 if(closeFailure){j.closeRecords=closeRecords;const e=Error('native_job_close_unverified');e.producer={activeLeases:0,closeRecords,cleanup:{Verified:false,status:'process_exit_required'}};throw e;}
 j.closed=true;if(typeof process!=='undefined')process.stdin.destroy();current=null;return{drained:true,closed:true};
}
