import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { randomUUID } from 'node:crypto';
import { createInterface } from 'node:readline';
import { setTimeout as sleep } from 'node:timers/promises';
import { startTransport } from './client_transport.mjs';

const [host, database, language, outputDirectory] = process.argv.slice(2);
assert.ok(['typescript', 'csharp'].includes(language));
const containerName = `llm-client-${randomUUID()}`;
const transport = await startTransport(host, database);
// Only compiled client files enter the container. No checkout, keys, host environment,
// Docker socket, or writable host mount is available to generated code.
const child = spawn(
  'docker',
  [
    'run',
    '--rm',
    '--pull=never',
    '--init',
    '-i',
    '--name',
    containerName,
    '--read-only',
    '--cap-drop=ALL',
    '--security-opt=no-new-privileges',
    '--user=65534:65534',
    '--pids-limit=128',
    '--memory=1g',
    '--cpus=2',
    '--tmpfs=/tmp:rw,nosuid,size=64m',
    `--network=container:${transport.name}`,
    '--mount',
    `type=bind,source=${outputDirectory},target=/app,readonly`,
    '--workdir=/app',
    language === 'typescript'
      ? 'node:22-bookworm-slim'
      : 'mcr.microsoft.com/dotnet/runtime:10.0',
    // The container also expires if the outer scorer is killed before its finally block.
    'timeout',
    '--kill-after=5',
    '150',
    language === 'typescript' ? 'node' : 'dotnet',
    language === 'typescript' ? '/app/client.cjs' : '/app/client.dll',
  ],
  { stdio: ['pipe', 'pipe', 'pipe'] }
);
let failure;
let nextId = 0;
const pending = new Map();
child.on('error', error => {
  failure = error;
});
child.on('exit', (code, signal) => {
  failure ??= new Error(`client exited: ${code ?? signal}`);
});
child.stdin.on('error', error => {
  failure ??= error;
});
let outputBytes = 0;
for (const stream of [child.stdout, child.stderr]) {
  stream.on('data', chunk => {
    outputBytes += chunk.length;
    if (outputBytes > 1024 * 1024) {
      failure = new Error('client output exceeded 1 MiB');
      child.stdout.destroy();
      child.stderr.destroy();
      child.kill();
    }
  });
}
child.stderr.pipe(process.stderr);
createInterface({ input: child.stdout }).on('line', line => {
  if (!line.startsWith('EVAL:')) {
    console.log(line);
    return;
  }
  try {
    const reply = JSON.parse(line.slice(5));
    const callback = pending.get(reply.id);
    if (callback) {
      pending.delete(reply.id);
      callback(reply);
    }
  } catch (error) {
    failure = error;
  }
});
async function rpc(command, extra = {}) {
  transport.check();
  if (failure) throw failure;
  const id = ++nextId;
  let timer;
  try {
    const response = await Promise.race([
      new Promise(resolve => {
        pending.set(id, resolve);
        child.stdin.write(JSON.stringify({ id, command, ...extra }) + '\n');
      }),
      new Promise((_, reject) => {
        timer = setTimeout(
          () => reject(failure ?? new Error(`client RPC timeout: ${command}`)),
          10000
        );
      }),
    ]);
    assert.equal(response.error, undefined, response.error);
    return response;
  } finally {
    clearTimeout(timer);
    pending.delete(id);
  }
}
async function until(predicate) {
  const deadline = Date.now() + 20000;
  let state;
  do {
    state = await rpc('snapshot');
    if (predicate(state)) return state;
    await sleep(50);
  } while (Date.now() < deadline);
  throw new Error(`client state timeout: ${JSON.stringify(state)}`);
}
let token;
async function serverValue() {
  const response = await fetch(`${host}/v1/database/${database}/sql`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}` },
    body: 'SELECT value FROM counter',
    signal: AbortSignal.timeout(10000),
  });
  assert.ok(response.ok, await response.clone().text());
  return (await response.json())[0].rows[0][0];
}
async function addOnServer(delta) {
  const response = await fetch(`${host}/v1/database/${database}/call/add`, {
    method: 'POST',
    headers: {
      Authorization: `Bearer ${token}`,
      'Content-Type': 'application/json',
    },
    body: JSON.stringify([delta]),
    signal: AbortSignal.timeout(10000),
  });
  assert.ok(response.ok, await response.text());
}
try {
  const tokenResponse = await fetch(`${host}/v1/identity`, {
    method: 'POST',
    signal: AbortSignal.timeout(10000),
  });
  assert.ok(tokenResponse.ok);
  ({ token } = await tokenResponse.json());
  await rpc('start', {
    uri: transport.uri,
    database,
  });
  const initial = await until(s => s.ready && s.value === 0);
  assert.ok(initial.identity, 'client identity missing');
  await rpc('add', { delta: 3 });
  const first = await until(s => s.outcome === 'committed' && s.value === 3);
  assert.equal(first.changes, 1);

  // Let the reducer commit, but do not deliver its result or cache update.
  transport.hideReplies(true);
  await rpc('add', { delta: 5 });
  const deadline = Date.now() + 10000;
  while ((await serverValue()) !== 8) {
    assert.ok(
      Date.now() < deadline,
      'pending reducer did not reach the server'
    );
    await sleep(50);
  }
  transport.setOnline(false);
  const offline = await until(s => !s.ready && s.outcome === 'unknown');
  assert.equal(
    offline.value,
    3,
    'cached value must remain readable while disconnected'
  );
  assert.equal(offline.changes, 1);
  await addOnServer(7);
  transport.hideReplies(false);
  transport.setOnline(true);
  const recovered = await until(s => s.ready && s.value === 15);
  assert.equal(
    recovered.identity,
    initial.identity,
    'reconnect changed identity'
  );
  assert.equal(
    recovered.outcome,
    'unknown',
    'reconnect must not turn uncertainty into success'
  );
  assert.equal(
    recovered.changes,
    2,
    'subscription replay must produce one net cache update'
  );
  assert.equal(
    await serverValue(),
    15,
    'unknown non-idempotent call was retried'
  );
  await rpc('add', { delta: 2 });
  const last = await until(s => s.outcome === 'committed' && s.value === 17);
  assert.equal(
    last.changes,
    3,
    'callbacks or subscriptions were registered twice'
  );
  await rpc('stop');
  await addOnServer(11);
  await sleep(1500);
  const stopped = await rpc('snapshot');
  assert.equal(stopped.ready, false);
  assert.equal(stopped.changes, 3, 'stopped client kept receiving updates');
  console.log('CLIENT_RECOVERY_OK');
} finally {
  const cleanup = spawnSync('docker', ['rm', '-f', containerName], {
    timeout: 10000,
    encoding: 'utf8',
  });
  if (
    cleanup.status !== 0 &&
    !/No such container|removal of container .* is already in progress/.test(
      cleanup.stderr ?? ''
    )
  ) {
    console.error(
      `Client container cleanup failed: ${cleanup.error ?? cleanup.stderr}`
    );
    process.exitCode = 1;
  }
  child.kill();
  transport.close();
}
