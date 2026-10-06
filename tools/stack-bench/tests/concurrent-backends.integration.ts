import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

test('two admitted native attempts overlap and deny cross-attempt traffic', {
  skip: process.env.STACK_BENCH_CONCURRENT_BACKENDS_TEST !== '1', timeout: 180_000,
}, () => {
  const image = process.env.STACK_BENCH_NETWORK_CONTROLLER_IMAGE;
  assert.match(image ?? '', /^sha256:[a-f0-9]{64}$/);
  const prefix = `stack-bench-concurrent-probe-${randomBytes(6).toString('hex')}`;
  const docker = (args: string[], input?: string): string => {
    const result = spawnSync('docker', args, { encoding: 'utf8', input, timeout: 150_000 });
    assert.equal(result.status, 0, `${args.slice(0, 3).join(' ')}: ${result.stderr || result.error}\n${result.stdout}`);
    return result.stdout.trim();
  };
  const cache = docker(['inspect', '--format', '{{.Id}}', 'stack-bench-npm-cache']);
  const memberships = docker(['inspect', '--format', '{{json .NetworkSettings.Networks}}', cache]);
  const volume = docker(['volume', 'create', `${prefix}-state`]);
  const state = docker(['volume', 'inspect', '--format', '{{.Mountpoint}}', volume]);
  let controller: string | undefined;
  try {
    const worker = `
      import { mkdirSync, existsSync, writeFileSync } from 'node:fs';
      import { setTimeout as delay } from 'node:timers/promises';
      import { createBackendLease, claimBackendResources, backendResourceLockKeys, resourceLockScope,
        readBackendLease } from './dist/src/runtime/backend-lease.js';
      import { releaseBackendLease } from './dist/src/runtime/backend-teardown.js';
      import { loadTrack, portsFor } from './dist/src/composition/tracks.js';
      import { STACK_ADAPTER_REGISTRY } from './dist/src/stacks/stack-adapters.js';
      import { attemptDocker } from './dist/src/runtime/docker-network.js';
      const backend=process.env.PROBE_BACKEND, index=Number(process.env.PROBE_INDEX);
      const directory=process.env.PROBE_STATE+'/'+backend;
      mkdirSync(directory,{recursive:true});
      const path=directory+'/lease.json';
      const lease=createBackendLease({backend,runId:process.env.PROBE_PREFIX+'-'+backend,
        track:'chat',runIndex:18+index,database:'concurrent_probe'});
      try {
        claimBackendResources(path,lease,{...resourceLockScope(),
          keys:backendResourceLockKeys(lease,portsFor(loadTrack(lease.track),backend,lease.runIndex))});
        STACK_ADAPTER_REGISTRY.get(backend).lifecycle.activate({leasePath:path,leaseToken:lease.ownershipToken,lease,
          ports:portsFor(loadTrack(lease.track),backend,lease.runIndex)});
        const active=readBackendLease(path,{token:lease.ownershipToken,active:true});
        attemptDocker(['exec','-d',active.resources.browserContainer.id,'node','-e',
          "require('node:http').createServer((q,s)=>s.end('"+backend+"')).listen(18080,'0.0.0.0')"]);
        writeFileSync(directory+'/ready','ready');
        const deadline=Date.now()+60_000;
        while(!existsSync(process.env.PROBE_STATE+'/release')) {
          if(Date.now()>deadline) throw new Error('concurrent probe coordinator timed out');
          await delay(50);
        }
      } finally {
        if(existsSync(path)&&!releaseBackendLease(path,lease.ownershipToken)) throw new Error('owned cleanup refused');
      }
    `;
    controller = docker(['create', '--name', `${prefix}-controller`, '--network', 'host',
      '--mount', 'type=bind,source=/var/run/docker.sock,target=/var/run/docker.sock',
      '--mount', `type=bind,source=${state},target=${state}`,
      '--mount', `type=bind,source=${fileURLToPath(new URL('../', import.meta.url))},target=/opt/stack-bench/dist,readonly`,
      '-e', 'STACK_BENCH_APPLIANCE=1',
      '-e', `STACK_BENCH_RESOURCE_LOCK_DIR=${state}/locks`,
      '-e', `STACK_BENCH_CONTROLLER_IMAGE_ID=${image}`, '--entrypoint', 'node', image!,
      '--input-type=module', '-e', `
      import assert from 'node:assert/strict';
      import {spawn} from 'node:child_process';
      import {existsSync,readFileSync,readdirSync,writeFileSync} from 'node:fs';
      import {setTimeout as delay} from 'node:timers/promises';
      import {attemptDocker,requireAttemptNetwork} from './dist/src/runtime/docker-network.js';
      import {attemptDatabaseIdentity} from './dist/src/stacks/hosted-database-identity.js';
      const state=${JSON.stringify(state)}, names=['postgres','mongodb'];
      const children=names.map((backend,index)=>{
        const child=spawn(process.execPath,['--input-type=module','-e',${JSON.stringify(worker)}],
          {stdio:'inherit',env:{...process.env,PROBE_BACKEND:backend,PROBE_INDEX:String(index),
            PROBE_STATE:state,PROBE_PREFIX:${JSON.stringify(prefix)}}});
        return {child,done:new Promise(resolve=>child.once('close',resolve))};
      });
      let leases=[];
      try {
        const deadline=Date.now()+90_000;
        while(!names.every(name=>existsSync(state+'/'+name+'/ready'))) {
          assert.ok(children.every(value=>value.child.exitCode===null),'both activation workers remain alive');
          assert.ok(Date.now()<deadline,'both attempts activated before the deadline');
          await delay(50);
        }
        leases=names.map(name=>JSON.parse(readFileSync(state+'/'+name+'/lease.json','utf8')));
        assert.notEqual(leases[0].resources.network.id,leases[1].resources.network.id);
        assert.notEqual(leases[0].ownershipToken,leases[1].ownershipToken);
        assert.ok(leases.every(lease=>lease.resources.locks.every(lock=>!lock.key.startsWith('capacity:'))));
        for(const [index,lease] of leases.entries()) {
          requireAttemptNetwork(lease);
          const other=leases[1-index], secret=attemptDatabaseIdentity(lease.ownershipToken);
          const id=lease.resources.container.id;
          if(index===0) attemptDocker(['exec','-e','PGPASSWORD='+secret.password,id,'psql','-h','127.0.0.1',
            '-U','appuser','-d','concurrent_probe','-v','ON_ERROR_STOP=1','-c','CREATE TABLE proof(id integer); INSERT INTO proof VALUES(1);']);
          else attemptDocker(['exec',id,'mongosh','--quiet','--username','appuser','--password',secret.password,
            '--authenticationDatabase','concurrent_probe','concurrent_probe','--eval','db.proof.insertOne({id:1}); if(db.proof.countDocuments()!==1) quit(1)']);
          const otherNetworks=JSON.parse(attemptDocker(['inspect','--format','{{json .NetworkSettings.Networks}}',other.resources.container.id]));
          const otherAddress=Object.values(otherNetworks).find(value=>value.NetworkID===other.resources.network.id).IPAddress;
          const cache=lease.resources.network.services[0];
          const urls=['http://127.0.0.1:18080','http://'+cache.address+':4873/-/ping','http://'+otherAddress+':18080'];
          const result=attemptDocker(['exec',lease.resources.browserContainer.id,'node','-e',
            'Promise.all('+JSON.stringify(urls)+'.map(async u=>{try{await fetch(u,{signal:AbortSignal.timeout(1500)});return true}catch{return false}})).then(r=>console.log(JSON.stringify(r)))']);
          assert.deepEqual(JSON.parse(result),[true,true,false],'own app/cache work; other live attempt is denied');
          const ports=[index===0?5432:27017,index===0?27017:5432];
          const tcp=attemptDocker(['exec',lease.resources.browserContainer.id,'node','-e',
            'const net=require("node:net"); Promise.all('+JSON.stringify([['127.0.0.1',ports[0]],[otherAddress,ports[1]]])+'.map(([host,port])=>new Promise(ok=>{const s=net.createConnection({host,port});s.setTimeout(1500);s.on("connect",()=>{s.destroy();ok(true)});s.on("timeout",()=>{s.destroy();ok(false)});s.on("error",()=>ok(false))}))).then(r=>console.log(JSON.stringify(r)))']);
          assert.deepEqual(JSON.parse(tcp),[true,false],'own native DB is live; other native DB is denied');
        }
      } finally {
        writeFileSync(state+'/release','release');
        assert.deepEqual(await Promise.all(children.map(value=>value.done)),[0,0],'both workers release cleanly');
      }
      for(const lease of leases) {
        for(const key of ['container','browserContainer']) assert.throws(()=>attemptDocker(['inspect',lease.resources[key].id]));
        assert.throws(()=>attemptDocker(['network','inspect',lease.resources.network.id]));
        assert.equal(JSON.parse(readFileSync(state+'/'+lease.backend+'/lease.json','utf8')).state,'released');
      }
      assert.equal(readdirSync(state+'/locks').filter(name=>name.endsWith('.lock.json')).length,0);
      console.log('two concurrent admitted native attempts: writes, isolation, and exact cleanup passed');
      `]);
    const output = docker(['start', '--attach', controller]);
    assert.match(output, /two concurrent admitted native attempts: writes, isolation, and exact cleanup passed/);
    console.log(output);
  } finally {
    if (controller) docker(['rm', '-f', '--volumes', controller]);
    assert.equal(docker(['inspect', '--format', '{{.Id}}', 'stack-bench-npm-cache']), cache);
    assert.equal(docker(['inspect', '--format', '{{json .NetworkSettings.Networks}}', cache]), memberships);
    docker(['volume', 'rm', volume]);
  }
});
