import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { once } from 'node:events';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { createGzip, constants } from 'node:zlib';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { chromium, type CDPSession } from 'playwright';
import { Actor, gradeFeature } from '../grader/grade.js';
import { executeAction, type ActionEvidence } from '../src/actions/action-contract.js';
import { ACTION_REGISTRY } from '../src/actions/action-catalog.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';
import { startNetworkInterruption } from '../src/actions/network-interruption.js';

// Failure cases specified before the fix: decoded cached fonts have no readable
// body; fresh/reopened actors can lose capture settings; cache bypass can leak
// into ordinary checks; and a privacy fix must still detect delivered secrets.
test('privacy actors retain font bodies across reloads without changing ordinary HTTP caching', async t => {
  const assets = join(dirname(createRequire(import.meta.url).resolve('playwright-core/package.json')), 'lib/vite/traceViewer');
  const fontName = readdirSync(assets).find(name => name.endsWith('.ttf'));
  assert(fontName, 'Use the real font already shipped with the pinned browser tooling');
  const font = readFileSync(join(assets, fontName));
  let leak = false;
  let fontRequests = 0;
  const server = createServer((req, res) => {
    if (req.url === '/font') {
      fontRequests++;
      // Reproduce the observed font response MIME, with valid decodable bytes.
      res.writeHead(200, { 'Content-Type': 'text/html', 'Cache-Control': 'public, max-age=3600' });
      res.end(Buffer.concat([font, Buffer.from(leak ? 'private-font-canary' : '')]));
    } else {
      res.writeHead(200, { 'Content-Type': 'text/html', 'Cache-Control': 'no-store' });
      res.end(`<style>@font-face {font-family:probe;src:url('/font')} body {font-family:probe}</style>
        <span id="loaded">waiting</span><script>
        document.fonts.load('16px probe').then(fonts => {
          document.querySelector('#loaded').textContent = fonts.length ? 'decoded' : 'failed';
        });</script>`);
    }
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  t.after(() => { server.closeAllConnections(); server.close(); });
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  t.after(() => browser.close());
  const evidence: unknown[] = [];
  t.after(() => {
    if (process.env.STACK_BENCH_CAPTURE_EVIDENCE) writeFileSync(process.env.STACK_BENCH_CAPTURE_EVIDENCE,
      JSON.stringify({ rerun: 'STACK_BENCH_CAPTURE_EVIDENCE=<file> node --test dist/tests/transport-cache.integration.js',
        browser: browser.version(), node: process.version, fontSha256: createHash('sha256').update(font).digest('hex'),
        executables: ['./transport-cache.integration.js', '../grader/grade.js', '../grader/transport-frames.js']
          .map(path => ({ path, sha256: createHash('sha256').update(readFileSync(new URL(path, import.meta.url))).digest('hex') })),
        evidence }, null, 2));
  });
  for (const phase of ['ordinary', 'initial', 'replacement', 'fresh', 'race']) {
    for (leak of phase === 'ordinary' ? [false] : [false, true]) {
      await t.test(`${phase}, leak=${leak}`, async () => {
        fontRequests = 0;
        const actor = phase === 'fresh' ? 'reader-fresh' : 'reader';
        const loaded = { do: 'expect', actor, testid: 'loaded', contains: 'decoded', within: 3000 };
        const prepare = phase === 'fresh' ? [{ do: 'freshClient', actor: 'reader' }]
          : phase === 'replacement' ? [{ do: 'closeClient', actor }, { do: 'openClient', actor, settleMs: 0 }] : [];
        const observation = { do: 'expectNotReceived', actor, contains: 'private-font-canary', within: 100 };
        const feature = compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 1, features: [{
          id: 1, name: 'cached response capture', actors: ['reader'],
          setup: [{ ...loaded, actor: 'reader' }],
          criteria: [{ id: 'capture', desc: 'decoded responses remain observable', points: 1,
            steps: [...prepare, loaded,
              { do: 'reload', actor, settleMs: 0 }, loaded,
              { do: 'reload', actor, settleMs: 0 }, loaded,
              ...(phase === 'ordinary' ? [] : phase === 'race'
                ? [{ do: 'race', settleMs: 0, branches: [[observation], [loaded]] }] : [observation])],
          }],
        }] }).features[0]!;
        const result = await gradeFeature(browser, feature,
          { url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false },
          { runId: 'cache-capture', roomName: name => name, url, actions: [], spacetime: null,
            backend: 'postgres', nullControl: false, defaultWithin: 3000 });
        evidence.push({ phase, leak, fontRequests, result });
        assert.equal(result.setupEvidence.status, 'passed');
        assert.equal(result.criteria[0]!.evidence.status, leak ? 'failed' : 'passed', JSON.stringify(result));
        if (phase === 'ordinary') assert.equal(fontRequests, 1, 'Ordinary checks must still use native caching');
        else assert(fontRequests >= 3, 'Privacy observations must capture fresh response bytes');
        assert.equal(result.cleanupEvidence, undefined, 'No cleanup failure');
        assert.equal(browser.contexts().length, 0, 'All actor contexts must be closed');
      });
    }
  }
});

