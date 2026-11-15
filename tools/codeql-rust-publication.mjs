import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import zlib from 'node:zlib';
import https from 'node:https';
import {Readable} from 'node:stream';
import {execFileSync} from 'node:child_process';
import {pathToFileURL} from 'node:url';
import {OFFICIAL,consumeOfficialSarif,canonicalizeLocations,checkOfficialBundle} from './codeql-official-owned.mjs';
import {produceBundle} from './codeql-bundle-owned.mjs';

export const LIMITS=Object.freeze({archive:32*1024*1024,bundle:512*1024*1024,sarif:64*1024*1024,input:8*1024*1024,inventory:1024*1024,sourceTotal:64*1024*1024,members:20000,memberNames:8*1024*1024,chunk:64*1024});
const crcTable=Uint32Array.from({length:256},(_,n)=>{let c=n;for(let i=0;i<8;i++)c=(c>>>1)^((c&1)?0xedb88320:0);return c>>>0;});
const expectedRepo='AutoDevJapan/phone-friction-control-ios-sdk-audit',expectedId='1413129506',expectedBranch='main';
const owned=new WeakMap();
const fail=reason=>{throw Error(reason);};
const hash=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function takeOwnership(bytes){const ref=Object.freeze({bytes:bytes.length,sha256:hash(bytes)});owned.set(ref,bytes);return ref;}
function raw(ref){const bytes=owned.get(ref);if(!bytes)fail('snapshot_unknown');return bytes;}
export function copySnapshot(bytes,limit){if(!Buffer.isBuffer(bytes)||bytes.length<1||bytes.length>limit)fail('snapshot_size');return takeOwnership(Buffer.from(bytes));}
export function captureFile(file,limit,io=fs){
  let fd;
  try{
    fd=io.openSync(file,io.constants.O_RDONLY|io.constants.O_NOFOLLOW);
    const before=io.fstatSync(fd);
    if(!before.isFile()||!Number.isSafeInteger(before.size)||before.size<1||before.size>limit)fail('snapshot_size');
    const bytes=Buffer.alloc(before.size);let position=0;
    while(position<bytes.length){const n=io.readSync(fd,bytes,position,bytes.length-position,position);if(n<1)fail('snapshot_truncated');position+=n;}
    const tail=Buffer.alloc(1);if(io.readSync(fd,tail,0,1,position)!==0)fail('snapshot_grew');
    const after=io.fstatSync(fd);
    if(before.size!==after.size||before.dev!==after.dev||before.ino!==after.ino||before.mtimeMs!==after.mtimeMs||before.ctimeMs!==after.ctimeMs)fail('snapshot_changed_during_capture');
    return takeOwnership(bytes);
  }finally{if(fd!==undefined)io.closeSync(fd);}
}
function jsonSnapshot(file,limit,io=fs){const snap=captureFile(file,limit,io);return {snapshot:snap,value:JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(raw(snap)))};}
function crc32(bytes){let crc=0xffffffff;for(const byte of bytes){crc^=byte;for(let bit=0;bit<8;bit++)crc=(crc>>>1)^((crc&1)?0xedb88320:0);}return(crc^0xffffffff)>>>0;}
export function openZip(snapshot){
  const bytes=raw(snapshot),n=bytes.length;
  const u16=p=>bytes.readUInt16LE(p),u32=p=>bytes.readUInt32LE(p);
  let end=-1;for(let p=n-22;p>=Math.max(0,n-65557);p--)if(u32(p)===0x06054b50&&p+22+u16(p+20)===n){end=p;break;}
  if(end<0)fail('zip_end');
  let count=u16(end+10),size=u32(end+12),offset=u32(end+16),directoryEnd=end;
  const integer64=p=>{const value=bytes.readBigUInt64LE(p);if(value>BigInt(Number.MAX_SAFE_INTEGER))fail('zip_integer');return Number(value);};
  if(end>=20&&u32(end-20)===0x07064b50){
    if(u32(end-16)!==0||u32(end-4)!==1)fail('zip64_disks');
    const position=integer64(end-12);
    if(position+56>end-20||u32(position)!==0x06064b50||integer64(position+4)<44||position+12+integer64(position+4)!==end-20||u32(position+16)||u32(position+20))fail('zip64_end');
    count=integer64(position+32);if(integer64(position+24)!==count)fail('zip64_count');
    size=integer64(position+40);offset=integer64(position+48);directoryEnd=position;
  }else if(count===0xffff||size===0xffffffff||offset===0xffffffff)fail('zip64_missing');
  if(u16(end+4)||u16(end+6)||u16(end+8)!==u16(end+10)||count<1||count>LIMITS.members||offset+size!==directoryEnd||size>LIMITS.memberNames)fail('zip_directory');
  const entries=new Map(),ranges=[];let p=offset,totalNames=0;
  for(let i=0;i<count;i++){
    if(p+46>end||u32(p)!==0x02014b50)fail('zip_member');
    const flags=u16(p+8),method=u16(p+10),crc=u32(p+16),nameSize=u16(p+28),extra=u16(p+30),comment=u16(p+32),disk=u16(p+34),external=u32(p+38);let compressed=u32(p+20),expanded=u32(p+24),local=u32(p+42);
    if(p+46+nameSize+extra+comment>end||nameSize<1||nameSize>4096||disk||flags&1||![0,8].includes(method))fail('zip_member_kind');
    if(compressed===0xffffffff||expanded===0xffffffff||local===0xffffffff){
      let cursor=p+46+nameSize,found=false;
      const extraEnd=cursor+extra;
      while(cursor+4<=extraEnd){
        const tag=u16(cursor),length=u16(cursor+2),start=cursor+4;cursor=start+length;if(cursor>extraEnd)fail('zip_extra_bounds');
        if(tag!==1)continue;if(found)fail('zip64_extra_duplicate');found=true;let q=start;
        const next=()=>{if(q+8>cursor)fail('zip64_extra_size');const value=integer64(q);q+=8;return value;};
        if(expanded===0xffffffff)expanded=next();if(compressed===0xffffffff)compressed=next();if(local===0xffffffff)local=next();
      }if(!found)fail('zip64_extra_missing');
    }
    const name=new TextDecoder('utf-8',{fatal:true}).decode(bytes.subarray(p+46,p+46+nameSize));
    if(name.includes('\0')||name.includes('\\')||entries.has(name)||((external>>>16)&0xf000)===0xa000)fail('zip_member_name');
    totalNames+=nameSize;if(totalNames>LIMITS.memberNames)fail('zip_name_budget');
    if(local+30>offset||u32(local)!==0x04034b50||u16(local+6)!==flags||u16(local+8)!==method)fail('zip_local');
    const localNameSize=u16(local+26),localExtra=u16(local+28),start=local+30+localNameSize+localExtra;
    if(start+compressed>offset||new TextDecoder('utf-8',{fatal:true}).decode(bytes.subarray(local+30,local+30+localNameSize))!==name)fail('zip_local_bounds');
    ranges.push([local,start+compressed]);
    entries.set(name,Object.freeze({name,method,crc,compressed,expanded,start}));
    p+=46+nameSize+extra+comment;
  }
  if(p!==offset+size)fail('zip_directory_size');
  ranges.sort((a,b)=>a[0]-b[0]);for(let i=1;i<ranges.length;i++)if(ranges[i][0]<ranges[i-1][1])fail('zip_overlap');
  return Object.freeze({
    names:Object.freeze([...entries.keys()]),
    async memberHash(name,expected,deadline){
      const entry=entries.get(name);if(!entry||!Number.isSafeInteger(expected)||expected<0||entry.expanded!==expected)fail('zip_entry_size');
      const packed=bytes.subarray(entry.start,entry.start+entry.compressed),digest=crypto.createHash('sha256');
      const source=Readable.from((function*(){for(let i=0;i<packed.length;i+=LIMITS.chunk)yield packed.subarray(i,Math.min(packed.length,i+LIMITS.chunk));})(),{objectMode:false});
      const decoder=entry.method===8?zlib.createInflateRaw():source;
      const closed=stream=>new Promise(resolve=>{stream.once('close',resolve);stream.on('error',()=>{});});
      const sourceClosed=closed(source),decoderClosed=decoder===source?sourceClosed:closed(decoder);
      let count=0,crc=0xffffffff;
      try{
        if(decoder!==source)source.pipe(decoder);
        for await(const chunk of decoder){
          if(Date.now()>deadline)fail('database_capture_deadline');
          count+=chunk.length;if(count>expected)fail('zip_entry_size');digest.update(chunk);
          for(const b of chunk)crc=(crc>>>8)^crcTable[(crc^b)&255];
        }
        if(count!==expected||((crc^0xffffffff)>>>0)!==entry.crc)fail('zip_entry_crc_or_size');
        return {bytes:count,sha256:digest.digest('hex')};
      }finally{
        source.destroy();if(decoder!==source)decoder.destroy();
        let timer;try{await Promise.race([Promise.all([sourceClosed,decoderClosed]),new Promise((_,reject)=>{timer=setTimeout(()=>reject(Error('zip_stream_drain')),2000);})]);}finally{clearTimeout(timer);}
      }
    },
    member(name,max){
      const entry=entries.get(name);if(!entry||entry.expanded>max)fail('zip_entry_size');
      const packed=bytes.subarray(entry.start,entry.start+entry.compressed);
      const body=entry.method===0?Buffer.from(packed):zlib.inflateRawSync(packed,{maxOutputLength:Math.max(1,max)});
      if(body.length!==entry.expanded||crc32(body)!==entry.crc)fail('zip_entry_crc_or_size');
      return takeOwnership(body);
    }
  });
}
export function canonicalRustPath(name){
  return typeof name==='string'&&Buffer.from(name,'utf8').toString('utf8')===name&&name.endsWith('.rs')&&!/^[\/]/.test(name)&&!/[\x00-\x1f\x7f\\:]/.test(name)&&name.split('/').every(part=>part&&part!=='.'&&part!=='..');
}
export function verifySource(snapshot,inputs){
  if(!Array.isArray(inputs)||inputs.length<1||inputs.length>LIMITS.members||new Set(inputs.map(i=>i.path)).size!==inputs.length)fail('source_inventory');
  const zip=openZip(snapshot);
  for(const input of inputs){
    if(!canonicalRustPath(input.path)||!Number.isSafeInteger(input.bytes)||input.bytes<0||input.bytes>LIMITS.input||!/^[0-9a-f]{64}$/.test(input.sha256))fail('source_inventory_item');
    const matches=zip.names.filter(name=>name===input.path||name.endsWith('/'+input.path));
    if(matches.length!==1)fail('source_missing_or_duplicate');
    const body=zip.member(matches[0],LIMITS.input);
    if(body.bytes!==input.bytes||body.sha256!==input.sha256)fail('source_byte_mismatch');
  }
  return Object.freeze({bytes:snapshot.bytes,sha256:snapshot.sha256});
}
const databaseGraphs=new WeakMap();
const preparedFlights=new WeakMap();
function safeDatabaseName(name){return typeof name==='string'&&name&&!name.includes('\\')&&!name.includes('\0')&&!name.split('/').some(p=>!p||p==='.'||p==='..');}
function fileDigest(file,io,deadline){
  let fd;
  try{
    fd=io.openSync(file,io.constants.O_RDONLY|io.constants.O_NOFOLLOW);
    const before=io.fstatSync(fd);if(!before.isFile()||!Number.isSafeInteger(before.size)||before.size<0||before.size>8*1024**3)fail('database_file_size');
    const digest=crypto.createHash('sha256'),chunk=Buffer.alloc(LIMITS.chunk);let position=0;
    while(position<before.size){if(Date.now()>deadline)fail('database_capture_deadline');const n=io.readSync(fd,chunk,0,Math.min(chunk.length,before.size-position),position);if(!n)fail('database_file_truncated');digest.update(chunk.subarray(0,n));position+=n;}
    if(io.readSync(fd,chunk,0,1,position)!==0)fail('database_file_grew');
    const after=io.fstatSync(fd);
    if(before.size!==after.size||before.dev!==after.dev||before.ino!==after.ino||before.mtimeMs!==after.mtimeMs||before.ctimeMs!==after.ctimeMs)fail('database_file_changed');
    return Object.freeze({bytes:position,sha256:digest.digest('hex'),device:before.dev,inode:before.ino});
  }finally{if(fd!==undefined)io.closeSync(fd);}
}
function scanDatabase(graph){
  const files=new Map(),directories=new Set(['']);let total=0;
  const walk=(directoryFD,relative,depth)=>{
    if(depth>64||directories.size>4096||Date.now()>graph.deadline)fail('database_tree_budget');
    const location='/proc/'+graph.ownerPID+'/fd/'+directoryFD,before=graph.io.fstatSync(directoryFD);
    if(!before.isDirectory())fail('database_directory_kind');
    for(const name of graph.io.readdirSync(location).sort()){
      if(typeof name!=='string'||name.includes('/')||!safeDatabaseName(name))fail('database_member_name');
      const child=relative?relative+'/'+name:name,file=path.posix.join(location,name),stat=graph.io.lstatSync(file);
      if(stat.isSymbolicLink())fail('database_symlink');
      if(stat.isDirectory()){
        let fd;
        try{
          fd=graph.io.openSync(file,graph.io.constants.O_RDONLY|graph.io.constants.O_DIRECTORY|graph.io.constants.O_NOFOLLOW);
          const held=graph.io.fstatSync(fd);
          if(!held.isDirectory()||held.dev!==stat.dev||held.ino!==stat.ino)fail('database_directory_changed');
          directories.add(child);walk(fd,child,depth+1);
        }finally{if(fd!==undefined)graph.io.closeSync(fd);}
      }else if(stat.isFile()){
        if(files.size>=LIMITS.members)fail('database_tree_budget');
        const captured=fileDigest(file,graph.io,graph.deadline);total+=captured.bytes;
        if(total>8*1024**3)fail('database_tree_budget');files.set(child,captured);
      }else fail('database_member_kind');
    }
    const after=graph.io.fstatSync(directoryFD);
    if(before.dev!==after.dev||before.ino!==after.ino||before.mtimeMs!==after.mtimeMs||before.ctimeMs!==after.ctimeMs)fail('database_directory_changed');
  };
  walk(graph.fd,'',0);
  if(!files.has('src.zip')||!files.has('codeql-database.yml')||![...files.keys()].some(n=>n.startsWith('db-rust/')))fail('database_components');
  return {files,directories,total};
}
export function captureDatabaseGraph(database,{io=fs,ownerPID,deadline=Date.now()+240000}={}){
  if(!Number.isInteger(ownerPID)||ownerPID<1)fail('database_owner_identity');
  let fd;
  try{
    fd=io.openSync(database,io.constants.O_RDONLY|io.constants.O_DIRECTORY|io.constants.O_NOFOLLOW);
    const root=io.fstatSync(fd);if(!root.isDirectory())fail('database_root_kind');
    const graph={io,fd,ownerPID,anchor:'/proc/'+ownerPID+'/fd/'+fd,deadline,root,closed:false};
    const snapshot=scanDatabase(graph);
    const records=[...snapshot.files].map(([name,value])=>({name,bytes:value.bytes,sha256:value.sha256})).sort((a,b)=>a.name<b.name?-1:a.name>b.name?1:0);
    const graphSHA256=hash(Buffer.from(JSON.stringify(records)));
    const token=Object.freeze({files:snapshot.files.size,totalBytes:snapshot.total,graphSHA256,rootDevice:root.dev,rootInode:root.ino});
    databaseGraphs.set(token,{...graph,...snapshot,graphSHA256});fd=undefined;return token;
  }finally{if(fd!==undefined)io.closeSync(fd);}
}
export function closeDatabaseGraph(token){
  const graph=databaseGraphs.get(token);if(!graph||graph.closed)return;
  graph.closed=true;graph.io.closeSync(graph.fd);databaseGraphs.delete(token);
}
function checkedGraph(token){const graph=databaseGraphs.get(token);if(!graph||graph.closed)fail('database_graph_unknown');return graph;}
function assertDatabaseUnchanged(token){
  const graph=checkedGraph(token),now=scanDatabase(graph);
  if(now.files.size!==graph.files.size||now.directories.size!==graph.directories.size)fail('database_changed');
  for(const [name,record] of graph.files){const current=now.files.get(name);if(!current||current.bytes!==record.bytes||current.sha256!==record.sha256)fail('database_changed');}
  for(const name of graph.directories)if(!now.directories.has(name))fail('database_changed');
}
export async function verifyBundle(snapshot,sourceProof,inputs,databaseToken){
  const graph=checkedGraph(databaseToken),zip=openZip(snapshot);
  const expectedSource=graph.files.get('src.zip');
  if(expectedSource.sha256!==sourceProof.sha256||expectedSource.bytes!==sourceProof.bytes)fail('database_source_mismatch');
  const source=zip.member('rust/src.zip',LIMITS.archive);
  if(source.bytes!==sourceProof.bytes||source.sha256!==sourceProof.sha256)fail('bundle_source_mismatch');
  verifySource(source,inputs);
  const found=new Set();
  for(const name of zip.names){
    if(name==='rust/'){zip.member(name,0);continue;}
    if(!name.startsWith('rust/'))fail('bundle_database_namespace');
    const relative=name.slice(5);
    if(relative.endsWith('/')){
      const directory=relative.slice(0,-1);if(!graph.directories.has(directory))fail('bundle_extra_directory');zip.member(name,0);continue;
    }
    if(!safeDatabaseName(relative)||found.has(relative)||!graph.files.has(relative))fail('bundle_extra_database_member');
    const expected=graph.files.get(relative),actual=await zip.memberHash(name,expected.bytes,graph.deadline);
    if(actual.bytes!==expected.bytes||actual.sha256!==expected.sha256){const error=Error('bundle_database_bytes');error.database={memberSHA256:hash(Buffer.from(relative)),expectedBytes:expected.bytes,actualBytes:actual.bytes,expectedSHA256:expected.sha256,actualSHA256:actual.sha256};throw error;}
    found.add(relative);
  }
  for(const name of graph.files.keys())if((name==='src.zip'||name==='codeql-database.yml'||name==='base-database-oids.json'||name.startsWith('db-rust/'))&&!found.has(name))fail('bundle_missing_database_member');
  return {bytes:snapshot.bytes,sha256:snapshot.sha256,databaseFiles:found.size,capturedDatabaseFiles:graph.files.size,capturedDatabaseSHA256:graph.graphSHA256,databaseBytes:graph.total,source_archive:{bytes:source.bytes,sha256:source.sha256}};
}
export function eligibility(env,event,head){
  if(env.DATABASE_PUBLISH!=='true')fail('publication_disabled');
  if(env.GITHUB_SERVER_URL!=='https://github.com'||env.GITHUB_REPOSITORY!==expectedRepo||env.GITHUB_REPOSITORY_ID!==expectedId)fail('repository_identity');
  const name=env.GITHUB_EVENT_NAME;
  if(!['push','workflow_dispatch','schedule','pull_request'].includes(name))fail('event_unknown');
  const repo=event?.repository;
  if(name!=='schedule'&&(!repo||repo.full_name!==expectedRepo||String(repo.id)!==expectedId||repo.default_branch!==expectedBranch))fail('event_repository_identity');
  if(repo&&((repo.full_name!==undefined&&repo.full_name!==expectedRepo)||(repo.id!==undefined&&String(repo.id)!==expectedId)||(repo.default_branch!==undefined&&repo.default_branch!==expectedBranch)))fail('event_repository_identity');
  if(name==='pull_request')return {eligible:false,reason:'pull_request'};
  // schedule's documented ref is its default branch even when the event has no repository body.
  if(env.GITHUB_REF!=='refs/heads/'+expectedBranch)return {eligible:false,reason:'non_default_branch'};
  if(!/^[0-9a-f]{40}$/.test(env.GITHUB_SHA||'')||head!==env.GITHUB_SHA)fail('commit_identity');
  return {eligible:true,repository:expectedRepo,repository_id:expectedId,ref:env.GITHUB_REF,commit_oid:head,event:name};
}
export async function sendSnapshot(snapshot,{url,token,request=https.request,deadlineMs=60000,chunkDelayMs=0,onChunk}){
  const bytes=raw(snapshot);
  if(bytes.length<1)fail('snapshot_size');
  if(!token)fail('upload_token_missing');
  return await new Promise((resolve,reject)=>{
    const state={sourceEOF:false,sourceClosed:false,produced:0,queued:0,written:0,pendingWrites:0,requestFinish:false,requestClosed:false,responseEnd:false,responseComplete:false,responseClosed:true,socketAssigned:false,socketClosed:true,status:null,success:false};
    let source,req,res,socket,timer,drainTimer,reason=null,settled=false;
    const closed=()=>state.sourceClosed&&state.requestClosed&&state.responseClosed&&state.socketClosed&&state.pendingWrites===0;
    const complete=()=>state.socketAssigned&&socket===req?.socket&&state.sourceEOF&&state.produced===bytes.length&&state.queued===bytes.length&&state.written===bytes.length&&state.requestFinish&&state.responseEnd&&state.responseComplete&&state.status>=200&&state.status<300;
    const finish=()=>{
      if(settled||!closed())return;
      if(!reason&&!complete())reason='upload_incomplete';
      settled=true;clearTimeout(timer);clearTimeout(drainTimer);
      state.success=!reason;
      if(reason){const error=Error(reason);error.observed={...state};reject(error);}else resolve({...state,bytes:bytes.length,sha256:snapshot.sha256});
    };
    const stop=code=>{
      if(settled)return;
      if(!reason)reason=code;
      if(source&&!source.destroyed)source.destroy();
      if(res&&!res.destroyed)res.destroy();
      if(req&&!req.destroyed)req.destroy();
      if(socket&&!socket.destroyed)socket.destroy();
      if(!drainTimer)drainTimer=setTimeout(()=>{
        if(settled)return;settled=true;clearTimeout(timer);
        const error=Error('upload_drain_incomplete');error.observed={...state};reject(error);
      },2000);
      finish();
    };
    source=Readable.from((async function*(){
      for(let offset=0;offset<bytes.length;offset+=LIMITS.chunk){
        if(chunkDelayMs)await new Promise(r=>setTimeout(r,chunkDelayMs));
        if(reason)return;
        const chunk=Buffer.from(bytes.subarray(offset,Math.min(bytes.length,offset+LIMITS.chunk)));
        if(onChunk)onChunk();
        yield chunk;
      }
    })(),{objectMode:false,highWaterMark:LIMITS.chunk});
    source.on('data',chunk=>{state.produced+=chunk.length;if(state.produced>bytes.length)stop('upload_length');});
    source.once('end',()=>{state.sourceEOF=true;if(state.produced!==bytes.length)stop('upload_length');finish();});
    source.once('error',()=>stop('upload_source_error'));
    source.once('close',()=>{state.sourceClosed=true;if(!state.sourceEOF&&!reason)stop('upload_source_close');finish();});
    try{
      req=request(url,{method:'POST',agent:false,headers:{Authorization:'Bearer '+token,'User-Agent':'CodeQL-database-upload','Content-Type':'application/zip','Content-Length':bytes.length,'X-GitHub-Api-Version':'2022-11-28'}},response=>{
        res=response;state.responseClosed=false;state.status=res.statusCode;
        res.once('end',()=>{state.responseEnd=true;state.responseComplete=res.complete===true;
          if(!state.sourceEOF||!state.requestFinish||state.produced!==bytes.length||state.queued!==bytes.length||state.written!==bytes.length)stop('upload_early_response');
          else if(!state.responseComplete)stop('upload_response_incomplete');
          else if(!(state.status>=200&&state.status<300))stop('upload_http_'+state.status);
          finish();
        });
        res.once('error',()=>stop('upload_response_error'));
        res.once('aborted',()=>stop('upload_response_aborted'));
        res.once('close',()=>{state.responseClosed=true;if(!state.responseEnd&&!reason)stop('upload_response_close');finish();});
        res.resume();
      });
      req.on('socket',value=>{if(state.socketAssigned){stop('upload_socket_duplicate');return;}socket=value;state.socketAssigned=true;state.socketClosed=false;socket.once('error',()=>stop('upload_socket_error'));socket.once('close',()=>{state.socketClosed=true;finish();});if(req.socket!==value)stop('upload_socket_identity');});
      const write=req.write.bind(req);
      req.write=(chunk,encoding,callback)=>{
        const cb=typeof encoding==='function'?encoding:callback;
        if(!Buffer.isBuffer(chunk)){stop('upload_body_kind');return false;}
        state.queued+=chunk.length;state.pendingWrites++;
        if(state.queued>bytes.length){state.pendingWrites--;stop('upload_length');return false;}
        try{return write(chunk,typeof encoding==='string'?encoding:undefined,error=>{
          state.pendingWrites--;if(error)stop('upload_write_error');else state.written+=chunk.length;
          if(cb)cb(error);finish();
        });}catch{state.pendingWrites--;stop('upload_write_error');return false;}
      };
      req.once('finish',()=>{state.requestFinish=req.writableFinished===true;if(!state.sourceEOF||state.produced!==bytes.length||state.queued!==bytes.length||state.written!==bytes.length)stop('upload_finish_without_body');finish();});
      req.once('error',()=>stop('upload_request_error'));
      req.once('close',()=>{state.requestClosed=true;if(!complete()&&!reason)stop('upload_request_close');finish();});
      timer=setTimeout(()=>stop('upload_deadline'),deadlineMs);
      source.pipe(req);
    }catch{state.requestClosed=true;stop('upload_request_create');}
  });
}
function writeExclusive(file,data,io=fs){io.writeFileSync(file,data,{flag:'wx',mode:0o400});}
export function describeInventory(env,io=fs,execute=execFileSync,log=console.log){
  const root=path.resolve(env.GITHUB_WORKSPACE||'.'),spec=JSON.parse(io.readFileSync(path.join(root,'rust-project.json'),'utf8'));
  const tracked=execute('git',['ls-files','-z','--','*.rs'],{encoding:'utf8',timeout:5000,maxBuffer:LIMITS.inventory}).split('\0').filter(Boolean).sort();
  if(!tracked.length||new Set(tracked).size!==tracked.length||tracked.some(name=>!canonicalRustPath(name)))fail('rust_input_inventory');
  const selected=new Set(tracked),seen=new Set(),queue=spec.crates.map(crate=>crate.root_module);
  for(const name of queue){
    if(seen.has(name))continue;
    if(!canonicalRustPath(name)||!selected.has(name))fail('rust_project_outside_inventory');
    const file=path.join(root,name),stat=io.lstatSync(file);if(!stat.isFile()||stat.isSymbolicLink())fail('rust_source_kind');
    seen.add(name);
    const text=io.readFileSync(file,'utf8');
    for(const match of text.matchAll(/include!\s*\(\s*"([^"]+\.rs)"\s*\)/g))queue.push(path.posix.normalize(path.posix.join(path.posix.dirname(name),match[1])));
  }
  if(seen.size!==selected.size||tracked.some(name=>!seen.has(name)))fail('rust_project_missing_tracked_source');
  const sysroot=env.CODEQL_RUST_SYSROOT;
  if(!sysroot||!path.isAbsolute(sysroot))fail('rust_sysroot_unknown');
  spec.sysroot=sysroot;spec.sysroot_src=path.join(sysroot,'lib/rustlib/src/rust/library');
  if(!io.statSync(spec.sysroot_src).isDirectory())fail('rust_src_missing');
  for(const crate of spec.crates){crate.root_module=path.join(root,crate.root_module);crate.source.include_dirs=crate.source.include_dirs.map(dir=>path.join(root,dir));}
  const inputs=tracked.map(name=>{const body=io.readFileSync(path.join(root,name));return {path:name,bytes:body.length,sha256:hash(body)};});
  io.writeFileSync(path.join(env.RUNNER_TEMP,'codeql-rust-inputs.json'),JSON.stringify(inputs));
  io.writeFileSync(path.join(root,'rust-project.json'),JSON.stringify(spec));
  log('rust_project_crate_views='+spec.crates.length+' tracked_sources='+tracked.length+' full_build=false');
}

function requireInventory(value){
  if(!Array.isArray(value)||!value.length||value.length>LIMITS.members)fail('source_inventory');
  let previous='',total=0;
  for(const item of value){
    if(!canonicalRustPath(item.path)||item.path<=previous||!Number.isSafeInteger(item.bytes)||item.bytes<0||item.bytes>LIMITS.input||!/^[0-9a-f]{64}$/.test(item.sha256))fail('source_inventory_item');
    total+=item.bytes;if(total>LIMITS.sourceTotal)fail('owned_source_budget');
    previous=item.path;
  }
}
function captureInputs(env,io){
  const directory=path.join(env.RUNNER_TEMP,'codeql-verified-rust'),cluster=path.join(env.RUNNER_TEMP,'codeql_databases'),db=path.join(cluster,'rust');
  const inventory=jsonSnapshot(path.join(env.RUNNER_TEMP,'codeql-rust-inputs.json'),LIMITS.inventory,io);requireInventory(inventory.value);
  const source=captureFile(path.join(db,'src.zip'),LIMITS.archive,io);
  io.writeFileSync(path.join(env.RUNNER_TEMP,'codeql-rust-archive-members.txt'),openZip(source).names.join('\n')+'\n');
  const sourceProof=verifySource(source,inventory.value),sourceBodies=new Map(),sourceZip=openZip(source);
  for(const input of inventory.value){
    const name=sourceZip.names.find(name=>name===input.path||name.endsWith('/'+input.path));
    const held=sourceZip.member(name,LIMITS.input);sourceBodies.set(input.path,Buffer.from(raw(held)));
  }
  const results=path.join(env.RUNNER_TEMP,'codeql-query-results'),names=io.readdirSync(results).filter(name=>path.extname(name)==='.sarif');
  if(names.length!==1||names[0]!=='rust.sarif')fail('sarif_set');
  const sarif=captureFile(path.join(results,'rust.sarif'),LIMITS.sarif,io),value=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(raw(sarif)));
  if(value.version!=='2.1.0'||!Array.isArray(value.runs)||!value.runs.length||value.runs.some(run=>run?.tool?.driver?.name!=='CodeQL'))fail('sarif_structure');
  io.mkdirSync(directory,{mode:0o700});
  writeExclusive(path.join(directory,'rust.sarif'),raw(sarif),io);
  const proof={scope:'one invocation holds inventory/source/SARIF through official consumption and database upload; not full semantic cfg proof',source_inputs:inventory.value.length,inventory:{bytes:inventory.snapshot.bytes,sha256:inventory.snapshot.sha256},source_archive:sourceProof,sarif:{name:'rust.sarif',bytes:sarif.bytes,sha256:sarif.sha256}};
  writeExclusive(path.join(directory,'verified-inputs.json'),JSON.stringify(proof),io);
  return {directory,cluster,db,inventory,source,sarif,sourceBodies,proof};
}
export function credentiallessEnvironment(env){
  return Object.fromEntries(Object.entries(env).filter(([name])=>!/(TOKEN|SECRET|PASSWORD|CREDENTIAL)/i.test(name)));
}
function eventIdentity(env,event,head){
  if(env.GITHUB_SERVER_URL!=='https://github.com'||env.GITHUB_REPOSITORY!==expectedRepo||env.GITHUB_REPOSITORY_ID!==expectedId)fail('repository_identity');
  if(!['push','pull_request','workflow_dispatch','schedule'].includes(env.GITHUB_EVENT_NAME))fail('event_unknown');
  const repo=event?.repository;
  if(env.GITHUB_EVENT_NAME!=='schedule'&&(!repo||repo.full_name!==expectedRepo||String(repo.id)!==expectedId||repo.default_branch!==expectedBranch))fail('event_repository_identity');
  if(repo&&((repo.full_name!==undefined&&repo.full_name!==expectedRepo)||(repo.id!==undefined&&String(repo.id)!==expectedId)||(repo.default_branch!==undefined&&repo.default_branch!==expectedBranch)))fail('event_repository_identity');
  if(!/^[0-9a-f]{40}$/.test(env.GITHUB_SHA||'')||head!==env.GITHUB_SHA)fail('commit_identity');
  return {repository:expectedRepo,repository_id:expectedId,ref:env.GITHUB_REF,commit_oid:head,event:env.GITHUB_EVENT_NAME,database:env.GITHUB_EVENT_NAME!=='pull_request'&&env.GITHUB_REF==='refs/heads/main'};
}
function disposeSnapshot(ref){const value=owned.get(ref);if(value){value.fill(0);owned.delete(ref);}}
export function closePrepared(ref){
  const flight=preparedFlights.get(ref);if(!flight||flight.closed)return;
  flight.closed=true;closeDatabaseGraph(flight.database);
  for(const body of flight.session.sourceBodies.values())body.fill(0);flight.session.sourceBodies.clear();
  for(const body of flight.internalReads.values())body.fill(0);flight.internalReads.clear();
  for(const snapshot of [flight.session.source,flight.session.sarif,flight.session.inventory.snapshot,flight.official,flight.bundle,flight.eventSnapshot])disposeSnapshot(snapshot);
  preparedFlights.delete(ref);
}
export async function prepareFlight(env,services={}){
  if(Object.entries(env).some(([name,value])=>/(TOKEN|SECRET|PASSWORD|CREDENTIAL)/i.test(name)&&value))fail('preparation_credentials');
  const io=services.fs||fs,execute=services.exec||execFileSync,produce=services.produce||produceBundle,log=services.log||console.log;
  const event=jsonSnapshot(env.GITHUB_EVENT_PATH,LIMITS.inventory,io),head=execute('git',['rev-parse','HEAD'],{encoding:'utf8',timeout:5000}).trim();
  const identity=eventIdentity(env,event.value,head),session=captureInputs(env,io);let database,bundle,official;
  try{
    const canonical=canonicalizeLocations(raw(session.sarif),session.sourceBodies,path.resolve(env.GITHUB_WORKSPACE));
    const officialPath=path.join(env.RUNNER_TEMP,'codeql-official-action','lib','entry-points.js');
    official=captureFile(officialPath,8*1024*1024,io);checkOfficialBundle(raw(official));
    const locations=JSON.parse(env.CODEQL_DATABASES||'null');
    if(typeof locations?.rust!=='string'||path.resolve(locations.rust)!==path.resolve(session.db)||io.realpathSync(session.db)!==path.resolve(session.db))fail('database_location');
    const cli=env.CODEQL_PATH;if(!cli||!path.isAbsolute(cli))fail('cli_missing');
    execute(cli,['database','cleanup',session.db,'--cache-cleanup=clear'],{stdio:'inherit',timeout:60000,env:credentiallessEnvironment(env)});
    const oids=path.join(session.cluster,'base-database-oids.json');let capturedOID;
    if(io.existsSync(oids)){capturedOID=captureFile(oids,LIMITS.inventory,io);io.writeFileSync(path.join(session.db,'base-database-oids.json'),raw(capturedOID));}
    database=captureDatabaseGraph(session.db,{io,ownerPID:services.ownerPID??process.pid});
    const graph=checkedGraph(database);
    if(capturedOID&&graph.files.get('base-database-oids.json')?.sha256!==capturedOID.sha256)fail('database_oid_changed');
    const sourceRecord=graph.files.get('src.zip');
    if(sourceRecord.sha256!==session.source.sha256||sourceRecord.bytes!==session.source.bytes)fail('database_source_mismatch');
    const includes=graph.files.has('base-database-oids.json')?['base-database-oids.json']:[];
    const produced=await produce({cli,database:graph.anchor,includes,env:credentiallessEnvironment(env),limit:LIMITS.bundle});
    if(!Buffer.isBuffer(produced.bytes))fail('producer_body');
    bundle=copySnapshot(produced.bytes,LIMITS.bundle);produced.bytes.fill(0);
    const verified=await verifyBundle(bundle,session.source,session.inventory.value,database);
    assertDatabaseUnchanged(database);
    const internalReads=new Map([[path.resolve(env.GITHUB_EVENT_PATH),Buffer.from(raw(event.snapshot))]]),internalAbsent=[];
    const diffPath=path.join(env.CODEQL_ACTION_TEMP||env.RUNNER_TEMP,'pr-diff-range.json');
    if(io.existsSync(diffPath)){const diff=captureFile(diffPath,LIMITS.input,io);internalReads.set(path.resolve(diffPath),Buffer.from(raw(diff)));disposeSnapshot(diff);}else internalAbsent.push(path.resolve(diffPath));
    const reference=Object.freeze({verified:true,sourceInputs:session.inventory.value.length,databaseFiles:database.files,databaseGraphSHA256:database.graphSHA256,bundleBytes:bundle.bytes,bundleSHA256:bundle.sha256,canonicalSARIF:canonical.canonical_sha256});
    const flight={session,database,bundle,official,officialPath,eventSnapshot:event.snapshot,internalReads,internalAbsent,identity,env:{...env},canonical,producer:produced.evidence,closed:false,publishing:false};
    preparedFlights.set(reference,flight);
    writeExclusive(path.join(session.directory,'verified-database.json'),JSON.stringify({...identity,...verified,root:{device:database.rootDevice,inode:database.rootInode},producer:produced.evidence}),io);
    writeExclusive(path.join(session.directory,'canonical-rust.sarif'),canonical.bytes,io);
    writeExclusive(path.join(session.directory,'verified-flight.json'),JSON.stringify({...reference,identity,rawSARIF:session.sarif.sha256,scope:'post-analysis DB root held through full-file capture, original producer anonymous pipe, every archive file matched before credentials; not compiler or host attestation'}),io);
    log('rust_owned_flight_ready source_inputs='+session.inventory.value.length+' database_files='+database.files+' bundle_bytes='+bundle.bytes);
    return reference;
  }catch(error){
    try{const message=String(error?.message||''),reason=/^[a-z][a-z0-9_]{0,100}$/.test(message)?message:'preparation_io_error';writeExclusive(path.join(session.directory,'verification-failure.json'),JSON.stringify({reason,database:error.database||null,producer:error.producer||null}),io);}catch{error.evidenceWriteFailed=true;}
    if(error.producer){try{writeExclusive(path.join(session.directory,'producer-failure.json'),JSON.stringify(error.producer),io);writeExclusive(path.join(session.directory,'producer-stderr.txt'),String(error.producerStderr||''),io);}catch{error.evidenceWriteFailed=true;}}
    if(database)closeDatabaseGraph(database);
    for(const body of session.sourceBodies.values())body.fill(0);
    for(const ref of [session.source,session.sarif,session.inventory.snapshot,official,bundle,event.snapshot])disposeSnapshot(ref);
    throw error;
  }
}
export async function publishPrepared(reference,{token},services={}){
  const flight=preparedFlights.get(reference);
  if(!flight||flight.closed||flight.publishing||!token)fail('prepared_flight_unknown');
  flight.publishing=true;
  const io=services.fs||fs,consume=services.sarif||consumeOfficialSarif,transmit=services.send||sendSnapshot;
  const publish=flight.env.DATABASE_PUBLISH==='true';
  try{
    const result=await consume({bundle:raw(flight.official),filename:flight.officialPath,sarif:raw(flight.session.sarif),sources:flight.session.sourceBodies,workspace:path.resolve(flight.env.GITHUB_WORKSPACE),env:flight.env,token,publish,internalReads:flight.internalReads,internalAbsent:flight.internalAbsent,io});
    writeExclusive(path.join(flight.session.directory,'sarif-upload-result.json'),JSON.stringify({...flight.identity,...result}),io);
    let databaseResult={publication:false,reason:!publish?'publication_disabled':!flight.identity.database?'non_default_or_pull_request':'not_attempted'};
    if(publish&&flight.identity.database){
      const url=new URL('https://uploads.github.com/repos/'+expectedRepo+'/code-scanning/codeql/databases/rust');url.searchParams.set('name','rust-database');url.searchParams.set('commit_oid',flight.identity.commit_oid);
      databaseResult=await transmit(flight.bundle,{url,token});
    }
    writeExclusive(path.join(flight.session.directory,'database-upload-result.json'),JSON.stringify({...flight.identity,...databaseResult}),io);
    return {sarif:result,database:databaseResult,publication:publish};
  }catch(error){
    const text=String(error?.message||''),reason=/^[a-z][a-z0-9_]{0,100}$/.test(text)?text:'owned_publication_error';
    try{writeExclusive(path.join(flight.session.directory,'consumer-failure.json'),JSON.stringify({reason,errorName:/^[A-Za-z]+Error$/.test(String(error.name))?error.name:'Error',messageSHA256:hash(Buffer.from(String(error.message||''))),officialFrames:[...String(error.stack||'').matchAll(/entry-points[.]js:([0-9]+):([0-9]+)/g)].slice(0,5).map(m=>({line:Number(m[1]),column:Number(m[2])})),ownedIO:error.ownedIO||null}),io);}catch{error.evidenceWriteFailed=true;}
    throw error;
  }finally{closePrepared(reference);}
}
export async function runMode(mode,env=process.env,services={}){
  if(mode!=='inventory')fail('mode_unknown');
  return describeInventory(env,services.fs||fs,services.exec||execFileSync,services.log||console.log);
}
if(typeof process!=='undefined'&&process.argv[1]&&import.meta.url===pathToFileURL(process.argv[1]).href)runMode(process.argv[2]).catch(error=>{const text=String(error?.message||''),reason=/^[a-z][a-z0-9_]{0,100}$/.test(text)?text:'publication_io_or_transport';console.error('rust_publication_failed='+reason);process.exitCode=1;});
