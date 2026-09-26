import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { once } from 'node:events';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { createRequire } from 'node:module';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { chromium } from 'playwright';
import { gradeFeature } from '../grader/grade.js';
import { compileScenarioDefinition } from '../src/composition/definition-compiler.js';

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
    if (req.url === '/data') {
      const body = mode === 'leak' ? 'private-body-canary'
        : mode === 'captured' ? 'x'.repeat(8 * 1024 * 1024 - 32) : 'public';
      res.writeHead(200, { 'Content-Type': 'application/json' });
      res.write('{"value":"');
      if (mode !== 'stalled') setTimeout(() => res.end(`${body}"}`), 400);
    } else {
      res.writeHead(200, { 'Content-Type': 'text/html' });
      res.end(`${mode === 'captured' ? 'private-body-canary' : ''}<span id="ready">waiting</span><script>fetch('/data').then(() => {
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
  for (mode of ['clean', 'leak', 'captured', 'stalled']) {
    await t.test(mode, async () => {
      const feature = compileScenarioDefinition({ schemaVersion: 1, track: 'ecommerce', level: 1, features: [{
        id: 1, name: 'response completion', actors: ['reader'], setup: [],
        criteria: [{ id: 'capture', desc: 'no private response body', points: 1, steps: [
          { do: 'expect', actor: 'reader', testid: 'ready', contains: 'headers received' },
          { do: 'expectNotReceived', actor: 'reader', contains: 'private-body-canary', within: 50 },
        ] }],
      }] }).features[0]!;
      const result = await gradeFeature(browser, feature,
        { url, level: 1, headed: false, selectedCheckKeys: [], nullControl: false },
        { runId: 'body-completion', roomName: name => name, url, actions: [], spacetime: null,
          backend: 'postgres', nullControl: false, defaultWithin: 1000 });
      evidence.push({ mode, result });
      assert.equal(result.criteria[0]!.evidence.status,
        mode === 'clean' ? 'passed' : mode === 'stalled' ? 'inconclusive' : 'failed', JSON.stringify(result));
      assert.equal(browser.contexts().length, 0);
    });
  }
});
