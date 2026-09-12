import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { randomBytes } from 'node:crypto';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

// Explicit, model-free gate. Uses installed immutable images and briefly attaches
// the trusted cache to each owned bridge. It does not reset or stop the cache.
for (const backend of ['postgres', 'mongodb', 'spacetime']) test(`owned ${backend} activation, native authentication, smoke, restart, and exact cleanup`, {
  skip: process.env.STACK_BENCH_OWNED_BACKEND_TEST !== '1', timeout: 240_000,
}, () => {
  const image = process.env.STACK_BENCH_NETWORK_CONTROLLER_IMAGE;
  assert.match(image ?? '', /^sha256:[a-f0-9]{64}$/);
  const prefix = `stack-bench-owned-probe-${randomBytes(6).toString('hex')}`;
  const docker = (args: string[], input?: string): string => {
    const result = spawnSync('docker', args, { encoding: 'utf8', input, timeout: 210_000 });
    assert.equal(result.status, 0, `${args.slice(0, 3).join(' ')}: ${result.stderr || result.error}\n${result.stdout}`);
    return result.stdout.trim();
  };
  const cache = docker(['inspect', '--format', '{{.Id}}', 'stack-bench-npm-cache']);
  const networksBefore = docker(['inspect', '--format', '{{json .NetworkSettings.Networks}}', cache]);
  const volume = docker(['volume', 'create', `${prefix}-state`]);
  const state = docker(['volume', 'inspect', '--format', '{{.Mountpoint}}', volume]);
  let controller: string | undefined;
  try {
    controller = docker(['create', '--name', `${prefix}-controller`, '--network', 'host',
      '--mount', 'type=bind,source=/var/run/docker.sock,target=/var/run/docker.sock',
      '--mount', `type=bind,source=${state},target=${state}`,
      '--mount', `type=bind,source=${fileURLToPath(new URL('../', import.meta.url))},target=/opt/stack-bench/dist,readonly`,
      '-e', 'STACK_BENCH_APPLIANCE=1', '-e', `STACK_BENCH_CONTROLLER_IMAGE_ID=${image}`,
      '--entrypoint', 'node', image!, '--input-type=module', '-e', `
      import assert from 'node:assert/strict';
      import { mkdirSync } from 'node:fs';
      import { createBackendLease, writeBackendLease, readBackendLease } from './dist/src/runtime/backend-lease.js';
      import { releaseBackendLease } from './dist/src/runtime/backend-teardown.js';
      import { loadTrack, portsFor } from './dist/src/composition/tracks.js';
      import { activateAttemptBackend } from './dist/src/stacks/hosted-lifecycle.js';
      import { controlSpacetime } from './dist/src/stacks/spacetime-lifecycle.js';
      import { attemptDatabaseIdentity } from './dist/src/stacks/hosted-database-identity.js';
      import { resetMongoDb } from './dist/src/stacks/backends/mongodb-operations.js';
      import { requireLeasedDatabase } from './dist/src/stacks/backend-reset-guard.js';
      import { attemptDocker, requireAttemptNetwork } from './dist/src/runtime/docker-network.js';
      import { runContainerSmoke } from './dist/src/runtime/container-smoke.js';
      for (const backend of [${JSON.stringify(backend)}]) {
        const directory = ${JSON.stringify(state)} + '/' + backend;
        mkdirSync(directory, {recursive:true});
        const path = directory + '/lease.json';
        const lease = createBackendLease({runId:${JSON.stringify(prefix)}+'-'+backend,
          backend,track:'chat',runIndex:20,
          database:backend==='spacetime'?null:'owned_probe',
          serverUri:backend==='spacetime'?'http://127.0.0.1:3299':null,
          module:backend==='spacetime'?'owned-probe':null,
          dataDir:backend==='spacetime'?directory+'/data':null});
        writeBackendLease(path,lease,{exclusive:true});
        let active;
        try {
          activateAttemptBackend({leasePath:path,lease,ports:portsFor(loadTrack(lease.track),backend,lease.runIndex)});
          active=readBackendLease(path,{token:lease.ownershipToken,active:true});
          const id=active.resources.container.id;
          const secret=attemptDatabaseIdentity(lease.ownershipToken);
          if(backend==='postgres') {
            const query='CREATE TABLE proof(id integer); INSERT INTO proof VALUES(1); SELECT count(*) FROM proof;';
            attemptDocker(['exec','-e','PGPASSWORD='+secret.password,id,'psql','-h','127.0.0.1','-U','appuser','-d','owned_probe','-v','ON_ERROR_STOP=1','-c',query]);
          } else if(backend==='mongodb') {
            const applicationShell=['exec',id,'mongosh','--quiet','--username','appuser','--password',secret.password,
              '--authenticationDatabase','owned_probe','owned_probe','--eval'];
            attemptDocker([...applicationShell,'db.proof.insertOne({id:1}); if(db.proof.countDocuments()!==1) quit(1)']);
            attemptDocker([...applicationShell,
              'if(db.hello().setName!=="rs0" || !db.hello().isWritablePrimary) throw new Error("not a primary replica-set member"); '
              +'const session=db.getMongo().startSession(); const tx=session.getDatabase("owned_probe"); '
              +'session.startTransaction(); tx.proof.insertOne({id:10}); session.commitTransaction(); '
              +'session.startTransaction(); tx.proof.insertOne({id:11}); session.abortTransaction(); session.endSession(); '
              +'if(!db.proof.findOne({id:10}) || db.proof.findOne({id:11})) throw new Error("transaction commit or rollback failed");']);
            attemptDocker(['exec','--user','mongodb',id,'mongod','--shutdown','--dbpath','/data/db']);
            attemptDocker(['exec','-d','--user','mongodb',id,'sh','-c','exec mongod --bind_ip 127.0.0.1 --replSet rs0 --keyFile /data/configdb/stack-bench-keyfile > /tmp/stack-bench-mongodb-restart.log 2>&1']);
            let restarted=false;
            for(let retry=0;retry<60;retry++) {
              try {
                attemptDocker([...applicationShell,'if(!db.hello().isWritablePrimary) quit(1); if(!db.proof.findOne({id:10})) throw new Error("committed data lost after restart");']);
                restarted=true;
                break;
              } catch { await new Promise(resolve=>setTimeout(resolve,500)); }
            }
            if(!restarted) console.error(attemptDocker(['exec',id,'tail','-n','20','/tmp/stack-bench-mongodb-restart.log']));
            assert.equal(restarted,true,'MongoDB did not recover committed data as primary after restart');
            resetMongoDb({lease:requireLeasedDatabase(active)});
            attemptDocker([...applicationShell,
              'if(db.proof.countDocuments()!==0) throw new Error("reset left application data"); '
              +'db.proof.insertOne({id:2}); if(!db.proof.findOne({id:2})) throw new Error("read failed"); '
              +'if(db.proof.updateOne({id:2},{$set:{id:3}}).modifiedCount!==1) throw new Error("update failed"); '
              +'if(db.proof.deleteOne({id:3}).deletedCount!==1) throw new Error("delete failed"); '
              +'let denied; try { db.dropDatabase(); } catch(error) { denied=error.code; } '
              +'if(denied!==13) throw new Error("application role did not reject dropDatabase");']);
            console.log('mongodb: trusted reset preserved application CRUD and denied dropDatabase');
          } else {
            const started=active.resources.network.namespaceStartedAt;
            await controlSpacetime({lease:active});
            assert.equal(readBackendLease(path).resources.network.namespaceStartedAt,started);
            assert.equal(requireAttemptNetwork(active),'container:'+id);
          }
          const smoke=runContainerSmoke({command:(_file,args)=>attemptDocker(args),
            imageId:process.env.STACK_BENCH_CONTROLLER_IMAGE_ID,resultsDir:directory,
            destinations:['http://'+active.resources.network.services[0].address+':4873/-/ping'],
            tcpPorts:[backend==='postgres'?5432:backend==='mongodb'?27017:3299],
            requiredExecutables:['node'],credentialStatusCommand:null,credentialMount:null,
            credentialEnvironment:null,marker:'smoke-proof',networkMode:requireAttemptNetwork(active),
            leaseContext:{path,lease:active}});
          assert.equal(smoke.tcpReached.length,1);
          console.log(backend+': native backend, isolated smoke'+(backend==='spacetime'?', process restart':'')+' passed');
        } catch(error) {
          const current=readBackendLease(path);
          if(current.resources.container) {
            try { console.error(attemptDocker(['exec',current.resources.container.id,'tail','-n','30','/tmp/stack-bench-backend.log'])); } catch {}
          }
          throw error;
        } finally {
          assert.equal(releaseBackendLease(path,lease.ownershipToken),true);
          const released=readBackendLease(path);
          assert.equal(released.state,'released');
          for(const kind of ['container','browserContainer']) {
            const resource=released.resources[kind];
            if(resource) assert.throws(()=>attemptDocker(['inspect',resource.id]));
          }
          if(released.resources.network) assert.throws(()=>attemptDocker(['network','inspect',released.resources.network.id]));
        }
      }
      `]);
    const output = docker(['start', '--attach', controller]);
    if (backend === 'mongodb') assert.match(output, /mongodb: trusted reset preserved application CRUD and denied dropDatabase/);
    if (backend === 'spacetime') assert.match(output, /spacetime: native backend, isolated smoke, process restart passed/);
    console.log(output);
  } finally {
    if (controller) docker(['rm', '-f', '--volumes', controller]);
    assert.equal(docker(['inspect', '--format', '{{.Id}}', 'stack-bench-npm-cache']), cache);
    assert.equal(docker(['inspect', '--format', '{{json .NetworkSettings.Networks}}', cache]), networksBefore);
    docker(['volume', 'rm', volume]);
  }
});
