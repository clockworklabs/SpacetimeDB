import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { createHash, randomUUID } from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import { once } from 'node:events';
import { mkdir, writeFile, rm, mkdtemp } from 'node:fs/promises';
import { resolve, dirname } from 'node:path';
import { tmpdir } from 'node:os';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { SignatureV4 } from '@smithy/signature-v4';
import { Hash } from '@smithy/hash-node';

const cli = process.env.SPACETIME_CLI ?? 'spacetime';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const awsHost = 'proof.s3.us-east-1.amazonaws.com';
const runId = randomUUID(),
  database = `s3-recovery-${runId}`;
const directory = await mkdtemp(resolve(tmpdir(), 's3-recovery-')),
  dataDirectory = resolve(directory, 'server');
await mkdir(directory, { recursive: true });
const result = {
  runId,
  passed: false,
  checks: [],
  cleanup: {},
  provider: 'local fault-injection S3 receiver',
  versions: { node: process.version, spacetimedb: 'workspace SDK' },
  rerun: 'pnpm --filter @spacetimedb/s3 test:local',
};
const check = name => {
  result.checks.push(name);
  console.log(`PASS: ${name}`);
};
async function until(predicate, label, timeout = 35_000) {
  const deadline = Date.now() + timeout;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await delay(100);
  }
  throw new Error(`Timed out: ${label}`);
}
const credentials = {
  accessKeyId: 'AKIAIOSFODNN7EXAMPLE',
  secretAccessKey: 'wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY',
  sessionToken: 'dummy-session',
};
const objects = new Map(),
  faults = new Map(),
  copies = new Map();
let held,
  receiverError,
  listPages = 0,
  versioning = false,
  publicAccess = false;
const sha = body => createHash('sha256').update(body).digest('base64');
const object = body => ({
  body: Buffer.from(body),
  checksum: sha(body),
  etag: '"' + createHash('md5').update(body).digest('hex') + '"',
});
const xml = value =>
  value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;');
