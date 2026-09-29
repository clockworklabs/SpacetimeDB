import assert from 'node:assert/strict';
import { execFile } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { promisify } from 'node:util';
import test from 'node:test';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction } from '../src/actions/action-contract.js';
import { checkoutStateSchema, type CheckoutState } from '../src/stacks/checkout-state.js';
import { prepareRuntimeCrash, recoverRuntimeCrash } from '../src/runtime/backend-control.js';
import { prepareProcessCrash } from '../src/stacks/process-crash.js';
import { activateAttemptBackend } from '../src/stacks/hosted-lifecycle.js';
import { waitFor, answers } from '../src/stacks/lifecycle-readiness.js';
import { backendResourceLockKeys, claimBackendResources, createBackendLease, publicBackendLease,
  readBackendLease, resourceLockScope, updateBackendLease } from '../src/runtime/backend-lease.js';
import { releaseBackendLease } from '../src/runtime/backend-teardown.js';
import { attemptDocker, attemptControllerImage, ATTEMPT_CREATION_LABEL, recordAttemptCreation,
  requireAttemptNetwork } from '../src/runtime/docker-network.js';
import { CODING_CONTAINER_AGENT } from '../src/runtime/coding-container-policy.js';

// No package installation or model. Real sockets and processes run in the normal
// leased namespace. The state oracle below is a prepared, unchanged cart: this
// fixture qualifies fault admission, not a new checkout/database implementation.
const fixture = `import http from 'node:http';
import fs from 'node:fs';
import {spawn} from 'node:child_process';
const mode=fs.readFileSync('/app/mode','utf8').trim();
const port=Number(process.env.PORT), api=port+1;
const role=process.argv[2]||'entry';
const aux=[];
const ready=()=>{
  if(role!=='api') for(let i=0;i<36;i++) aux.push(spawn('sleep',['infinity']).pid);
  if(mode==='late-auxiliary'&&role==='entry') aux.push(spawn(process.execPath,[import.meta.filename,'api'],{stdio:'inherit'}).pid);
  fs.writeFileSync('/tmp/'+role+'-ready.json',JSON.stringify({pid:process.pid,port:role==='api'?api:port,aux}));
};
const handle=(req,res)=>{
  if(req.url==='/health') return res.end('ready');
  fs.appendFileSync('/tmp/entered','request\\n');
  // Held until the real service SIGKILL. There is no fabricated HTTP outcome.
};
if(mode==='late-auxiliary'&&role==='api'){
  fs.writeFileSync('/tmp/aux-ready','ready');
  const poll=setInterval(()=>{if(fs.existsSync('/tmp/listen-now')){
    clearInterval(poll);http.createServer(handle).listen(api,'0.0.0.0',ready);
  }},1);
}else if(mode==='missing'){ready();setInterval(()=>{},1000);}
else if(mode==='proxy'&&role==='entry'){
  const upstream=spawn(process.execPath,[import.meta.filename,'api'],{stdio:'inherit'});
  const server=http.createServer((req,res)=>{
    if(req.url==='/health') return res.end('ready');
    const call=http.request({host:'127.0.0.1',port:api,path:req.url,method:req.method},reply=>reply.pipe(res));
    // Like a proxy with an outstanding upstream operation, do not synthesize a
    // completed checkout response when its API dies; entry death closes clients.
    call.on('error',()=>{});req.pipe(call);
  });
  server.listen(port,'0.0.0.0',ready);upstream.on('error',error=>{throw error;});
}else http.createServer(handle).listen(role==='api'?api:port,'0.0.0.0',ready);
`;

const sha = (value: string | Buffer) => createHash('sha256').update(value).digest('hex');
const asyncExec = promisify(execFile);

