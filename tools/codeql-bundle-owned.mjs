import {spawn} from 'node:child_process';
import crypto from 'node:crypto';
const fail=reason=>{throw Error(reason);};
// Bash creates a real anonymous POSIX pipe between CodeQL and cat. The Node
// stdout endpoint is not opened by CodeQL as a file. No output ZIP path exists.
export async function produceBundle({cli,database,includes,env,limit,spawnProcess=spawn}){
  if(!cli?.startsWith('/')||!database?.startsWith('/')||!Array.isArray(includes)||!Number.isSafeInteger(limit)||limit<1)fail('producer_arguments');
  if(Object.keys(env).some(k=>/(TOKEN|SECRET|PASSWORD|CREDENTIAL)/i.test(k)))fail('producer_credentials');
  const argumentsForCLI=[cli,'database','bundle',database,'--output=/dev/stdout','--name=rust','--cache-cleanup=clear','--verbosity=errors'];
  for(const name of includes){if(typeof name!=='string'||!name||name.includes('/')||name==='.'||name==='..'||name.includes('\0'))fail('producer_include');argumentsForCLI.push('--include',name);}
  return await new Promise((resolve,reject)=>{
    const chunks=[],stderr=[];let bytes=0,errors=0,overflow=false,stdoutEnd=false,stdoutClose=false,stderrEnd=false,stderrClose=false,exited=false,closed=false,code=null,signal=null,spawnError=false,settled=false,child;
    const finish=()=>{
      if(settled||!closed||!stdoutClose||!stderrClose)return;
      settled=true;
      const evidence={monitorPID:child.pid??null,exitCode:code,signal,stdoutEnd,stdoutClose,stderrEnd,stderrClose,rootExitObserved:exited,rootCloseObserved:closed,bytes,stderrBytes:errors,outputPathReopened:false,backend:'gnu-timeout-60s-kill-after-2s; bash-real-pipe; original-CodeQL-file-output-to-dev-stdout'};
      const output=overflow?null:Buffer.concat(chunks);chunks.length=0;
      evidence.sha256=output?crypto.createHash('sha256').update(output).digest('hex'):null;
      const stderrText=Buffer.concat(stderr).toString('utf8'),markers=[...stderrText.matchAll(/^owned_codeql_cli_pid=([0-9]+)$/gm)];
      evidence.producerPID=markers.length===1?Number(markers[0][1]):null;
      evidence.producerIdentityObserved=Number.isSafeInteger(evidence.producerPID)&&evidence.producerPID>0&&evidence.producerPID!==evidence.monitorPID;
      evidence.identityScope='Bash-started CLI root PID and wait status; not a live process-generation or toolchain attestation';
      if(spawnError||overflow||!evidence.producerIdentityObserved||code!==0||signal||!stdoutEnd||!stdoutClose||!stderrEnd||!stderrClose||!exited){const error=Error(overflow?'producer_size':spawnError?'producer_spawn':!evidence.producerIdentityObserved?'producer_identity':'producer_failed');error.producer=evidence;error.producerStderr=stderrText;reject(error);return;}
      resolve({bytes:output,evidence,stderr:stderrText});
    };
    try{
      child=spawnProcess('/usr/bin/timeout',['--signal=TERM','--kill-after=2s','60s','/bin/bash','-o','pipefail','-c','{ "$@" & producer=$!; printf "owned_codeql_cli_pid=%s\n" "$producer" >&2; wait "$producer"; } | /bin/cat','owned-codeql-bundle',...argumentsForCLI],{env,stdio:['ignore','pipe','pipe'],windowsHide:true});
      child.once('error',()=>{spawnError=true;});
      child.stdout.on('data',chunk=>{bytes+=chunk.length;if(bytes>limit){overflow=true;chunks.length=0;}else if(!overflow)chunks.push(Buffer.from(chunk));});
      child.stderr.on('data',chunk=>{errors+=chunk.length;if(errors<=1024*1024)stderr.push(Buffer.from(chunk));});
      child.stdout.once('end',()=>{stdoutEnd=true;});child.stdout.once('close',()=>{stdoutClose=true;finish();});
      child.stderr.once('end',()=>{stderrEnd=true;});child.stderr.once('close',()=>{stderrClose=true;finish();});
      child.stdout.on('error',()=>{spawnError=true;});child.stderr.on('error',()=>{spawnError=true;});
      child.once('exit',(value,why)=>{exited=true;code=value;signal=why;});
      child.once('close',(value,why)=>{closed=true;code=value;signal=why;finish();});
    }catch{reject(Error('producer_spawn'));}
  });
}
