import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { once } from 'node:events';
import { createRequire } from 'node:module';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { chromium, type Page } from 'playwright';
import { Actor } from '../grader/grade.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { executeAction, ActionInconclusive } from '../src/actions/action-contract.js';
import { withAuthRequestPatch } from '../src/actions/auth-request-patch.js';
import { installSpacetimeWriteCapture } from '../src/stacks/backends/spacetime-browser-session.js';
import { createBackendLease, writeBackendLease } from '../src/runtime/backend-lease.js';

// Failure cases specified before implementation: unsupported or ambiguous
// capture, changed non-name arguments, missing commit receipts, wrong target,
// reconnects and unsupported protocol use UI BEFORE sending. Once sent, refusal
// fails; lost/malformed replies, collisions and timeout are inconclusive. None
// may retry through the form. All other values and the live connection survive.
test('SpacetimeDB catalog replay preserves the caller and never retries a sent write', async () => {
  const codec = await import(new URL('../src/stacks/spacetime-wire-codec.js', import.meta.url).href);
  const { wsServer } = createRequire(import.meta.url)('playwright-core/lib/utilsBundle');
  const encode = (type: { serialize(writer: unknown, value: unknown): void }, value: unknown) => {
    const writer = new codec.BinaryWriter(256); type.serialize(writer, value); return Buffer.from(writer.getBuffer());
  };
  const strings = codec.AlgebraicType.makeSerializer({ tag: 'String' });
  const readString = codec.AlgebraicType.makeDeserializer({ tag: 'String' });
  const callBytes = (name: string, requestId: number, nonce: string) => {
    const writer = new codec.BinaryWriter(128); strings(writer, name); strings(writer, nonce);
    return encode(codec.ClientMessage, { tag: 'CallReducer', value: {
      reducer: 'catalog_add', requestId, flags: 0, args: writer.getBuffer(),
    } });
  };
  let mode = '', calls = 0, connections = 0, page: Page;
  const rows: string[] = [], observations: unknown[] = [];
  const server = createServer(async (req, res) => {
    const url = new URL(req.url!, 'http://localhost');
    if (url.pathname.endsWith('/schema')) {
      res.setHeader('Content-Type', 'application/json');
      res.end(JSON.stringify({ reducers: [{ name: 'catalog_add', params: { elements: [
        { name: { some: 'title' }, algebraic_type: mode === 'unsupported-schema' ? { Array: { U8: [] } } : { String: [] } },
        { name: { some: 'nonce' }, algebraic_type: { String: [] } },
      ] } }] }));
    } else if (url.pathname === '/encode') {
      res.end(callBytes(url.searchParams.get('name')!, Number(url.searchParams.get('id')), url.searchParams.get('nonce')!));
    } else {
      res.setHeader('Content-Type', 'text/html');
      res.end(`<input data-role="name"><button data-role="save">Save</button><script>
        window.formSubmits=0; window.replies=0;
        window.socket=new WebSocket(location.origin.replace('http:','ws:')+'/v1/database/catalog/subscribe', ${JSON.stringify(mode === 'unsupported-protocol' ? 'unknown.protocol' : 'v3.bsatn.spacetimedb')});
        socket.binaryType='arraybuffer'; socket.onmessage=()=>window.replies++;
        document.querySelector('button').onclick=async()=>{window.formSubmits++;
          const name=document.querySelector('input').value;
          const nonce=${JSON.stringify(mode)}==='changing-argument'?String(window.formSubmits):'unchanged';
          socket.send(await (await fetch('/encode?'+new URLSearchParams({name,id:String(window.formSubmits),nonce}))).arrayBuffer());};
      </script>`);
    }
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const sockets = new wsServer({ server });
  sockets.on('connection', (socket: { on(event: string, fn: (data: Buffer) => void): void; send(data: Buffer): void; close(): void }) => {
    connections++;
    const send = (value: unknown) => socket.send(Buffer.concat([Buffer.from([0]), encode(codec.ServerMessage, value)]));
    send({ tag: 'InitialConnection', value: { identity: { __identity__: 1n }, connectionId: { __connection_id__: BigInt(connections) }, token: 'fixture-token' } });
    socket.on('message', data => {
      const message = codec.ClientMessage.deserialize(new codec.BinaryReader(data));
      if (message.tag !== 'CallReducer') return;
      const reader = new codec.BinaryReader(message.value.args), name = readString(reader), nonce = readString(reader);
      assert.equal(nonce, mode === 'changing-argument' ? String(calls + 1) : 'unchanged'); calls++;
      const native = message.value.requestId > 100;
      if (!(native && mode === 'refused')) rows.push(name);
      if (native && mode === 'lost') { socket.close(); return; }
      if (native && mode === 'timeout') return;
      if (native && mode === 'malformed') { socket.send(Buffer.from([0, 255])); return; }
      if (native && mode === 'collision' && name !== 'Collision') {
        void page.evaluate(bytes => (window as unknown as { socket: WebSocket }).socket.send(new Uint8Array(bytes)),
          [...callBytes('Collision', message.value.requestId, 'unchanged')]);
        return;
      }
      if (mode === 'unconfirmed' && name === 'Original') return;
      send({ tag: 'ReducerResult', value: { requestId: message.value.requestId,
        timestamp: { __timestamp_micros_since_unix_epoch__: 1n }, result: native && mode === 'refused'
          ? { tag: 'Err', value: new Uint8Array() } : { tag: 'OkEmpty' } } });
    });
  });
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-native-replay-'));
  const leasePath = join(root, 'lease.json');
  const previous = { path: process.env.STACK_BENCH_LEASE, token: process.env.STACK_BENCH_LEASE_TOKEN };
  const browser = await chromium.launch();
  let result = 'failed';
  try {
    for (mode of ['accepted', 'changing-argument', 'unsupported-schema', 'unsupported-protocol', 'unconfirmed', 'wrong-module', 'reconnect', 'refused', 'lost', 'malformed', 'collision', 'timeout']) {
      calls = 0; connections = 0; rows.length = 0;
      const lease = createBackendLease({ runId: 'native-replay', backend: 'spacetime', track: 'ecommerce', runIndex: 0,
        serverUri: url, module: mode === 'wrong-module' ? 'other-module' : 'catalog', dataDir: join(root, 'data') });
      lease.state = 'active'; writeBackendLease(leasePath, lease);
      process.env.STACK_BENCH_LEASE = leasePath; process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
      const context = await browser.newContext();
      try {
        page = await context.newPage();
        const actor = new Actor('admin', page, context, false, true);
        await actor.ready; await page.goto(url);
        await page.waitForFunction(() => (window as unknown as { socket: WebSocket }).socket.readyState === WebSocket.OPEN);
        for (const name of ['Original', 'Control']) {
          await page.locator('input').fill(name); await page.locator('button').click();
          const deadline = Date.now() + 3000;
          while (calls < (name === 'Original' ? 1 : 2)) {
            assert(Date.now() < deadline, `${mode}: UI control did not reach the server`);
            await new Promise(resolve => setTimeout(resolve, 5));
          }
        }
        await page.waitForTimeout(50);
        if (mode === 'reconnect') {
          await page.evaluate(async () => {
            const state = window as unknown as { socket: WebSocket };
            const url = state.socket.url;
            await new Promise<void>(resolve => { state.socket.addEventListener('close', () => resolve(), { once: true }); state.socket.close(); });
            state.socket = new WebSocket(url, 'v3.bsatn.spacetimedb');
            await new Promise<void>(resolve => state.socket.addEventListener('open', () => resolve(), { once: true }));
          });
        }
        const interaction = { defaultWithin: 1000, expand: (s: string) => s, sleep: async () => {} };
        const outcome = await executeAction(ACTION_REGISTRY, 'repeatFormWrite', {
          do: 'repeatFormWrite', actor: 'admin', match: 'Original', control: 'Control', replacement: 'New "product"',
          fields: [{ testid: 'name', text: 'New "product"' }], submit: 'save',
        }, { capabilities: { actors: { get: () => actor }, 'browser-interaction': interaction,
          'transport-observation': interaction, 'named-actions': { fetch } } });
        await page.waitForTimeout(50);
        const native = ['accepted', 'refused', 'lost', 'malformed', 'collision', 'timeout'].includes(mode);
        assert.equal(await page.evaluate(() => (window as unknown as { formSubmits: number }).formSubmits), native ? 2 : 3, mode);
        assert.equal(calls, mode === 'collision' ? 4 : 3, `${mode}: no retry`);
        assert.equal(connections, mode === 'reconnect' ? 2 : 1, `${mode}: no harness-created connection`);
        assert.equal(outcome.status, mode === 'refused' ? 'failed' : ['lost', 'malformed', 'collision', 'timeout'].includes(mode) ? 'inconclusive' : 'passed', JSON.stringify(outcome));
        assert.equal(rows.filter(name => name === 'New "product"').length, mode === 'refused' ? 0 : 1, mode);
        observations.push({ mode, calls, connections, outcome });
      } finally { await context.close(); }
    }
    result = 'passed';
  } finally {
    await browser.close(); sockets.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    if (previous.path === undefined) delete process.env.STACK_BENCH_LEASE; else process.env.STACK_BENCH_LEASE = previous.path;
    if (previous.token === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN; else process.env.STACK_BENCH_LEASE_TOKEN = previous.token;
    rmSync(root, { recursive: true, force: true });
    if (process.env.STACK_BENCH_REPLAY_EVIDENCE) {
      mkdirSync(process.env.STACK_BENCH_REPLAY_EVIDENCE, { recursive: true });
      writeFileSync(join(process.env.STACK_BENCH_REPLAY_EVIDENCE, 'spacetime-receipt.json'), JSON.stringify({ result, observations,
        rerun: 'node --test dist/tests/spacetime-captured-replay.integration.js' }, null, 2));
    }
  }
});

// Failure cases: a receipt cannot hide a later ambiguous call or decode failure.
// Renaming a reducer does not change its credential semantics; supported scalar
// fields, 64-bit integers, optional values, and enums must preserve their values.
// An exact localhost alias is valid; wrong ports, type changes, and nonmatching
// credentials must not earn a receipt. Unknown schemas and wrong lease targets
// remain unmeasured. No probe may replay a second request on another connection.
// A completed HTTP or native login can reload and close its socket. That must
// preserve its receipt; closing before a native reply must remain inconclusive.
test('SpacetimeDB auth receipts remain valid only for one complete credential call', async t => {
  const codec = await import(new URL('../src/stacks/spacetime-wire-codec.js', import.meta.url).href);
  const { wsServer } = createRequire(import.meta.url)('playwright-core/lib/utilsBundle');
  const encode = (type: { serialize(writer: unknown, value: unknown): void }, value: unknown) => {
    const writer = new codec.BinaryWriter(256); type.serialize(writer, value); return Buffer.from(writer.getBuffer());
  };
  let mode = '', connections = 0;
  const received: unknown[][] = [], observations: unknown[] = [];
  const scalarCase = () => mode === 'scalar-fields';
  const integerCase = () => ['u64-fields', 'type-changing-fields'].includes(mode);
  const optionCase = () => mode.startsWith('optional-');
  const types = (): { tag: string; value?: unknown }[] => {
    const base = [{ tag: 'String' }, { tag: 'String' }];
    if (scalarCase()) return [...base, { tag: 'Bool' }, { tag: 'U32' }];
    if (integerCase()) return [...base, { tag: 'U64' }];
    if (optionCase()) return [...base, { tag: 'Sum', value: { variants: [
      { name: 'some', algebraicType: { tag: 'Bool' } },
      { name: 'none', algebraicType: { tag: 'Product', value: { elements: [] } } },
    ] } }];
    if (mode === 'enum-fields') return [...base, { tag: 'Sum', value: { variants: [
      { name: 'Customer', algebraicType: { tag: 'Product', value: { elements: [] } } },
      { name: 'Staff', algebraicType: { tag: 'Product', value: { elements: [] } } },
    ] } }];
    return base;
  };
  const values = (): unknown[] => {
    const base = [mode === 'nonmatching-credentials' ? 'different-customer' : 'customer', 'secret'];
    return scalarCase() ? [...base, false, 17] : integerCase() ? [...base, 9007199254740993n]
      : optionCase() ? [...base, mode === 'optional-some' ? false : undefined]
        : mode === 'enum-fields' ? [...base, { tag: 'Customer', value: {} }] : base;
  };
  const rawTypes = (): unknown[] => {
    const base = [{ String: [] }, { String: [] }];
    if (scalarCase()) return [...base, { Bool: [] }, { U32: [] }];
    if (integerCase()) return [...base, { U64: [] }];
    if (optionCase()) return [...base, { Sum: { variants: [
      { name: { some: 'some' }, algebraic_type: { Bool: [] } },
      { name: { some: 'none' }, algebraic_type: { Product: { elements: [] } } },
    ] } }];
    if (mode === 'enum-fields') return [...base, { Sum: { variants: [
      { name: { some: 'Customer' }, algebraic_type: { Product: { elements: [] } } },
      { name: { some: 'Staff' }, algebraic_type: { Product: { elements: [] } } },
    ] } }];
    return base;
  };
  const evidenceJson = (value: unknown) => JSON.stringify(value,
    (_key, entry) => typeof entry === 'bigint' ? { bigint: String(entry) } : entry, 2);
  const callBytes = (id: number) => {
    const writer = new codec.BinaryWriter(128), args = values();
    types().forEach((type, index) => codec.AlgebraicType.makeSerializer(type)(writer, args[index]));
    return encode(codec.ClientMessage, { tag: 'CallReducer', value: {
      reducer: mode === 'renamed' || scalarCase() ? 'register_customer' : 'sign_up', requestId: id, flags: 0, args: writer.getBuffer(),
    } });
  };
  const server = createServer((req, res) => {
    if (req.url === '/authenticate') {
      let body = '';
      req.on('data', data => { body += String(data); });
      req.on('end', () => {
        received.push(JSON.parse(body));
        res.setHeader('Content-Type', 'application/json'); res.end('true');
      });
    } else if (req.url?.includes('/schema')) {
      res.setHeader('Content-Type', 'application/json');
      res.end(JSON.stringify({ reducers: [{ name: mode === 'renamed' || scalarCase() ? 'register_customer' : 'sign_up',
        params: { elements: rawTypes().map((raw, index) => ({ name: { some: ['username', 'password', integerCase() ? 'nonce' : 'is_admin', 'nonce'][index] },
          algebraic_type: mode === 'unsupported-schema' && index === 1 ? { Array: { U8: [] } } : raw })) } }] }));
    } else {
      res.setHeader('Content-Type', 'text/html'); res.end('<body>Native auth form</body>');
    }
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const port = (server.address() as { port: number }).port, url = `http://127.0.0.1:${port}`;
  const sockets = new wsServer({ server });
  sockets.on('connection', (socket: { on(event: string, fn: (data: Buffer) => void): void; send(data: Buffer): void; close(): void }) => {
    connections++;
    const send = (value: unknown) => socket.send(Buffer.concat([Buffer.from([0]), encode(codec.ServerMessage, value)]));
    send({ tag: 'InitialConnection', value: { identity: { __identity__: 1n }, connectionId: { __connection_id__: BigInt(connections) }, token: 'fixture-token' } });
    socket.on('message', data => {
      // Malformed outbound traffic must be stopped by the active native probe.
      if (data[0] === 255) { received.push(['malformed']); return; }
      const message = codec.ClientMessage.deserialize(new codec.BinaryReader(data));
      if (message.tag !== 'CallReducer') return;
      const reader = new codec.BinaryReader(message.value.args);
      received.push(types().map(type => codec.AlgebraicType.makeDeserializer(type)(reader)));
      if (mode === 'lost-receipt') { socket.close(); return; }
      send({ tag: 'ReducerResult', value: { requestId: message.value.requestId,
        timestamp: { __timestamp_micros_since_unix_epoch__: 1n }, result: mode === 'refused'
          ? { tag: 'Err', value: new Uint8Array() } : { tag: 'OkEmpty' } } });
      if (mode === 'malformed-inbound') socket.send(Buffer.from([0, 255]));
    });
  });
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-native-auth-')), leasePath = join(root, 'lease.json');
  const previous = { path: process.env.STACK_BENCH_LEASE, token: process.env.STACK_BENCH_LEASE_TOKEN };
  const browser = await chromium.launch();
  try {
    for (mode of ['accepted', 'localhost', 'refused', 'renamed', 'scalar-fields', 'u64-fields', 'optional-some', 'optional-none', 'enum-fields', 'type-changing-fields', 'nonmatching-credentials', 'missing-claim', 'reload-after-receipt', 'http-reload', 'lost-receipt', 'reconnected-second', 'second-after-receipt',
      'second-then-submit-error', 'malformed-outbound', 'malformed-inbound', 'wrong-module', 'foreign-target', 'unsupported-protocol', 'unsupported-schema', 'app-proxy']) {
      await t.test(mode, async () => {
        received.length = 0; connections = 0;
        const lease = createBackendLease({ runId: 'native-auth', backend: 'spacetime', track: 'ecommerce', runIndex: 0,
          serverUri: mode === 'foreign-target' ? `http://127.0.0.1:${port + 1}` : url,
          module: mode === 'wrong-module' ? 'different' : 'auth', dataDir: join(root, 'data') });
        lease.state = 'active'; writeBackendLease(leasePath, lease);
        process.env.STACK_BENCH_LEASE = leasePath; process.env.STACK_BENCH_LEASE_TOKEN = lease.ownershipToken;
        const context = await browser.newContext();
        try {
          const page = await context.newPage(); await installSpacetimeWriteCapture(page); await page.goto(url);
          const socketUrl = (['foreign-target', 'localhost'].includes(mode) ? url.replace('127.0.0.1', 'localhost') : url).replace('http:', 'ws:')
            + (mode === 'app-proxy' ? '/proxy' : '') + '/v1/database/auth/subscribe';
          await page.evaluate(async ({ socketUrl, protocol }) => {
            const socket = new WebSocket(socketUrl, protocol); socket.binaryType = 'arraybuffer';
            Object.assign(window, { socket, replies: 0 });
            socket.addEventListener('message', () => (window as unknown as { replies: number }).replies++);
            await new Promise(resolve => socket.addEventListener('message', resolve, { once: true }));
          }, { socketUrl, protocol: mode === 'unsupported-protocol' ? 'unsupported' : 'v3.bsatn.spacetimedb' });
          const patch = mode === 'type-changing-fields' ? { fields: { nonce: 'wrong-type' } } : scalarCase() ? { fields: { isAdmin: true } } : mode === 'missing-claim' ? { fields: { role: 'admin' } } : { password: 'modified' };
          let outcome: unknown;
          try {
            outcome = await withAuthRequestPatch(page, 'customer', 'secret', patch, async () => {
              await page.evaluate(async ({ first, second, mode }) => {
                const state = window as unknown as { socket: WebSocket; replies: number };
                if (mode === 'http-reload') {
                  await fetch('/authenticate', { method: 'POST', headers: { 'content-type': 'application/json' },
                    body: JSON.stringify(['customer', 'secret']) }).then(response => response.json());
                } else {
                  const reply = new Promise(resolve => state.socket.addEventListener(mode === 'lost-receipt' ? 'close' : 'message', resolve, { once: true }));
                  state.socket.send(new Uint8Array(first));
                  if (mode === 'type-changing-fields') await Promise.race([reply, new Promise(resolve => setTimeout(resolve, 500))]);
                  else await reply;
                }
                if (mode.startsWith('second-')) state.socket.send(new Uint8Array(second));
                if (mode === 'malformed-outbound') state.socket.send(new Uint8Array([255]));
              }, { first: [...callBytes(1)], second: [...callBytes(2)], mode });
              if (['reload-after-receipt', 'http-reload', 'reconnected-second'].includes(mode)) await page.reload();
              if (mode === 'reconnected-second') await page.evaluate(async ({ socketUrl, second }) => {
                const socket = new WebSocket(socketUrl, 'v3.bsatn.spacetimedb');
                Object.assign(window, { socket });
                await new Promise(resolve => socket.addEventListener('message', resolve, { once: true }));
                socket.send(new Uint8Array(second));
              }, { socketUrl, second: [...callBytes(2)] });
              // Let native socket events settle while the submission remains active.
              await page.waitForTimeout(100);
              if (mode === 'second-then-submit-error') throw new Error('Submission did not finish');
              return { submitted: true };
            });
          } catch (error) { outcome = error; }
          const measured = ['accepted', 'localhost', 'refused', 'renamed', 'scalar-fields', 'u64-fields', 'optional-some', 'optional-none', 'enum-fields', 'missing-claim', 'reload-after-receipt', 'http-reload', 'app-proxy'].includes(mode);
          observations.push({ mode, measured, connections, received: structuredClone(received),
            outcome: outcome instanceof Error ? { name: outcome.name, message: outcome.message } : outcome });
          if (!measured) assert(outcome instanceof ActionInconclusive, `${mode}: ${evidenceJson(observations.at(-1))}`);
          else {
            assert(!(outcome instanceof Error), `${mode}: ${String(outcome)}`);
            const receipt = (outcome as { requestPatch: { transport?: string; success?: boolean; status?: number; absentParameters?: string[] } }).requestPatch;
            if (mode === 'http-reload') assert.equal(receipt.status, 200);
            else { assert.equal(receipt.transport, 'spacetime-websocket'); assert.equal(receipt.success, mode !== 'refused'); }
            const expected = values();
            if (scalarCase()) expected[2] = true;
            else if (mode !== 'missing-claim') expected[1] = 'modified';
            assert.deepEqual(received, [expected], 'only the intended credential or claim changes');
            if (mode === 'missing-claim') assert.deepEqual(receipt.absentParameters, ['role']);
          }
          assert.equal(connections, mode === 'reconnected-second' ? 2 : 1, 'only the app can create connections');
          if (mode.startsWith('second-') || mode === 'reconnected-second' || mode === 'malformed-outbound') assert.equal(received.length, 1, 'ambiguous traffic must not be forwarded');
        } finally { await context.close(); }
      });
    }
  } finally {
    await browser.close(); sockets.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    if (previous.path === undefined) delete process.env.STACK_BENCH_LEASE; else process.env.STACK_BENCH_LEASE = previous.path;
    if (previous.token === undefined) delete process.env.STACK_BENCH_LEASE_TOKEN; else process.env.STACK_BENCH_LEASE_TOKEN = previous.token;
    rmSync(root, { recursive: true, force: true });
    if (process.env.STACK_BENCH_REPLAY_EVIDENCE) {
      mkdirSync(process.env.STACK_BENCH_REPLAY_EVIDENCE, { recursive: true });
      writeFileSync(join(process.env.STACK_BENCH_REPLAY_EVIDENCE, 'spacetime-auth-receipt.json'), evidenceJson({ observations,
        rerun: 'node --test --test-name-pattern="SpacetimeDB auth receipts" dist/tests/spacetime-captured-replay.integration.js' }));
    }
  }
});