// A privacy verdict can coincide with an ordinary response still being read.
// Allow bounded completion, detect a late body leak, and never pass a hung body.
test('privacy verdict drains an in-flight body and keeps a stalled response inconclusive', async t => {
  let mode = 'clean';
  const server = createServer((req, res) => {
    if (req.url === '/private-route?token=private-query') {
      const body = mode.endsWith('leak') ? 'private-body-canary'
        : mode === 'captured' ? 'x'.repeat(8 * 1024 * 1024 - 32) : 'public';
      const respond = () => {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.write('{"value":"');
        if (mode !== 'stalled') setTimeout(() => res.end(`${body}"}`), 400);
      };
      if (mode.startsWith('late-')) setTimeout(respond, 150);
      else if (mode !== 'headers-stalled') respond();
    } else {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end(`${mode === 'captured' ? 'private-body-canary' : ''}<span id="ready">${mode.startsWith('late-') || mode === 'headers-stalled' ? 'request started' : 'waiting'}</span><script>for(let n=0;n<${mode === 'headers-stalled' ? 10 : 1};n++)fetch('/private-route?token=private-query', {headers:{'X-Private':'private-header'}}).then(() => {
        document.querySelector('#ready').textContent='headers received'; });</script>`);
    }
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  t.after(() => { server.closeAllConnections(); server.close(); });
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  t.after(() => browser.close());
  const evidence: unknown[] = [];
  t.after(() => {
    if (process.env.STACK_BENCH_CAPTURE_EVIDENCE) writeFileSync(`${process.env.STACK_BENCH_CAPTURE_EVIDENCE}.pending.json`,
      JSON.stringify({ browser: browser.version(), evidence }, null, 2));
  });
  // Headers can arrive after the observation window; an unanswered started
  // request must not establish absence, and a late leak must still fail.
  for (mode of ['clean', 'leak', 'captured', 'stalled', 'late-clean', 'late-leak', 'headers-stalled']) {
    await t.test(mode, async () => {
      // A stalled body must identify its source without logging private paths or queries.
      const stderr = t.mock.method(process.stderr, 'write');
      const feature = compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 1, features: [{
        id: 1, name: 'response completion', actors: ['reader'], setup: [],
        criteria: [{ id: 'capture', desc: 'no private response body', points: 1, steps: [
          { do: 'expect', actor: 'reader', testid: 'ready', contains: mode.startsWith('late-') || mode === 'headers-stalled' ? 'request started' : 'headers received' },
          { do: 'expectNotReceived', actor: 'reader', contains: 'private-body-canary', within: 50 },
        ] }],
      }] }).features[0]!;
      const result = await gradeFeature(browser, feature,
        { url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false },
        { runId: 'body-completion', roomName: name => name, url, actions: [], spacetime: null,
          backend: 'postgres', nullControl: false, defaultWithin: 1000 });
      const diagnostics = stderr.mock.calls.map(call => String(call.arguments[0]));
      evidence.push({ mode, result, diagnostics });
      assert.equal(result.criteria[0]!.evidence.status,
        mode.endsWith('clean') ? 'passed' : mode.endsWith('stalled') ? 'inconclusive' : 'failed', JSON.stringify(result));
      assert.equal(browser.contexts().length, 0);
      if (mode === 'headers-stalled') {
        // Requests with no response headers must leave bounded, safe diagnostics.
        const lines = diagnostics.filter(line => line.startsWith('transport request pending '));
        assert.equal(lines.length, 8, 'Only the first eight outstanding requests are logged');
        for (const line of lines) {
          const pending = JSON.parse(line.slice('transport request pending '.length)) as Record<string, unknown>;
          assert.equal(pending.phase, 'absence');
          assert.equal(pending.method, 'GET');
          assert.equal(pending.resourceType, 'fetch');
          assert.equal(pending.origin, url);
          assert.equal(pending.pathSha256, createHash('sha256').update('/private-route').digest('hex'));
          assert(typeof pending.elapsedMs === 'number' && pending.elapsedMs >= 0);
          assert.deepEqual(Object.keys(pending).sort(), ['elapsedMs', 'method', 'origin', 'pathSha256', 'phase', 'resourceType']);
          assert(!/private-route|private-query|private-header|private-body-canary/.test(line));
        }
      }
      if (mode === 'stalled') {
        const pending = stderr.mock.calls.map(call => String(call.arguments[0]))
          .find(line => line.startsWith('transport request pending '));
        assert(pending, 'The inconclusive result must identify the pending response');
        assert(pending.includes('"phase":"absence"'));
        assert(pending.includes('"resourceType":"fetch"'));
        assert(!/private-route|private-query|private-body-canary/.test(pending));
      }
    });
  }
});

