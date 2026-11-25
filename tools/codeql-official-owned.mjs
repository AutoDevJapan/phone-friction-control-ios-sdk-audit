import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import vm from 'node:vm';
import {createRequire,isBuiltin} from 'node:module';
import {Readable} from 'node:stream';
import {fileURLToPath,pathToFileURL} from 'node:url';
export const OFFICIAL=Object.freeze({revision:'33d7b85da1a04171e211befe2c739303b9a4c066',bytes:7450655,sha256:'73d629849c79c82ffb9a3c2c9748f4f8639e1ac384e499f1d6678e47b011cd88',gitBlob:'952c615417325b357e632dd14fe2bd0087599257'});
const fail=reason=>{throw Error(reason);},sha=bytes=>crypto.createHash('sha256').update(bytes).digest('hex');
function sourceTable(sources,workspace){
  const table=new Map();
  for(const [name,body] of sources){
    if(typeof name!=='string'||name.includes('\\')||name.includes('\0')||name.split('/').some(p=>!p||p==='.'||p==='..')||!Buffer.isBuffer(body))fail('official_source_input');
    const file=path.resolve(workspace,name),relative=path.relative(workspace,file);
    if(!relative||relative==='..'||relative.startsWith('..'+path.sep)||path.isAbsolute(relative)||table.has(file))fail('official_source_input');
    table.set(file,body);
  }return table;
}
// Preserve the result and its location, but make the reference unambiguous
// before the official fingerprint consumer can inspect any source path.
export function canonicalizeLocations(sarif,sources,workspace){
  workspace=path.resolve(workspace);
  const table=sourceTable(sources,workspace),value=JSON.parse(new TextDecoder('utf8',{fatal:true}).decode(sarif));
  if(value.version!=='2.1.0'||!Array.isArray(value.runs)||!value.runs.length)fail('sarif_structure');
  let locations=0;
  for(const run of value.runs){
    if(!run||!Array.isArray(run.results??[])||!Array.isArray(run.artifacts??[]))fail('sarif_structure');
    const bases=run.originalUriBaseIds??{};
    const resolveLocation=(location,depth=0,seen=new Set())=>{
      if(!location||typeof location!=='object'||depth>8)fail('sarif_location_missing');
      if(location.uri===undefined&&location.index!==undefined){
        if(!Number.isInteger(location.index)||location.index<0||!run.artifacts?.[location.index]?.location)fail('sarif_location_index');
        return resolveLocation(run.artifacts[location.index].location,depth+1,seen);
      }
      if(typeof location.uri!=='string'||!location.uri.length)fail('sarif_location_missing');
      let decoded;try{decoded=decodeURIComponent(location.uri);}catch{fail('sarif_location_encoding');}
      if(/[\0-\x1f\x7f\\]/.test(decoded))fail('sarif_location_encoding');
      let root=workspace;
      if(location.uriBaseId!==undefined){
        const baseID=location.uriBaseId;if(typeof baseID!=='string'||seen.has(baseID)||!bases[baseID])fail('sarif_location_base');
        const next=new Set(seen);next.add(baseID);
        root=resolveLocation(bases[baseID],depth+1,next);
      }
      let file;
      if(/^file:/.test(location.uri)){
        try{const url=new URL(location.uri);if(url.hostname&&url.hostname!=='localhost')fail('sarif_location_authority');file=fileURLToPath(url);}catch{fail('sarif_location_uri');}
      }else{
        if(/^[A-Za-z][A-Za-z0-9+.-]*:/.test(decoded)&&!path.isAbsolute(decoded))fail('sarif_location_scheme');
        file=path.resolve(root,decoded);
      }
      const relative=path.relative(workspace,file);
      if(relative==='..'||relative.startsWith('..'+path.sep)||path.isAbsolute(relative))fail('sarif_location_escape');
      return file;
    };
    for(const result of run.results??[]){
      if(!result||!Array.isArray(result.locations??[]))fail('sarif_structure');
      const physical=result.locations?.[0]?.physicalLocation;
      if(!physical?.artifactLocation)continue;
      const file=resolveLocation(physical.artifactLocation);
      if(!table.has(file))fail('sarif_location_unknown');
      const normalized={...physical.artifactLocation,uri:path.relative(workspace,file).split(path.sep).map(encodeURIComponent).join('/')};
      delete normalized.index;delete normalized.uriBaseId;
      physical.artifactLocation=normalized;locations++;
    }
  }
  const bytes=Buffer.from(JSON.stringify(value));
  return {bytes,locations,raw_sha256:sha(sarif),canonical_sha256:sha(bytes)};
}
export function snapshotView(sarif,sources,workspace,{io=fs,internalReads=new Map(),internalAbsent=[],outputFiles=[],dumpDirectory}={}){
  workspace=path.resolve(workspace);
  const virtual=path.join(workspace,'.codeql-owned-'+crypto.randomBytes(16).toString('hex')+'.sarif');
  const held=new Map([[virtual,Buffer.from(sarif)]]),sourceNames=new Set();
  for(const [name,body] of sourceTable(sources,workspace)){held.set(name,Buffer.from(body));sourceNames.add(name);}
  const internal=new Set();
  for(const [name,body] of internalReads){const file=path.resolve(name);if(held.has(file)||!Buffer.isBuffer(body))fail('official_internal_input');held.set(file,Buffer.from(body));internal.add(file);}
  const absent=new Set(internalAbsent.map(p=>path.resolve(p)));
  if([...absent].some(p=>held.has(p)))fail('official_internal_input');
  const names=new Set(held.keys()),outputs=new Set(outputFiles.map(p=>path.resolve(p)));
  const dump=dumpDirectory?path.resolve(dumpDirectory):null;
  let closed=false,sarifReads=0,sourceReads=0,internalReadsCount=0;
  const key=file=>{if(typeof file!=='string')fail('official_io_path');return path.resolve(file);};
  const deny=(reason,file,operation)=>{const error=Error(reason);error.ownedIO={operation,pathSHA256:sha(Buffer.from(file)),region:path.relative(workspace,file).startsWith('..')?'outside_workspace':'workspace'};throw error;};
  const get=(file,operation='read')=>{
    if(closed)fail('official_snapshot_closed');
    if(!names.has(file))deny('official_io_not_admitted',file,operation);
    if(file===virtual)sarifReads++;else if(sourceNames.has(file))sourceReads++;else internalReadsCount++;
    return held.get(file);
  };
  const output=file=>outputs.has(file)||(dump&&(file===dump||file.startsWith(dump+path.sep)));
  const knownStat=file=>({isFile:()=>true,isDirectory:()=>false,isSymbolicLink:()=>false,size:held.get(file).length});
  const permitted=['readFileSync','existsSync','statSync','lstatSync','realpathSync','createReadStream','appendFileSync','writeFileSync','mkdirSync','constants','promises'];
  const facade=new Proxy(Object.create(null),{ownKeys:()=>permitted,getOwnPropertyDescriptor:(_target,name)=>permitted.includes(name)?{value:facade[name],enumerable:true,configurable:true}:undefined,get(_target,name){
    if(name==='__esModule')return undefined;
    if(typeof name==='symbol')return undefined;
    if(name==='promises')return Object.freeze({readFile:async(...args)=>facade.readFileSync(...args),stat:async(...args)=>facade.statSync(...args),lstat:async(...args)=>facade.lstatSync(...args),realpath:async(...args)=>facade.realpathSync(...args),appendFile:async(...args)=>facade.appendFileSync(...args),writeFile:async(...args)=>facade.writeFileSync(...args),mkdir:async(...args)=>facade.mkdirSync(...args)});
    if(name==='readFileSync')return(file,options)=>{const body=get(key(file),'readFileSync'),encoding=typeof options==='string'?options:options?.encoding;return encoding?body.toString(encoding):Buffer.from(body);};
    if(name==='existsSync')return file=>{file=key(file);if(closed)fail('official_snapshot_closed');if(names.has(file))return true;if(absent.has(file))return false;if(output(file))return io.existsSync(file);deny('official_io_not_admitted',file,'existsSync');};
    if(name==='statSync'||name==='lstatSync')return file=>{file=key(file);if(closed)fail('official_snapshot_closed');if(names.has(file))return knownStat(file);if(output(file))return io[name](file);deny('official_io_not_admitted',file,String(name));};
    if(name==='realpathSync')return file=>{file=key(file);if(closed)fail('official_snapshot_closed');if(names.has(file))return file;deny('official_io_not_admitted',file,'realpathSync');};
    if(name==='createReadStream')return(file,options)=>{file=key(file);if(!sourceNames.has(file))deny('official_source_not_admitted',file,'createReadStream');const stream=Readable.from([Buffer.from(get(file))],{objectMode:false}),encoding=typeof options==='string'?options:options?.encoding;if(encoding)stream.setEncoding(encoding);return stream;};
    if(['appendFileSync','writeFileSync','mkdirSync'].includes(name))return(file,...args)=>{file=key(file);if(closed||!output(file)||names.has(file))fail('official_output_not_admitted');return io[name](file,...args);};
    if(name==='constants')return io.constants;
    // No generic fallback: an additional official I/O dependency must be
    // identified and admitted as a non-source capability, never by a SARIF URI.
    return()=>fail('official_operation_not_admitted');
  }});
  return Object.freeze({virtual,fs:facade,stats:()=>({sarifReads,sourceReads,internalReads:internalReadsCount,closed}),close(){if(closed)return;closed=true;for(const b of held.values())b.fill(0);held.clear();}});
}
export function checkOfficialBundle(bytes){
  if(!Buffer.isBuffer(bytes)||bytes.length!==OFFICIAL.bytes||sha(bytes)!==OFFICIAL.sha256)fail('official_bundle_identity');
  const blob=crypto.createHash('sha1').update(Buffer.from('blob '+bytes.length+'\0')).update(bytes).digest('hex');if(blob!==OFFICIAL.gitBlob)fail('official_bundle_identity');
}
export function pinnedUploadLibrary(bytes,filename,view,env){
  checkOfficialBundle(bytes);
  // Use the real Node runtime with scoped environment and filesystem data
  // capabilities. This VM is not a security sandbox for untrusted code.
  const scopedProcess=Object.create(process);Object.defineProperty(scopedProcess,'env',{value:{...env}});
  const nativeRequire=createRequire(filename),module={exports:{}};
  const requireBuiltin=name=>{if(name==='fs'||name==='node:fs')return view.fs;if(name==='fs/promises'||name==='node:fs/promises')return view.fs.promises;if(name==='process'||name==='node:process')return scopedProcess;if(!isBuiltin(name))fail('official_external_module');return nativeRequire(name);};
  const globals={module,exports:module.exports,require:requireBuiltin,__filename:filename,__dirname:path.dirname(filename),process:scopedProcess,Buffer,console,URL,URLSearchParams,TextEncoder,TextDecoder,setTimeout,clearTimeout,setInterval,clearInterval,setImmediate,clearImmediate,queueMicrotask,AbortController,AbortSignal};
  for(const key of ['fetch','Blob','File','FormData','Headers','Request','Response','performance','structuredClone'])if(globalThis[key]!==undefined)globals[key]=globalThis[key];globals.global=globals;
  new vm.Script(bytes.toString('utf8'),{filename}).runInNewContext(globals,{timeout:10000});
  const lib=module.exports.uploadLib;
  for(const name of ['postProcessSarifFiles','writePostProcessedFiles','uploadPostProcessedFiles','waitForProcessing'])if(typeof lib?.[name]!=='function')fail('official_upload_api');
  return lib;
}
const scanning=Object.freeze({kind:'code-scanning',name:'code scanning',target:'PUT /repos/:owner/:repo/code-scanning/analysis',sarifExtension:'.sarif',sarifPredicate:name=>name.endsWith('.sarif')&&!name.endsWith('.quality.sarif')&&!name.endsWith('.csra.sarif'),fixCategory:(_,category)=>category,sentinelPrefix:'CODEQL_UPLOAD_SARIF_',transformPayload:payload=>payload});
export async function consumeOfficialSarif({bundle,filename,sarif,sources,workspace,env,token,publish,internalReads,internalAbsent,io=fs,logger=console}){
  if(!token||!env.INPUT_MATRIX)fail('official_action_inputs');
  const normalized=canonicalizeLocations(sarif,sources,workspace);
  const view=snapshotView(normalized.bytes,sources,workspace,{io,internalReads,internalAbsent,outputFiles:[env.GITHUB_ENV,env.GITHUB_OUTPUT].filter(Boolean),dumpDirectory:env.CODEQL_ACTION_SARIF_DUMP_DIR});
  try{
    const lib=pinnedUploadLibrary(bundle,filename,view,{...env,INPUT_TOKEN:token,GITHUB_TOKEN:token});
    const log={info:m=>logger.log(m),debug:()=>{},warning:m=>logger.warn(m),error:m=>logger.error(m),startGroup:m=>logger.log('::group::'+m),endGroup:()=>logger.log('::endgroup::')};
    const processed=await lib.postProcessSarifFiles(log,undefined,undefined,path.resolve(workspace),[view.virtual],'/language:rust',scanning);
    if(view.stats().sarifReads!==1||!processed?.sarif)fail('official_snapshot_not_consumed');
    await lib.writePostProcessedFiles(log,undefined,scanning,processed);
    const evidence={raw_bytes:sarif.length,raw_sha256:normalized.raw_sha256,canonical_sha256:normalized.canonical_sha256,locations:normalized.locations,official_revision:OFFICIAL.revision,source_reads:view.stats().sourceReads,publication:false};
    if(!publish)return {...evidence,normalization:true};
    const result=await lib.uploadPostProcessedFiles(log,path.resolve(workspace),scanning,processed);
    if(typeof result?.sarifID!=='string'||!result.sarifID)fail('official_sarif_id');
    await lib.waitForProcessing({owner:'AutoDevJapan',repo:'phone-friction-control-ios-sdk-audit'},result.sarifID,log);
    return {...evidence,publication:true,sarif_id:result.sarifID,processing:'official_wait_returned_not_persistence_proof'};
  }finally{view.close();}
}
