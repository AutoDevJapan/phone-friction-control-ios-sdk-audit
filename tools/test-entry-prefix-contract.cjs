// Extract actual producer expressions; no SDK memory or synthetic token access.
const fs = require('fs'), cp = require('child_process'), path = require('path');
const source = fs.readFileSync('tools/ios-abi-probe.rs', 'utf8');
const bound = source.match(/let bound = ([\s\S]*?);\s*let count = bound/)[1];
const condition = source.match(/if ([^\n]+) \{\n\s*\/\/ The public allocator/)[1];
const cutoff = source.match(/if next < ([^\n]+) \{/)[1];
const dir = '.godd/reviews/context-arc-trace';
const generated = `fn main(){
for label in ["FamilyActivityPicker.init(selection:)","UIHostingController.init(rootView:) allocating","other","public hosting page continuation offset=24"] {
 let bound=${bound};
 for next in [0usize,24,127,128,512,4096] {
  let first=bound.min(next);
  let has_tail=(${condition}) && next < (${cutoff});
  let tail=if has_tail {128usize}else{0};
  if cfg!(picker_entry_probe){assert_eq!(bound,128);assert!(!has_tail);assert_eq!(first+tail,128usize.min(next));}
  else {let ctor=label=="FamilyActivityPicker.init(selection:)"||label=="UIHostingController.init(rootView:) allocating";assert_eq!(bound,if ctor{512}else{128});assert!(first+tail<=512);}
 }
}}
`;
fs.writeFileSync(path.join(dir,'prefix-contract-extracted.rs'),generated);
for(const mode of ['legacy','new']) {
 const exe=path.join(dir,`prefix-contract-${mode}.exe`);
 const args=['--edition','2024',path.join(dir,'prefix-contract-extracted.rs'),'-o',exe];
 if(mode==='legacy')args.push('--cfg','picker_entry_probe');
 const build=cp.spawnSync('rustc',args,{encoding:'utf8'});
 if(build.status!==0)throw Error(build.stderr);
 const run=cp.spawnSync(exe,[],{encoding:'utf8'});
 if(run.status!==0)throw Error(run.stderr);
 console.log(`${mode}: actual-source expressions/4 labels/6 page boundaries PASS`);
}
const mainWorkflow=cp.execFileSync('git',['show','origin/main:.github/workflows/ios-sdk-audit.yml'],{encoding:'utf8'});
if(!mainWorkflow.includes('entry_prefix_max_bytes=128'))throw Error('legacy consumer contract missing');
if(!source.includes('"entry_prefix_max_bytes=128"'))throw Error('legacy declaration mismatch');
console.log('main consumer128 / producer128 / legacy no continuation verified; no Apple runtime');