// A harness reload must not cancel a preceding read or write. A GET can start
// before navigation without its headers arriving until after the reload begins.
// Both methods must capture leaks and keep stalled responses inconclusive.
test('privacy reload preserves preceding read and write responses', async t => {
  let leak = false, delayedHeaders = false, stalled = false, capturedLeak = false, eventSource = false, eventLeak = false, method = 'PUT';
  let pageLoads = 0;
  const server = createServer((req, res) => {
    res.setHeader('Cache-Control', 'no-store');
    if (req.url === '/events') {
      res.writeHead(200, { 'Content-Type': 'text/event-stream' });
      res.write(`data: ${eventLeak ? 'private-save-canary' : 'public'}\n\n`);
    } else if (req.url === '/api/profile?token=private-query') {
      const start = () => {
        res.writeHead(200, { 'Content-Type': 'application/json' });
        res.write('{"value":"');
        if (!stalled) setTimeout(() => res.end(`${leak ? 'private-save-canary' : 'public'}"}`), 300);
      };
      if (delayedHeaders) setTimeout(start, 150); else start();
    } else {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end(`${capturedLeak && pageLoads++ === 0 ? 'private-save-canary' : ''}${eventSource ? "<script>new EventSource('/events')</script>" : ''}<button id="profile-save" onclick="fetch('/api/profile?token=private-query', {method:'${method}',headers:{'X-Private':'private-header'}${method === 'PUT' ? ",body:'private-request-body'" : ''}}).then(r=>r.json())">Save</button>`);
    }
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  t.after(() => { server.closeAllConnections(); server.close(); });
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true });
  t.after(() => browser.close());
  const evidence: unknown[] = [];
  t.after(() => {
    if (process.env.STACK_BENCH_CAPTURE_EVIDENCE) writeFileSync(`${process.env.STACK_BENCH_CAPTURE_EVIDENCE}.reload.json`,
      JSON.stringify({ browser: browser.version(), evidence }, null, 2));
  });
  // A captured disclosure is a failure even if another request never finishes.
  for (const requestMethod of ['PUT', 'GET']) for (const mode of ['clean', 'leak', 'late-clean', 'late-leak', 'stalled', 'captured-stalled', 'eventsource-clean', 'eventsource-leak']) {
    await t.test(`${requestMethod} ${mode}`, async () => {
      const stderr = t.mock.method(process.stderr, 'write');
      method = requestMethod;
      pageLoads = 0;
      eventSource = mode.startsWith('eventsource'); eventLeak = mode === 'eventsource-leak';
      leak = !eventSource && mode.endsWith('leak'); delayedHeaders = mode.startsWith('late'); stalled = mode.endsWith('stalled'); capturedLeak = mode === 'captured-stalled';
      const feature = compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 1, features: [{
        id: 1, name: 'save then reload', actors: ['reader'], setup: [],
        criteria: [{ id: 'capture', desc: 'the save response is captured before controlled reload', points: 1, steps: [
          { do: 'click', actor: 'reader', testid: 'profile-save' },
          { do: 'reload', actor: 'reader', settleMs: 0 },
          { do: 'expectNotReceived', actor: 'reader', contains: 'private-save-canary', within: 50 },
        ] }],
      }] }).features[0]!;
      const result = await gradeFeature(browser, feature,
        { url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false },
        { runId: 'save-reload', roomName: name => name, url, actions: [], spacetime: null,
          backend: 'postgres', nullControl: false, defaultWithin: 1000 });
      const diagnostics = stderr.mock.calls.map(call => String(call.arguments[0]));
      evidence.push({ method, mode, result, diagnostics });
      assert.equal(result.criteria[0]!.evidence.status, capturedLeak || leak || eventLeak ? 'failed' : stalled ? 'inconclusive' : 'passed', JSON.stringify(result));
      if (mode === 'stalled') {
        const lines = diagnostics.filter(line => line.startsWith('transport request pending '));
        assert.equal(lines.length, 1, 'The navigation deadline must identify the outstanding request');
        const pending = JSON.parse(lines[0]!.slice('transport request pending '.length)) as Record<string, unknown>;
        assert.equal(pending.phase, 'navigation');
        assert.equal(pending.method, method);
        assert.equal(pending.resourceType, 'fetch');
        assert.equal(pending.origin, url);
        assert.equal(pending.pathSha256, createHash('sha256').update('/api/profile').digest('hex'));
        assert(typeof pending.elapsedMs === 'number' && pending.elapsedMs >= 1000);
        assert.deepEqual(Object.keys(pending).sort(), ['elapsedMs', 'method', 'origin', 'pathSha256', 'phase', 'resourceType']);
        assert(!/api\/profile|private-query|private-header|private-request-body|private-save-canary/.test(lines[0]!));
        // The interrupted read must retain its cause in both durable evidence layers.
        const persisted = JSON.parse(JSON.stringify(result)) as typeof result;
        const check = persisted.criteria[0]!.evidence;
        assert(check.finding?.kind === 'transport-incomplete', 'The criterion must retain the structured capture finding');
        assert(check.finding.fields.capture);
        assert.equal(check.finding.fields.capture.navigationInterrupted, 1);
        const action = check.actions.at(-1)!.evidence as ActionEvidence;
        assert.deepEqual(action.finding, check.finding, 'Action and criterion must preserve the same cause');
        assert.match(check.summary!, /navigation interruptions: 1/);
        assert.match(action.summary!, /navigation interruptions: 1/);
      }
      assert.equal(browser.contexts().length, 0);
    });
  }
});