const s3 = createServer(async (req, res) => {
  try {
    const url = new URL(req.url, `http://${req.headers.host}`),
      key = decodeURIComponent(url.pathname.slice(1));
    if (req.method === 'POST') {
      const form = await new Request(url, {
        method: 'POST',
        body: req,
        duplex: 'half',
        headers: req.headers,
      }).formData();
      const policy = JSON.parse(
        Buffer.from(form.get('policy'), 'base64').toString()
      );
      const body = Buffer.from(await form.get('file').arrayBuffer());
      assert.ok(new Date(policy.expiration).getTime() > Date.now());
      assert.equal(sha(body), form.get('x-amz-checksum-sha256'));
      const range = policy.conditions.find(value => Array.isArray(value));
      assert.equal(body.length, range[1]);
      assert.equal(body.length, range[2]);
      objects.set(form.get('key'), object(body));
      res.writeHead(204);
      res.end();
      return;
    }
    const signed = url.searchParams.get('X-Amz-SignedHeaders');
    assert.ok(signed);
    const headers = Object.fromEntries(
      signed
        .split(';')
        .map(name => [name, name === 'host' ? awsHost : req.headers[name]])
    );
    const query = Object.fromEntries(
      [...url.searchParams].filter(
        ([name]) =>
          !name.startsWith('X-Amz-') && name !== 'x-amz-content-sha256'
      )
    );
    const stamp = url.searchParams.get('X-Amz-Date');
    const signingDate = new Date(
      stamp.replace(
        /^(\d{4})(\d{2})(\d{2})T(\d{2})(\d{2})(\d{2})Z$/,
        '$1-$2-$3T$4:$5:$6Z'
      )
    );
    const signer = new SignatureV4({
      credentials,
      region: 'us-east-1',
      service: 's3',
      sha256: Hash.bind(null, 'sha256'),
      uriEscapePath: false,
    });
    const expected = await signer.presign(
      {
        method: req.method,
        protocol: 'http:',
        hostname: awsHost,
        path: url.pathname,
        query,
        headers: { ...headers, 'x-amz-content-sha256': 'UNSIGNED-PAYLOAD' },
      },
      {
        signingDate,
        expiresIn: Number(url.searchParams.get('X-Amz-Expires')),
        unhoistableHeaders: new Set(Object.keys(headers)),
      }
    );
    assert.deepEqual(Object.fromEntries(url.searchParams), expected.query);
    if (url.searchParams.has('versioning')) {
      res.end(
        versioning
          ? '<VersioningConfiguration><Status>Enabled</Status></VersioningConfiguration>'
          : '<VersioningConfiguration/>'
      );
      return;
    }
    if (url.searchParams.has('publicAccessBlock')) {
      res.end(
        '<PublicAccessBlockConfiguration>' +
          [
            'BlockPublicAcls',
            'IgnorePublicAcls',
            'BlockPublicPolicy',
            'RestrictPublicBuckets',
          ]
            .map(name => `<${name}>${!publicAccess}</${name}>`)
            .join('') +
          '</PublicAccessBlockConfiguration>'
      );
      return;
    }
    const fault = faults.get(key);
    if (fault === 'head-fails' && req.method === 'HEAD') {
      res.writeHead(503);
      res.end();
      return;
    }
    if (req.method === 'GET' && url.searchParams.get('list-type') === '2') {
      listPages++;
      const prefix = url.searchParams.get('prefix'),
        after = url.searchParams.get('start-after') ?? '';
      const all = [...objects.keys()]
        .filter(key => key.startsWith(prefix) && key > after)
        .sort();
      const keys = all.slice(0, Number(url.searchParams.get('max-keys')));
      res.end(
        `<ListBucketResult><IsTruncated>${keys.length < all.length}</IsTruncated>${keys.map(key => `<Contents><Key>${xml(encodeURIComponent(key))}</Key></Contents>`).join('')}</ListBucketResult>`
      );
      return;
    }
    if (req.method === 'PUT') {
      copies.set(key, (copies.get(key) ?? 0) + 1);
      const source = decodeURIComponent(
        req.headers['x-amz-copy-source']
      ).replace(/^\/proof\//, '');
      const original = objects.get(source);
      if (!original) {
        res.writeHead(404);
        res.end();
        return;
      }
      assert.equal(req.headers['x-amz-copy-source-if-match'], original.etag);
      if (fault === 'embedded-error') {
        res.end('<Error><Code>InternalError</Code></Error>');
        return;
      }
      const copy = () => objects.set(key, { ...original });
      if (fault !== 'hold-before-copy') copy();
      if (fault === 'hold-copy' || fault === 'hold-before-copy') {
        faults.delete(key);
        held = () => {
          if (fault === 'hold-before-copy') copy();
          res.end(
            `<CopyObjectResult><ETag>${original.etag}</ETag></CopyObjectResult>`
          );
          held = undefined;
        };
        return;
      }
      if (fault === 'lost-response') {
        faults.delete(key);
        req.socket.destroy();
        return;
      }
      res.end(
        `<CopyObjectResult><ETag>${original.etag}</ETag></CopyObjectResult>`
      );
      return;
    }
    if (req.method === 'DELETE') {
      objects.delete(key);
      if (fault === 'hold-delete') {
        faults.delete(key);
        held = () => {
          res.writeHead(204);
          res.end();
          held = undefined;
        };
        return;
      }
      res.writeHead(204);
      res.end();
      return;
    }
    const stored = objects.get(key);
    if (!stored) {
      res.writeHead(404);
      res.end();
      return;
    }
    res.writeHead(200, {
      'content-length': stored.body.length,
      'x-amz-checksum-sha256': stored.checksum,
      etag: stored.etag,
      'x-amz-server-side-encryption': 'AES256',
    });
    res.end(req.method === 'HEAD' ? undefined : stored.body);
  } catch (error) {
    receiverError = String(error);
    res.writeHead(400);
    res.end();
  }
});
let host, server;
async function startHost(port) {
  host = spawn(
    cli,
    [
      'start',
      '--listen-addr',
      `127.0.0.1:${port}`,
      '--data-dir',
      dataDirectory,
      '--non-interactive',
    ],
    { stdio: 'inherit', cwd: root, detached: process.platform !== 'win32' }
  );
  await once(host, 'spawn');
  server = `http://127.0.0.1:${port}`;
  await until(async () => {
    try {
      return (await fetch(`${server}/v1/ping`)).ok;
    } catch {
      return false;
    }
  }, 'host startup');
}
async function stopHost() {
  if (host?.pid && host.exitCode === null && host.signalCode === null) {
    const stopped = once(host, 'exit');
    if (process.platform === 'win32')
      spawnSync('taskkill', ['/PID', String(host.pid), '/T', '/F']);
    else process.kill(-host.pid, 'SIGKILL');
    await stopped;
  }
}
async function command(args, env = process.env) {
  const child = spawn(cli, args, {
    env,
    cwd: root,
    stdio: ['ignore', 'pipe', 'pipe'],
  });
  let output = '',
    errors = '';
  child.stdout.on('data', chunk => (output += chunk));
  child.stderr.on('data', chunk => (errors += chunk));
  const [code] = await once(child, 'exit');
  assert.equal(code, 0, errors);
  return output;
}
try {
  s3.listen(0, '127.0.0.1');
  await once(s3, 'listening');
  const probe = createServer();
  probe.listen(0, '127.0.0.1');
  await once(probe, 'listening');
  const port = probe.address().port;
  await new Promise(done => probe.close(done));
  await startHost(port);
  const environment = {
    ...process.env,
    AWS_ACCESS_KEY_ID: credentials.accessKeyId,
    AWS_SECRET_ACCESS_KEY: credentials.secretAccessKey,
    AWS_SESSION_TOKEN: credentials.sessionToken,
    AWS_SESSION_EXPIRES_AT: String(Date.now() + 3600000),
    S3_BUCKET: 'proof',
    S3_REGION: 'us-east-1',
    S3_PREFIX: 'app/',
    S3_ACCOUNT_ID: '123456789012',
    TEST_ENDPOINT: `http://127.0.0.1:${s3.address().port}`,
    TEST_FAST: 'true',
  };
  const publish = (patch = {}, envOnly = true) =>
    command(
      [
        'publish',
        '--server',
        server,
        '--yes',
        ...(envOnly ? ['--env-only'] : ['--module-path', 'spacetimedb']),
        database,
      ],
      { ...environment, ...patch }
    );
  await publish({}, false);
  const alice = await (
    await fetch(`${server}/v1/identity`, { method: 'POST' })
  ).json();
  const bob = await (
    await fetch(`${server}/v1/identity`, { method: 'POST' })
  ).json();
  const rpc = async (who, name, args = [], allowFailure = false) => {
    const response = await fetch(
      `${server}/v1/database/${database}/call/${name}`,
      {
        method: 'POST',
        headers: {
          Authorization: `Bearer ${who.token}`,
          'Content-Type': 'application/json',
        },
        body: JSON.stringify(args),
        signal: AbortSignal.timeout(15_000),
      }
    );
    const text = await response.text();
    if (allowFailure) return { status: response.status, text };
    assert.ok(response.ok, `${name}: ${response.status} ${text}`);
    const value = text ? JSON.parse(text) : undefined;
    // The HTTP procedure endpoint returns SATS products as positional arrays.
    if (name === 'begin_upload') {
      assert.ok(Array.isArray(value));
      const [id, option] = value;
      const grant = option[0] === 0 ? option[1] : undefined;
      return {
        id,
        upload: grant && {
          url: grant[0],
          fields: grant[1].map(([name, value]) => ({ name, value })),
        },
      };
    }
    if (name === 'get_download_url') return { url: value[0] };
    return value;
  };
  const view = async who => {
    const response = await fetch(`${server}/v1/database/${database}/sql`, {
      method: 'POST',
      headers: { Authorization: `Bearer ${who.token}` },
      body: 'SELECT * FROM my_files',
    });
    assert.ok(response.ok, await response.clone().text());
    return (await response.json())[0].rows;
  };
  const state = async (who, id) =>
    (await view(who)).find(row => row[0] === id)?.[4];
  const body = 'test-data',
    size = Buffer.byteLength(body),
    checksum = sha(body);
  const begin = (who, request, bytes = size) =>
    rpc(who, 'begin_upload', [
      request,
      'test.txt',
      'text/plain',
      bytes,
      checksum,
    ]);
  const localUrl = url =>
    url.replace(`https://${awsHost}`, `http://127.0.0.1:${s3.address().port}`);
  const upload = async (grant, content = body) => {
    const data = new FormData();
    for (const { name, value } of grant.upload.fields) data.append(name, value);
    data.append('file', new Blob([content]), 'test.txt');
    assert.equal(
      (await fetch(localUrl(grant.upload.url), { method: 'POST', body: data }))
        .status,
      204
    );
  };
  const finish = async (who, grant) => {
    await rpc(who, 'complete_upload', [grant.id]);
    await until(
      async () => (await state(who, grant.id)) === 'ready',
      'file ready'
    );
  };
  const remove = async (who, grant) => {
    await rpc(who, 'delete_file', [grant.id]);
    await until(async () => !(await state(who, grant.id)), 'file removed');
  };
  versioning = true;
  assert.ok(
    (
      await rpc(
        alice,
        'begin_upload',
        ['versioned', 'test.txt', 'text/plain', size, checksum],
        true
      )
    ).status >= 400
  );
  versioning = false;
  publicAccess = true;
  assert.ok(
    (
      await rpc(
        alice,
        'begin_upload',
        ['public', 'test.txt', 'text/plain', size, checksum],
        true
      )
    ).status >= 400
  );
  publicAccess = false;
  assert.deepEqual(await view(alice), []);
  check('Versioned and public buckets are rejected before reserving metadata');
  objects.set('outside-prefix/untouched', object(body));
  const first = await begin(alice, 'same-request');
  // Object keys are scoped to the installation: <prefix>final/<install>-<id>.
  const install = first.upload.fields
    .find(field => field.name === 'key')
    .value.match(/^app\/temporary\/(\d+)-\d+$/)[1];
  const finalKey = id => `app/final/${install}-${id}`;
  const duplicate = await begin(alice, 'same-request');
  assert.equal(first.id, duplicate.id);
  const conflict = await rpc(
    alice,
    'begin_upload',
    ['same-request', 'changed.txt', 'text/plain', size, checksum],
    true
  );
  assert.ok(conflict.status >= 400);
  const other = await begin(bob, 'same-request');
  assert.notEqual(first.id, other.id);
  assert.deepEqual(
    (await view(bob)).map(row => row[0]),
    [other.id]
  );
  for (const name of ['complete_upload', 'get_download_url', 'delete_file'])
    assert.ok((await rpc(bob, name, [first.id], true)).status >= 400);
  await command(['sql', '--server', server, database, 'SELECT * FROM s3.file']);
  const privateRead = await fetch(`${server}/v1/database/${database}/sql`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${bob.token}` },
    body: 'SELECT * FROM s3.file',
  });
  assert.ok(!privateRead.ok);
  for (const args of [
    ['invalid', 'bad\nname', 'text/plain', size, checksum],
    ['invalid', 'test.txt', 'text/plain\r\nx-evil: yes', size, checksum],
    ['invalid', 'test.txt', 'text/plain', size, checksum.slice(0, -2) + 'B='],
  ])
    assert.ok((await rpc(alice, 'begin_upload', args, true)).status >= 400);
  const maintenanceDenied = await rpc(
    alice,
    'maintain',
    [[1, [0, [500000]]]],
    true
  );
  assert.ok(maintenanceDenied.status >= 400);
  assert.match(maintenanceDenied.text, /No such procedure/);
  check(
    'Real independent identities: private metadata, owner views, cross-owner operations denied, idempotent reservation'
  );
  const quota = await Promise.all(
    Array.from({ length: 6 }, (_, i) =>
      rpc(
        alice,
        'begin_upload',
        [`quota-${i}`, 'test.txt', 'text/plain', size, checksum],
        true
      )
    )
  );
  assert.equal(quota.filter(value => value.status < 300).length, 3);
  for (const row of await view(alice))
    if (row[0] !== first.id) await rpc(alice, 'delete_file', [row[0]]);
  await until(async () => (await view(alice)).length === 1, 'quota cleanup');
  await remove(bob, other);
  check(
    'Concurrent reservations cannot exceed the per-owner file quota; deletion releases reservations'
  );
  await upload(first);
  await Promise.all([finish(alice, first), finish(alice, first)]);
  await finish(alice, first);
  assert.equal(copies.get(finalKey(first.id)), 1);
  const download = await rpc(alice, 'get_download_url', [first.id]);
  assert.equal(await (await fetch(localUrl(download.url))).text(), body);
  check('Verified completion and completion retry return the same ready file');
  const empty = await rpc(alice, 'begin_upload', [
    'empty',
    'empty.txt',
    'text/plain',
    0,
    sha(''),
  ]);
  await upload(empty, '');
  await finish(alice, empty);
  const emptyDownload = await rpc(alice, 'get_download_url', [empty.id]);
  assert.equal(await (await fetch(localUrl(emptyDownload.url))).text(), '');
  await remove(alice, empty);
  check(
    'Empty files can be reserved, uploaded, verified, downloaded, and deleted'
  );
  const crash = await begin(alice, 'crash-copy');
  await upload(crash);
  faults.set(finalKey(crash.id), 'hold-copy');
  const interrupted = rpc(alice, 'complete_upload', [crash.id]).catch(
    () => undefined
  );
  await until(() => !!held, 'copy effect before crash');
  await delay(200);
  await stopHost();
  held();
  await interrupted;
  await startHost(port);
  await until(
    async () => (await state(alice, crash.id)) === 'ready',
    'copy recovery after host kill'
  );
  check(
    'Host killed after S3 copy: durable intent survives and scheduled recovery makes the verified file ready'
  );
  const lost = await begin(alice, 'lost-response');
  await upload(lost);
  faults.set(finalKey(lost.id), 'lost-response');
  await rpc(alice, 'complete_upload', [lost.id], true);
  await until(
    async () => (await state(alice, lost.id)) === 'ready',
    'lost copy response recovery'
  );
  check(
    'S3 copy succeeds but response is lost: retry discovers and verifies the existing object'
  );
  await remove(alice, lost);
  await remove(alice, crash);
  const late = await begin(alice, 'late-copy');
  await upload(late);
  faults.set(finalKey(late.id), 'hold-before-copy');
  const racing = rpc(alice, 'complete_upload', [late.id]).catch(
    () => undefined
  );
  await until(() => !!held, 'copy paused before effect');
  await remove(alice, late);
  assert.ok(
    (await rpc(alice, 'get_download_url', [late.id], true)).status >= 400
  );
  held();
  await racing;
  await until(
    () => !objects.has(finalKey(late.id)),
    'late copy orphan removal'
  );
  assert.ok(!(await state(alice, late.id)));
  check(
    'Deletion during copy denies grants immediately; late copy cannot restore metadata and is swept from storage'
  );
  faults.set(finalKey(first.id), 'hold-delete');
  await rpc(alice, 'delete_file', [first.id]);
  await until(() => !!held, 'delete effect before crash');
  await stopHost();
  held();
  await startHost(port);
  await until(async () => !(await state(alice, first.id)), 'delete recovery');
  check(
    'Host killed after external deletion: restart completes cleanup and releases quota exactly once'
  );
  const blocked = await begin(alice, 'blocked-copy');
  await upload(blocked);
  faults.set(finalKey(blocked.id), 'embedded-error');
  await rpc(alice, 'complete_upload', [blocked.id], true);
  await delay(500);
  assert.notEqual(await state(alice, blocked.id), 'ready');
  const healthy = await begin(alice, 'healthy');
  await upload(healthy);
  await finish(alice, healthy);
  faults.delete(finalKey(blocked.id));
  await until(
    async () => (await state(alice, blocked.id)) === 'ready',
    'retry recovery'
  );
  check(
    'Embedded copy errors do not publish files or prevent unrelated work; scheduled retries recover'
  );
  const pending = await begin(bob, 'expires');
  await upload(pending);
  await until(
    async () => !(await state(bob, pending.id)),
    'abandoned upload expires',
    20_000
  );
  assert.ok(!objects.has(`app/temporary/${install}-${pending.id}`));
  check('Abandoned uploads expire and their temporary object is deleted');
  const missing = await begin(bob, 'missing');
  await rpc(bob, 'complete_upload', [missing.id]);
  await until(
    async () => !(await state(bob, missing.id)),
    'missing upload releases quota after deadline',
    65_000
  );
  const large = await begin(bob, 'large', 120);
  assert.ok(
    (
      await rpc(
        bob,
        'begin_upload',
        ['over-bytes', 'test.txt', 'text/plain', size, checksum],
        true
      )
    ).status >= 400
  );
  await remove(bob, large);
  const afterQuota = await begin(bob, 'after-byte-quota');
  await remove(bob, afterQuota);
  check(
    'Failed completion expires; byte quotas include pending files and are released after cleanup'
  );
  for (let i = 0; i < 18; i++)
    objects.set(`app/final/${randomUUID()}`, object(body));
  // Same ID from an earlier installation, and an unknown ID in this one.
  objects.set(`app/final/1-${healthy.id}`, object(body));
  objects.set(finalKey('999999'), object(body));
  const pagesBefore = listPages;
  await until(
    () =>
      [...objects.keys()].filter(key => key.startsWith('app/final/')).length ===
      2,
    'orphan pagination'
  );
  assert.ok(listPages - pagesBefore >= 2);
  assert.equal(await state(alice, healthy.id), 'ready');
  check(
    'Bounded paginated orphan sweep removes unknown final keys and preserves ready files'
  );
  await publish({ S3_PREFIX: 'changed/' });
  const moved = await rpc(alice, 'get_download_url', [healthy.id], true);
  assert.ok(moved.status >= 400);
  assert.match(moved.text, /s3.target_changed_or_not_initialized/);
  await publish();
  assert.equal(await state(alice, healthy.id), 'ready');
  check('Changing the configured target cannot expose or move existing files');
  await publish({ AWS_SESSION_EXPIRES_AT: String(Date.now()) });
  const expired = await rpc(alice, 'get_download_url', [healthy.id], true);
  assert.ok(expired.status >= 400);
  assert.match(expired.text, /s3.credentials_expiring/);
  credentials.sessionToken = 'rotated-session';
  environment.AWS_SESSION_TOKEN = credentials.sessionToken;
  await publish();
  const rotated = await rpc(alice, 'get_download_url', [healthy.id]);
  assert.equal(
    new URL(rotated.url).searchParams.get('X-Amz-Security-Token'),
    credentials.sessionToken
  );
  assert.equal(await (await fetch(localUrl(rotated.url))).text(), body);
  check(
    'Expired credentials deny grants; rotated server credentials work without changing metadata'
  );
  await remove(alice, blocked);
  await remove(alice, healthy);
  await until(() => objects.size === 1, 'all managed storage cleaned');
  assert.ok(objects.has('outside-prefix/untouched'));
  objects.delete('outside-prefix/untouched');
  assert.ifError(receiverError);
  check(
    'All test objects removed; no signature mismatch at the independent receiver'
  );
  result.passed = true;
} catch (error) {
  result.error = String(error);
  result.receiverError = receiverError;
  process.exitCode = 1;
} finally {
  await stopHost();
  if (held) held();
  s3.closeAllConnections();
  if (s3.listening) await new Promise(done => s3.close(done));
  result.cleanup.host =
    !host || host.exitCode !== null || host.signalCode !== null;
  result.cleanup.receiver = true;
  result.cleanup.remainingFakeObjects = objects.size;
  if (
    result.cleanup.host &&
    dataDirectory === resolve(directory, 'server') &&
    directory.startsWith(resolve(tmpdir(), 's3-recovery-'))
  ) {
    await rm(dataDirectory, { recursive: true, force: true });
    result.cleanup.serverData = true;
  }
  await writeFile(
    resolve(directory, 'result.json'),
    JSON.stringify(result, null, 2)
  );
  console.log(JSON.stringify(result));
}
