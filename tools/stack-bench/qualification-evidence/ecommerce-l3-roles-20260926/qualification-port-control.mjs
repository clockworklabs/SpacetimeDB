import assert from 'node:assert/strict';
import {execFileSync} from 'node:child_process';
import {mkdirSync,writeFileSync} from 'node:fs';
import {preflightParallelMutationResources} from '../../dist/src/references/reference-live.js';
const image='sha256:2ea252ac3886601336e6d6f7763c1c2a26def1498f7339f4655592bb3f09e63f';
const name='stack-bench-qualification-port-control';
const env={...process.env,STACK_BENCH_APPLIANCE:'1',STACK_BENCH_CONTROLLER_IMAGE_ID:image};
const args={backend:'mongodb',track:'ecommerce',runIndex:100,mutationWorkers:3,spacetimePort:null};
const docker=(...args)=>execFileSync('docker',args,{encoding:'utf8',stdio:'pipe',env});
const cases=[];
let owned=false;
try {
  docker('run','-d','--name',name,'--publish','127.0.0.1:6503:6503','--entrypoint','sleep',image,'120');
  owned=true;
  let error;
  try {preflightParallelMutationResources(args,env)} catch(e){error=e}
  cases.push({name:'occupied final worker port rejects admission',passed:Boolean(error),message:error?.message??null});
  assert.match(error?.message??'',/port.*(?:unavailable|available|occupied)/i);
  docker('rm','-f',name);owned=false;
  preflightParallelMutationResources(args,env);
  cases.push({name:'released worker ports permit admission',passed:true});
  assert.throws(()=>preflightParallelMutationResources(args,{...env,
    STACK_BENCH_CONTROLLER_IMAGE_ID:'',STACK_BENCH_CONTROLLER_IMAGE:''}),/pinned controller image/);
  cases.push({name:'missing immutable image rejects admission',passed:true});
  assert.throws(()=>preflightParallelMutationResources(args,{...env,
    DOCKER_HOST:'npipe:////./pipe/stack-bench-no-such-engine'}));
  cases.push({name:'Docker engine failure remains an error',passed:true});
} finally {
  if(owned)docker('rm','-f',name);
  mkdirSync('local-notes/sol6-four-stack-20260925/role-fix',{recursive:true});
  writeFileSync('local-notes/sol6-four-stack-20260925/role-fix/qualification-port-control.json',JSON.stringify({image,args,cases,cleanup:'owned listener removed'},null,2)+'\n');
}
console.log(JSON.stringify(cases));