// Failure matrix, before observer changes: normal reload must not erase bytes
// already delivered; split UTF-8 and gzip retain exact markers; a finite-window
// observation keeps a silent live request inconclusive. Detached capture and byte overflow
// remain inconclusive, while an already captured leak takes precedence.
test('privacy observes HTTP delivery through reload during a finite window', async t => {
  let mode = '';
  let loads = 0;
  const marker = 'private-🔒-marker';
  const wire: { event: string; at: number; bytes?: number }[] = [];
  const server = createServer((req, res) => {
    res.setHeader('Cache-Control', 'no-store');
    if (req.url === '/poll') {
      wire.push({ event: 'request', at: Date.now() });
      res.on('close', () => wire.push({ event: 'close', at: Date.now() }));
      if (mode === 'silent') return;
      const start = () => {
        if (res.destroyed) return;
        const gzip = mode === 'gzip-leak';
        res.writeHead(200, { 'Content-Type': 'application/json', ...(gzip ? { 'Content-Encoding': 'gzip' } : {}) });
        const output = gzip ? createGzip() : res;
        if (output !== res) output.pipe(res);
        const text = (mode.includes('leak') && mode !== 'future-leak') || mode === 'completed' ? marker : 'public';
        const body = Buffer.from(`{"value":"${text}"}`);
        if (mode === 'overflow') {
          res.end('x'.repeat(8 * 1024 * 1024 + 1));
          wire.push({ event: 'overflow-sent', at: Date.now() });
        } else if (mode === 'completed' || mode.endsWith('capture-lost')) {
          res.end(body);
          wire.push({ event: 'complete', at: Date.now(), bytes: body.length });
        } else {
          // Split inside the four-byte lock character, not just between words.
          const split = text === marker ? body.indexOf(Buffer.from('🔒')) + 2 : 8;
          output.write(body.subarray(0, split));
          if (gzip) (output as ReturnType<typeof createGzip>).flush(constants.Z_SYNC_FLUSH);
          wire.push({ event: 'first-chunk', at: Date.now(), bytes: split });
          setTimeout(() => {
            if (res.destroyed) return;
            output.write(body.subarray(split));
            if (gzip) (output as ReturnType<typeof createGzip>).flush(constants.Z_SYNC_FLUSH);
            wire.push({ event: 'second-chunk', at: Date.now(), bytes: body.length - split });
          }, 25);
          setTimeout(() => { if (!res.destroyed) { output.end(); wire.push({ event: 'complete', at: Date.now() }); } }, 350);
        }
      };
      if (mode === 'zero-headers') setTimeout(start, 200);
      else start();
    } else if (req.url === '/later') {
      res.setHeader('Content-Type', 'application/json');
      setTimeout(() => { if (!res.destroyed) { res.end(JSON.stringify({ value: marker })); wire.push({ event: 'later-leak', at: Date.now() }); } }, 100);
    } else {
      loads++;
      res.setHeader('Content-Type', 'text/html');
      res.end(`<button id="poll" onclick="fetch('/poll').then(r=>r.text()).then(()=>document.getElementById('loaded').textContent='consumed').catch(()=>{})">Poll</button><span id="loaded">ready</span>${loads > 1 && mode === 'future-leak' ? '<script>setTimeout(()=>fetch("/later").then(r=>r.text()).catch(()=>{}),50)</script>' : ''}`);
    }
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  t.after(() => { server.closeAllConnections(); server.close(); });
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  // Match the appliance's full Chromium executable, not headless_shell.
  const browser = await chromium.launch({ headless: true, executablePath: chromium.executablePath() });
  t.after(() => browser.close());
  const evidence: unknown[] = [];
  t.after(() => {
    if (process.env.STACK_BENCH_CAPTURE_EVIDENCE) writeFileSync(`${process.env.STACK_BENCH_CAPTURE_EVIDENCE}.finite-http.json`,
      JSON.stringify({ browser: browser.version(), executable: chromium.executablePath(), evidence }, null, 2));
  });
  for (mode of ['clean', 'zero-headers', 'split-leak', 'gzip-leak', 'future-leak', 'completed', 'silent', 'capture-lost', 'overflow', 'leak-capture-lost']) {
    await t.test(mode, async caseT => {
      wire.length = 0; loads = 0;
      const sessions: CDPSession[] = [];
      const newContext = browser.newContext.bind(browser);
      caseT.mock.method(browser, 'newContext', async (...args: Parameters<typeof browser.newContext>) => {
        const context = await newContext(...args);
        const newSession = context.newCDPSession.bind(context);
        caseT.mock.method(context, 'newCDPSession', async (...sessionArgs: Parameters<typeof context.newCDPSession>) => {
          const session = await newSession(...sessionArgs);
          sessions.push(session);
          return session;
        });
        const newPage = context.newPage.bind(context);
        caseT.mock.method(context, 'newPage', async () => {
          const page = await newPage();
          const reload = page.reload.bind(page);
          caseT.mock.method(page, 'reload', async (...reloadArgs: Parameters<typeof page.reload>) => {
            // A real fetch starts at the boundary of the normal reload action.
            // No route is intercepted and no application traffic is suppressed.
            const request = page.waitForRequest(`${url}/poll`);
            const response = mode === 'zero-headers' ? null : page.waitForResponse(`${url}/poll`);
            await page.evaluate(() => { void fetch('/poll').then(r => r.text()).catch(() => {}); });
            await request;
            if (response) {
              const received = await response;
              if (mode === 'completed' || mode.endsWith('capture-lost')) {
                await received.finished();
                if (mode === 'completed') await page.waitForTimeout(120);
              }
              else await page.waitForTimeout(60);
            }
            wire.push({ event: 'reload', at: Date.now() });
            const result = await reload(...reloadArgs);
            if (mode.endsWith('capture-lost')) {
              for (const session of sessions) await session.detach();
              wire.push({ event: 'observer-detached', at: Date.now() });
            }
            return result;
          });
          return page;
        });
        return context;
      });
      const feature = compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 1, features: [{
        id: 1, name: 'finite HTTP privacy window', actors: ['reader'], setup: [], criteria: [{
          id: 'capture', desc: 'all delivered bytes remain observable through navigation', points: 1, steps: [
            { do: 'expect', actor: 'reader', testid: 'loaded', contains: 'ready' },
            ...(mode === 'silent' || mode === 'overflow' ? [{ do: 'click', actor: 'reader', testid: 'poll' }]
              : [{ do: 'reload', actor: 'reader', settleMs: 0 }]),
            ...(mode === 'overflow' ? [{ do: 'expect', actor: 'reader', testid: 'loaded', contains: 'consumed' }] : []),
            { do: 'expectNotReceived', actor: 'reader', contains: marker, within: 250 },
          ],
        }],
      }] }).features[0]!;
      const result = await gradeFeature(browser, feature,
        { url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false },
        { runId: 'finite-http', roomName: name => name, url, actions: [], spacetime: null,
          backend: 'postgres', nullControl: false, defaultWithin: 1000 });
      evidence.push({ mode, wire: [...wire], result });
      assert.equal(result.setupEvidence.status, 'passed', JSON.stringify(result));
      const expected = mode.includes('leak') || mode === 'completed' ? 'failed'
        : mode === 'capture-lost' || mode === 'overflow' || mode === 'silent' ? 'inconclusive' : 'passed';
      assert.equal(result.criteria[0]!.evidence.status, expected, JSON.stringify(evidence.at(-1)));
      if (mode === 'split-leak' || mode === 'gzip-leak') {
        assert(wire.find(event => event.event === 'second-chunk')!.at <= wire.find(event => event.event === 'reload')!.at,
          'The complete marker must reach the wire before reload');
      }
      if (mode === 'future-leak') assert(wire.some(event => event.event === 'later-leak'));
      assert.equal(browser.contexts().length, 0, 'All actor contexts must close');
    });
  }
});