// Run from a Linux controller with the normal Docker socket/cache and pinned
// STACK_BENCH_CONTROLLER_IMAGE_ID. Keep this off the ordinary unit suite.
test('hosted crash uses owned writer freeze while preserving complete process quiescence', {
  skip: process.env.STACK_BENCH_HOSTED_CRASH_TEST !== '1', timeout: 300_000,
}, async t => {
  assert.equal(process.platform, 'linux');
  const output = process.env.STACK_BENCH_HOSTED_CRASH_EVIDENCE;
  assert(output, 'an evidence output path is required');
  const image = attemptControllerImage();
  const buildImage = process.env.STACK_BENCH_HOSTED_CRASH_BUILD_IMAGE;
  assert.match(image, /^sha256:[a-f0-9]{64}$/);
  assert.match(buildImage ?? '', /^sha256:[a-f0-9]{64}$/, 'use the actual pinned coding/build image');
  const port = Number(process.env.STACK_BENCH_HOSTED_CRASH_PORT ?? 14570);
  assert(Number.isInteger(port) && port > 1024 && port < 65534);
  mkdirSync(dirname(output), { recursive: true });
  // The private lease must survive a stopped controller's ephemeral /tmp.
  const root = mkdtempSync(join(dirname(output), 'stack-bench-hosted-crash-'));
  const leasePath = join(root, 'lease.json'), app = join(root, 'app');
  mkdirSync(app);
  const start = '#!/bin/sh\ncd /app\nexec node fixture.mjs\n';
  writeFileSync(join(app, 'start.sh'), start);
  writeFileSync(join(app, 'fixture.mjs'), fixture);
  const ports = { vite: port, express: port + 1, dbPort: null };
  const limits = { cpuCount: 1, memoryBytes: 512 * 1024 * 1024, memorySwapBytes: 512 * 1024 * 1024, pids: 256 };
  const lease = createBackendLease({ runId: root.split('/').at(-1)!, backend: 'postgres',
    track: 'ecommerce', runIndex: 0, database: 'crash_window' });
  const previous = { lease: process.env.STACK_BENCH_LEASE, token: process.env.STACK_BENCH_LEASE_TOKEN };
  const evidence: Record<string, unknown> = { result: 'running', image, buildImage, ports, fixtureSha256: sha(fixture),
    rerun: 'STACK_BENCH_HOSTED_CRASH_TEST=1 STACK_BENCH_HOSTED_CRASH_EVIDENCE=<file> node --test --test-name-pattern="hosted crash uses owned" dist/tests/hosted-crash-window.integration.js',
    runtimeSha256: Object.fromEntries(['../src/actions/crash-action-executors.js', '../src/stacks/process-crash.js',
      '../src/runtime/backend-control.js', '../src/stacks/hosted-lifecycle.js'].map(path =>
      [path, sha(readFileSync(new URL(path, import.meta.url)))])), cases: [] };
  const save = () => writeFileSync(output, JSON.stringify(evidence, null, 2));
  let build = '', network = '';
  const spec = { backend: 'postgres', app, port, probe: '/health' };
  const uid = `${CODING_CONTAINER_AGENT.uid}:${CODING_CONTAINER_AGENT.gid}`;
  const exec = (args: string[]) => attemptDocker(['exec', build, ...args]);
  const json = (file: string) => JSON.parse(exec(['cat', file])) as { pid: number; port: number; aux: number[] };
  const killUser = () => exec(['sh', '-c', `pkill -KILL -u ${CODING_CONTAINER_AGENT.uid} || [ "$?" = 1 ]`]);
  const prepareMode = async (mode: string, foreign = false) => {
    killUser();
    exec(['sh', '-c', 'rm -f /tmp/entry-ready.json /tmp/api-ready.json /tmp/entered /tmp/respawn.json /tmp/aux-ready /tmp/listen-now']);
    attemptDocker(['exec', '-i', build, 'sh', '-c', 'cat > /app/mode'], mode);
    attemptDocker(['exec', '-d', '--user', foreign ? '0:0' : uid, '-e', `PORT=${port}`, build,
      'node', '/app/fixture.mjs']);
    await waitFor(async () => {
      try { json('/tmp/entry-ready.json'); return mode !== 'proxy' || !!json('/tmp/api-ready.json'); }
      catch { return false; }
    }, 10_000, `${mode} fixture processes`);
    if (mode !== 'missing') await waitFor(() => answers(`http://127.0.0.1:${port}/health`), 10_000, 'entry listener');
  };
  let failure: { error: unknown } | undefined;
  try {
    claimBackendResources(leasePath, lease, { ...resourceLockScope(), keys: backendResourceLockKeys(lease, ports) });
    activateAttemptBackend({ leasePath, lease, ports });
    let active = readBackendLease(leasePath, { token: lease.ownershipToken, active: true });
    network = active.resources.network!.id;
    const intent = recordAttemptCreation(leasePath, active, 'build');
    const networkMode = requireAttemptNetwork(active);
    build = attemptDocker(['create', '--name', intent.name, '--label', `${ATTEMPT_CREATION_LABEL}=${intent.creationToken}`,
      '--network', networkMode, '--init', '--cap-drop', 'ALL', '--cap-add', 'CHOWN', '--cap-add', 'DAC_OVERRIDE',
      '--cap-add', 'FOWNER', '--cap-add', 'KILL',
      '--cap-add', 'SETUID', '--cap-add', 'SETGID', '--security-opt', 'no-new-privileges:true',
      '--cpus', String(limits.cpuCount), '--memory', String(limits.memoryBytes),
      '--memory-swap', String(limits.memorySwapBytes), '--pids-limit', String(limits.pids),
      '--entrypoint', 'sleep', buildImage!, 'infinity']);
    try {
      updateBackendLease(leasePath, { token: lease.ownershipToken }, next => {
        next.resources.buildContainer = { name: intent.name, id: build, image: buildImage!, owned: true, networkMode,
          resourceLimits: limits }; return next;
      });
    } catch (error) {
      // This exact, newly created container has not started and has no app files.
      attemptDocker(['rm', build]); throw error;
    }
    const hostConfig = JSON.parse(attemptDocker(['inspect', '--format', '{{json .HostConfig}}', build]));
    const actualLimits = { cpuCount: hostConfig.NanoCpus / 1e9, memoryBytes: hostConfig.Memory,
      memorySwapBytes: hostConfig.MemorySwap, pids: hostConfig.PidsLimit };
    evidence.resourceLimits = { declared: limits, observed: actualLimits };
    assert.deepEqual(hostConfig.CapAdd.map((capability: string) => capability.replace(/^CAP_/, '')).sort(),
      ['CHOWN', 'DAC_OVERRIDE', 'FOWNER', 'KILL', 'SETGID', 'SETUID']);
    assert.deepEqual(actualLimits, limits, 'lease limits must match the created container');
    attemptDocker(['start', build]);
    assert.equal(exec(['id', '-u', CODING_CONTAINER_AGENT.name]), String(CODING_CONTAINER_AGENT.uid));
    assert.equal(exec(['id', '-g', CODING_CONTAINER_AGENT.name]), String(CODING_CONTAINER_AGENT.gid));
    exec(['sh', '-c', 'mkdir -p /app /run/application; chmod 777 /app /run/application']);
    for (const [name, source] of [['fixture.mjs', fixture], ['start.sh', start]]) {
      attemptDocker(['exec', '-i', build, 'sh', '-c', `cat > /app/${name}`], source);
    }
    exec(['chmod', '755', '/app/start.sh']);
    process.env.STACK_BENCH_LEASE = leasePath;
    process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
    active = readBackendLease(leasePath, { token: lease.ownershipToken, active: true });
    evidence.lease = publicBackendLease(active); save();
    for (const mode of ['direct', 'proxy', 'late-auxiliary'] as const) await t.test(mode, async () => {
      await prepareMode(mode);
      const entry = json('/tmp/entry-ready.json'), api = mode === 'proxy' ? json('/tmp/api-ready.json') : null;
      const before: CheckoutState = { accountId: 'a', itemId: 'i', priceMinor: 100, cart: [],
        stock: [{ warehouseId: 'w', quantity: 10 }], reservations: [], orders: [], payments: [], orphanOrderLines: 0 };
      const prepared = structuredClone(before); prepared.cart.push({ itemId: 'i', quantity: 1 });
      const wrap = (state: CheckoutState) => ({ state: checkoutStateSchema.parse(state), schemaSha256: { fixture: sha(fixture) },
        account: 'a', item: 'i', recordedAtMs: Date.now() });
      let quietBeforeRecovery = false;
      const result = await executeAction(ACTION_REGISTRY, 'crashCheckout', {
        do: 'crashCheckout', actor: 'buyer', before: 'before', prepared: 'prepared', quantity: 1,
        requests: 16, offsetMs: 0, target: 'application',
      }, { capabilities: {
        actors: { get: () => ({ name: 'buyer', page: { evaluate: async () => [] }, context: { cookies: async () => [] },
          writes: [{ url: `http://127.0.0.1:${port}/session`, headers: { authorization: 'Bearer fixture' } }] }) },
        'named-actions': { now: Date.now, sleep: async () => {}, resolve: () => ({ id: 'checkout' }),
          request: () => ({ url: `http://127.0.0.1:${port}/api/checkout` }), fetch },
        'database-read': { checkoutSnapshots: new Map([['before', wrap(before)], ['prepared', wrap(prepared)]]),
          markCheckoutUnsettled: () => {}, getCheckoutState: () => wrap(prepared) },
        'browser-observation': { recorded: new Map() },
        'process-crash': { prepare: async () => {
          const runtime = await prepareRuntimeCrash(spec, 'application');
          return { ...runtime, crash: async () => {
            await waitFor(async () => {
              try { await asyncExec('docker', ['exec', build, 'test', '-s', '/tmp/entered']); return true; }
              catch { return false; }
            }, 10_000, 'real checkout arrival');
            if (mode === 'late-auxiliary') {
              // This existing writer had no listening socket when ARMED. It
              // must still belong to the complete writer freeze boundary.
              exec(['touch', '/tmp/listen-now']);
              await waitFor(async () => { try { json('/tmp/api-ready.json'); return true; } catch { return false; } },
                5000, 'existing auxiliary becomes a listener after ARMED');
            }
            return runtime.crash();
          }, recover: async (signal: AbortSignal) => {
            const states = exec(['sh', '-c', `ps -u ${CODING_CONTAINER_AGENT.uid} -o stat= || [ "$?" = 1 ]`]);
            quietBeforeRecovery = states.split(/\s+/).filter(Boolean).every(state => state.startsWith('Z'));
            assert(quietBeforeRecovery, 'all application processes must be quiet before the real DB drain/restart');
            return runtime.recover(signal);
          } };
        } },
      } });
      (evidence.cases as unknown[]).push({ mode, entry, api, quietBeforeRecovery, result }); save();
      assert.equal(result.status, 'passed', JSON.stringify(result));
      const observed = result.observation as { receipt: {
        applicationFreeze?: { startedAtMs: number; completedAtMs: number;
          processes: Array<{ pid: number; startTicks: number; threads: number[] }> };
        processEvidence: string; clockOffsetBeforeMs: number;
      }; outcomes: Array<{ startedAtMs: number; completedAtMs: number }>;
      databaseDrain: { settled: boolean }; outstandingAtFault: number };
      assert(quietBeforeRecovery && observed.databaseDrain.settled);
      assert(observed.outstandingAtFault > 0);
      const freeze = observed.receipt.applicationFreeze;
      assert(freeze, 'application admission requires the verified all-writer freeze');
      assert(freeze.startedAtMs <= freeze.completedAtMs);
      assert.deepEqual(freeze.processes.map(row => row.pid).sort((a, b) => a - b),
        [entry.pid, ...entry.aux, ...(api ? [api.pid] : [])].sort((a, b) => a - b));
      for (const writer of freeze.processes) {
        assert(writer.startTicks > 0 && writer.threads.includes(writer.pid));
        assert(writer.threads.every(tid => Number.isSafeInteger(tid) && tid > 1));
      }
      assert(freeze.processes.find(row => row.pid === entry.pid)!.threads.length > 1,
        'Node worker threads must be verified as well as the process leader');
      const kills = [...observed.receipt.processEvidence.matchAll(/^KILLED \d+ \d+ (\d+)$/gm)].map(row => Number(row[1]));
      assert(freeze.completedAtMs <= Math.min(...kills), 'no KILL may precede the all-stopped observation');
      assert(observed.outcomes.some(row => row.startedAtMs <= freeze.startedAtMs - observed.receipt.clockOffsetBeforeMs
        && row.completedAtMs >= freeze.completedAtMs - observed.receipt.clockOffsetBeforeMs),
      'real held HTTP requests must span the full verified freeze');
      for (const pid of entry.aux) assert.match(observed.receipt.processEvidence, new RegExp(`^KILLED ${pid} `, 'm'));
      assert.equal(sha(exec(['cat', '/app/fixture.mjs']) + '\n'), sha(fixture), 'frozen fixture source survives recovery');
    });
    await t.test('disarming before CRASH leaves the application running', async () => {
      await prepareMode('direct');
      const entry = json('/tmp/entry-ready.json');
      const runtime = await prepareRuntimeCrash(spec, 'application');
      await runtime.close();
      const states = exec(['sh', '-c', `ps -u ${CODING_CONTAINER_AGENT.uid} -o stat=`]).split(/\s+/).filter(Boolean);
      const healthy = await answers(`http://127.0.0.1:${port}/health`);
      (evidence.cases as unknown[]).push({ mode: 'disarm', entry, states, healthy }); save();
      assert(healthy && states.length > 0 && states.every(state => !/^[TtZX]/.test(state)));
    });
    for (const mode of ['missing', 'foreign', 'stale'] as const) await t.test(mode, async () => {
      await prepareMode(mode === 'missing' ? 'missing' : 'direct', mode === 'foreign');
      const entry = json('/tmp/entry-ready.json');
      if (mode === 'foreign') attemptDocker(['exec', '-d', '--user', uid, build, 'sleep', 'infinity']);
      let runtime: Awaited<ReturnType<typeof prepareRuntimeCrash>> | undefined;
      try {
        let error: unknown;
        try {
          runtime = await prepareRuntimeCrash(spec, 'application');
          if (mode === 'stale') {
            exec(['kill', '-KILL', String(entry.pid)]);
            attemptDocker(['exec', '-d', '--user', uid, '-e', `PORT=${port}`, build, 'node', '/app/fixture.mjs']);
            await waitFor(() => answers(`http://127.0.0.1:${port}/health`), 10_000, 'replacement listener');
            await runtime.crash();
          }
        } catch (caught) { error = caught; }
        (evidence.cases as unknown[]).push({ mode, entry, rejected: !!error,
          error: error instanceof Error ? error.message : null }); save();
        assert(error, `${mode} entry must not establish an application fault boundary`);
      } finally {
        await runtime?.close();
        if (mode === 'foreign') for (const pid of [entry.pid, ...entry.aux]) exec(['sh', '-c', `kill -KILL ${pid} 2>/dev/null || true`]);
        killUser();
      }
    });
    await t.test('a new listener after arming belongs to the complete writer freeze', async () => {
      await prepareMode('direct');
      const entry = json('/tmp/entry-ready.json');
      const runtime = await prepareRuntimeCrash(spec, 'application');
      try {
        attemptDocker(['exec', '-d', '--user', uid, '-e', `PORT=${port}`, build, 'node', '/app/fixture.mjs', 'api']);
        await waitFor(async () => { try { json('/tmp/api-ready.json'); return true; } catch { return false; } },
          5000, 'new listener after ARMED');
        exec(['kill', '-0', String(entry.pid)]);
        const added = json('/tmp/api-ready.json');
        let receipt: unknown, failure: unknown;
        try { receipt = await runtime.crash(); } catch (error) {
          failure = error; receipt = error && typeof error === 'object' && 'receipt' in error ? error.receipt : null;
        }
        const state = exec(['sh', '-c', `ps -u ${CODING_CONTAINER_AGENT.uid} -o stat= || [ "$?" = 1 ]`]);
        const quiet = state.split(/\s+/).filter(Boolean).every(value => value.startsWith('Z'));
        (evidence.cases as unknown[]).push({ mode: 'added-listener', entry, added, quiet, receipt,
          error: failure instanceof Error ? failure.message : null }); save();
        assert(quiet, 'new service must still be stopped by complete UID quiescence');
        assert.equal(failure, undefined, 'a new writer before the freeze is included, not silently omitted');
        const frozen = receipt as { applicationFreeze?: { processes: Array<{ pid: number }> } };
        assert(frozen.applicationFreeze?.processes.some(row => row.pid === added.pid),
          'the late writer must have a verified stopped-state receipt');
      } finally { await runtime.close(); killUser(); }
    });
    await t.test('a terminated leader cannot hide a live owned worker', async () => {
      await prepareMode('direct');
      // Perl and syscall.ph are already in the pinned coding image. SYS_exit
      // ends only this thread; threads->exit would terminate the whole process.
      const worker = `use threads; use Time::HiRes qw(sleep);
open my $p, ">", "/tmp/leader.pid" or die $!; print $p "$$\\n"; close $p;
threads->create(sub { my $n=0; while(1){ open my $f, ">", "/tmp/tick" or die $!; print $f ++$n; close $f; sleep 0.05; } });
require 'syscall.ph'; sleep 0.1; syscall(&SYS_exit, 0);
`;
      attemptDocker(['exec', '-i', build, 'sh', '-c', 'cat > /app/dead-leader.pl'], worker);
      attemptDocker(['exec', '-d', '--user', uid, build, 'perl', '/app/dead-leader.pl']);
      const observe = () => JSON.parse(exec(['node', '-e', `const fs=require('node:fs');
const pid=Number(fs.readFileSync('/tmp/leader.pid','utf8'));
const state=p=>{try{return fs.readFileSync(p+'/stat','utf8').split(') ')[1].split(' ')[0]}catch{return null}};
let tids=[];try{tids=fs.readdirSync('/proc/'+pid+'/task').map(Number)}catch{}
console.log(JSON.stringify({pid,state:state('/proc/'+pid),tick:fs.readFileSync('/tmp/tick','utf8'),
tasks:tids.map(tid=>({tid,state:state('/proc/'+pid+'/task/'+tid)}))}));`])) as {
        pid: number; state: string | null; tick: string; tasks: Array<{ tid: number; state: string | null }>;
      };
      await waitFor(async () => { try {
        const row = observe(); return row.state === 'Z' && row.tasks.some(task => task.tid !== row.pid && task.state === 'S');
      } catch { return false; } }, 5000, 'terminated leader with active worker');
      const before = observe();
      const runtime = await prepareRuntimeCrash(spec, 'application');
      try {
        let receipt: unknown, failure: unknown;
        try { receipt = await runtime.crash(); } catch (error) {
          failure = error; receipt = error && typeof error === 'object' && 'receipt' in error ? error.receipt : null;
        }
        const after = observe();
        (evidence.cases as unknown[]).push({ mode: 'dead-leader', fixtureSha256: sha(worker), before, after, receipt,
          error: failure instanceof Error ? failure.message : null }); save();
        assert.equal(failure, undefined, 'a live worker with a terminated leader remains a supported writer');
        assert(after.tasks.every(task => task.state === null || ['Z', 'X'].includes(task.state)),
          'QUIET must not leave the owned worker running');
        const frozen = receipt as { applicationFreeze?: { processes: Array<{ pid: number; threads: number[] }> } };
        const group = frozen.applicationFreeze?.processes.find(row => row.pid === before.pid);
        assert(group, 'the thread group must be in the freeze receipt');
        assert.deepEqual(group.threads, before.tasks.filter(task => task.state && !['Z', 'X'].includes(task.state)).map(task => task.tid));
        assert(!group.threads.includes(before.pid), 'the terminated leader is not a live frozen task');
      } finally { await runtime.close(); killUser(); }
    });
    await t.test('respawn cannot bypass complete quiescence', async () => {
      await prepareMode('direct');
      const entry = json('/tmp/entry-ready.json');
      const runtime = await prepareRuntimeCrash(spec, 'application');
      // One fixture supervisor outside the application UID starts a replacement
      // after the initial listener dies. The real rescan must stop that writer.
      const watcher = `const fs=require('node:fs'),{spawn}=require('node:child_process');
        fs.writeFileSync('/tmp/watch-ready','ready');
        const until=Date.now()+10000,t=setInterval(()=>{
          let gone=false;try{gone=/\\) [ZX] /.test(fs.readFileSync('/proc/${entry.pid}/stat','utf8'));}catch{gone=true;}
          if(gone){clearInterval(t);const p=spawn('setpriv',['--reuid=${CODING_CONTAINER_AGENT.uid}',
            '--regid=${CODING_CONTAINER_AGENT.gid}','--clear-groups','node','/app/fixture.mjs'],
            {detached:true,stdio:'ignore'});fs.writeFileSync('/tmp/respawn.json',JSON.stringify({pid:p.pid}));p.unref();}
          else if(Date.now()>until)process.exit(2);
        },10);`;
      attemptDocker(['exec', '-d', '-e', `PORT=${port}`, build, 'node', '-e', watcher]);
      await waitFor(async () => { try { exec(['test', '-f', '/tmp/watch-ready']); return true; } catch { return false; } },
        5000, 'one-shot fixture supervisor');
      try {
        let receipt: unknown, failure: unknown;
        try { receipt = await runtime.crash(); } catch (error) {
          failure = error; receipt = error && typeof error === 'object' && 'receipt' in error ? error.receipt : null;
        }
        await waitFor(async () => { try { json('/tmp/respawn.json'); return true; } catch { return false; } },
          5000, 'replacement process');
        const replacement = json('/tmp/respawn.json');
        const state = exec(['sh', '-c', `ps -u ${CODING_CONTAINER_AGENT.uid} -o stat= || [ "$?" = 1 ]`]);
        const quiet = state.split(/\s+/).filter(Boolean).every(value => value.startsWith('Z'));
        (evidence.cases as unknown[]).push({ mode: 'respawn', entry, replacement, quiet, receipt,
          error: failure instanceof Error ? failure.message : null }); save();
        assert(quiet, 'a replacement writer must not survive the real all-user rescan');
        assert(failure, 'changed service identities must not establish a measured service boundary');
      } finally { await runtime.close(); killUser(); }
    });
    await t.test('database boundary is unchanged', async () => {
      await prepareMode('direct');
      const armed = await prepareProcessCrash(active, 'database');
      try {
        const receipt = await armed.crash();
        const drain = await recoverRuntimeCrash(spec, 'database');
        (evidence.cases as unknown[]).push({ mode: 'database', receipt, drain }); save();
        assert.equal(receipt.target, 'database');
        assert.match(receipt.processEvidence, /^KILLED \d+ \d+ \d+$/m);
        assert.match(receipt.processEvidence, /^QUIET$/m);
        assert.equal(drain, null);
        assert(await answers(`http://127.0.0.1:${port}/health`));
      } finally { await armed.close(); }
    });
    evidence.result = 'cases finished; assertion results are in the accompanying TAP output';
  } catch (error) {
    evidence.result = 'failed'; evidence.error = error instanceof Error ? error.message : String(error);
    failure = { error };
  }
  for (const [key, value] of [['STACK_BENCH_LEASE', previous.lease], ['STACK_BENCH_LEASE_TOKEN', previous.token]]) {
    if (value === undefined) delete process.env[key!]; else process.env[key!] = value;
  }
  // A cleanup failure takes precedence over a case failure; both are in the evidence.
  try {
    const current = readBackendLease(leasePath, { token: lease.ownershipToken });
    const owned = [current.resources.buildContainer, current.resources.browserContainer, current.resources.container]
      .flatMap(container => container ? [container.id] : []);
    const released = releaseBackendLease(leasePath, lease.ownershipToken);
    const remaining = owned.filter(id => attemptDocker(['ps', '-aq', '--filter', `id=${id}`]));
    const networkRemaining = network ? attemptDocker(['network', 'ls', '-q', '--filter', `id=${network}`]) : '';
    evidence.cleanup = { released, network, owned, remaining, networkRemaining };
    assert(released && !remaining.length && !networkRemaining, 'all owned resources must be released');
  } catch (error) {
    evidence.cleanupError = error instanceof Error ? error.message : String(error); throw error;
  } finally { save(); }
  if (failure) throw failure.error;
});
