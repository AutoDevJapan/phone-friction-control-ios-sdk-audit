import fs from 'node:fs';
import {openJob,announcePrepared,receivePublicationToken,announceFinished,closeJob} from './codeql-bundle-owned.mjs';
import {prepareFlight,publishPrepared,closePrepared} from './codeql-rust-publication.mjs';
// stdout is a dedicated kernel control pipe; ordinary logging stays diagnostic.
process.stdout.write=(...args)=>process.stderr.write(...args);
console.log=(...v)=>process.stderr.write(v.join(' ')+'\n');console.info=console.log;
let job,reference;
try{
 job=await openJob();reference=await prepareFlight({...process.env});
 announcePrepared(job,reference);
 const token=await receivePublicationToken(job);
 const result=await publishPrepared(reference,{token});reference=null;
 announceFinished(job,result.publication);
}catch(error){process.exitCode=1;const s=String(error?.message||''),reason=/^[a-z][a-z0-9_]{0,100}$/.test(s)?s:'root_flight_failed';process.stderr.write('root_flight_failed='+reason+'\n');}
finally{if(reference)closePrepared(reference);if(job){try{await closeJob(job);}catch{fs.writeSync(2,'root_cleanup_failed=outer_namespace_reap_required\n');process.exit(1);}}}