// Partial bytes may prove a privacy failure, but must not become complete JSON
// for ID discovery or confirm a write for bulk replay before its response ends.
test('HTTP privacy fragments do not become completed source or confirmed writes', async t => {
  const marker = 'private-write-marker';
  const json = JSON.stringify({ id: 7, value: marker, padding: 'x'.repeat(2 * 1024 * 1024) });
  let finish: (() => void) | undefined;
  let append: ((end: number) => void) | undefined;
  const server = createServer((req, res) => {
    if (req.url === '/api/write') {
      res.writeHead(200, { 'Content-Type': 'application/json' });
      let sent = 100;
      res.write(json.slice(0, sent));
      append = end => { res.write(json.slice(sent, end)); sent = end; };
      finish = () => res.end();
    } else { res.setHeader('Content-Type', 'text/html'); res.end('<title>write receipt</title>'); }
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  t.after(() => { server.closeAllConnections(); server.close(); });
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  const browser = await chromium.launch({ headless: true, executablePath: chromium.executablePath() });
  const proxy = await startNetworkInterruption(true);
  const context = await browser.newContext({ proxy: proxy.proxy });
  const page = await context.newPage();
  const actor = new Actor('reader', page, context, false, false, false, [`${url}/api/write`], true, proxy);
  t.after(async () => { await context.close(); await proxy.dispose(); await browser.close(); });
  await actor.ready;
  await page.goto(url);
  const response = page.waitForResponse(`${url}/api/write`);
  await page.evaluate(() => { void fetch('/api/write', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: '{"name":"item"}' }).then(r => r.json()); });
  const received = await response;
  const observation = await executeAction(ACTION_REGISTRY, 'expectNotReceived',
    { do: 'expectNotReceived', actor: 'reader', contains: marker, within: 100 }, {
      capabilities: {
        actors: { get: (name: string) => name === 'reader' ? actor : undefined },
        'transport-observation': {
          defaultWithin: 1000, expand: (value: string) => value,
          sleep: (milliseconds: number) => new Promise<void>(resolve => setTimeout(resolve, milliseconds)),
          verification: { structural() {}, unverified() {}, verified() {} },
        },
      },
    });
  const before = { received: [...actor.received], confirmed: actor.writes.at(-1)?.confirmed ?? null, observation };
  assert(finish, 'The real response must have delivered its first bytes');
  assert(append);
  // Repeated observations of one growing 2 MiB body must not exhaust the
  // capture budget by counting every prefix as a separate response.
  for (let end = 256 * 1024; end <= json.length + 256 * 1024; end += 256 * 1024) {
    const length = Math.min(end, json.length);
    append(length);
    const deadline = Date.now() + 3000;
    while (!(await proxy.httpSnapshot()).records.some(record => record.body.length === length)) {
      assert(Date.now() < deadline, 'The proxy must receive the next real body prefix');
      await page.waitForTimeout(10);
    }
    await actor.syncReceived();
  }
  finish();
  await received.finished();
  const deadline = Date.now() + 1000;
  while ((!actor.received.includes(json) || actor.writes.at(-1)?.confirmed !== true) && Date.now() < deadline) await page.waitForTimeout(10);
  const after = { received: [...actor.received], confirmed: actor.writes.at(-1)?.confirmed ?? null };
  await actor.syncReceived();
  assert.equal(actor.wasSent('not-in-this-response'), false, 'Growing prefixes must count once against the byte limit');
  if (process.env.STACK_BENCH_CAPTURE_EVIDENCE) writeFileSync(`${process.env.STACK_BENCH_CAPTURE_EVIDENCE}.write-boundary.json`,
    JSON.stringify({ browser: browser.version(), before, after }, null, 2));
  assert(!before.received.some(body => body.includes(marker)), 'Partial JSON cannot supply discovered IDs');
  assert.notEqual(before.confirmed, true, 'Partial HTTP success cannot authorize bulk replay');
  assert.equal(observation.status, 'failed', JSON.stringify(observation));
  assert(after.received.includes(json), 'The completed response remains available to existing source consumers');
  assert.equal(after.confirmed, true, 'A complete successful response confirms the captured write');
});
