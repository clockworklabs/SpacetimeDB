import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { once } from 'node:events';
import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { dirname, join } from 'node:path';
import test from 'node:test';
import ts from 'typescript';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

// Failure cases: complete success bodies (including empty/non-JSON 2xx) remain
// accepted; explicit refusals and incomplete response bodies must reject.
// A broken response is not evidence that the server did or did not commit.
test('reference HTTP helpers preserve complete responses and reject body transport loss', async t => {
  const cases = [
    { name: 'json', status: 200, body: '{"ok":true}', expected: { ok: true } },
    { name: 'empty', status: 204, body: '', expected: {} },
    { name: 'text', status: 200, body: 'Saved', expected: {} },
    { name: 'refused', status: 403, body: '{"error":"Not allowed"}', expected: undefined },
    { name: 'body-aborted', status: 200, body: '{"ok":', expected: undefined },
  ];
  const observations: unknown[] = [];
  const sources: Array<{ path: string; sha256: string }> = [];
  const server = createServer((request, response) => {
    const fixture = cases.find(item => request.url === `/${item.name}`)!;
    response.writeHead(fixture.status, { 'Content-Type': fixture.name === 'text' ? 'text/plain' : 'application/json',
      ...(fixture.name === 'body-aborted' ? { 'Content-Length': 100 } : {}) });
    if (fixture.name === 'body-aborted') {
      response.write(fixture.body);
      setTimeout(() => response.destroy(), 50);
    } else response.end(fixture.body);
  }).listen(0, '127.0.0.1');
  await once(server, 'listening');
  const url = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
  try {
    for (const stack of ['postgres', 'mongodb', 'mongodb-app', 'postgres-app']) {
      const path = stack.endsWith('-app') ? `reference-apps/ecommerce/${stack.slice(0, -4)}/client/src/App.tsx`
        : `reference-apps/ecommerce/${stack}/client/src/request.ts`;
      const source = readFileSync(join(STACK_BENCH_ROOT, path), 'utf8');
      sources.push({ path, sha256: createHash('sha256').update(source).digest('hex') });
      let helper = source;
      if (stack.endsWith('-app')) {
        const name = stack === 'mongodb-app' ? 'apiFetch' : 'api';
        const file = ts.createSourceFile(path, source, ts.ScriptTarget.Latest, true, ts.ScriptKind.TSX);
        const declaration = file.statements.find(statement => ts.isFunctionDeclaration(statement)
          && statement.name?.text === name);
        assert.ok(declaration, 'Exercise the actual App request helper');
        helper = `${declaration.getText(file)}\nexport { ${name} as request };`;
      }
      const code = ts.transpileModule(helper, { compilerOptions: {
        target: ts.ScriptTarget.ES2022, module: ts.ModuleKind.ESNext,
      } }).outputText;
      const { request } = await import(`data:text/javascript;base64,${Buffer.from(code).toString('base64')}`);
      for (const fixture of cases) await t.test(`${stack}: ${fixture.name}`, async () => {
        let value: unknown, rejected = false;
        try { value = await request(`${url}/${fixture.name}`, stack === 'postgres' ? 'POST' : stack === 'postgres-app' ? {} : null); }
        catch { rejected = true; }
        observations.push({ stack, case: fixture.name, httpStatus: fixture.status,
          expectedRejection: fixture.expected === undefined, rejected, value });
        assert.equal(rejected, fixture.expected === undefined, `${stack}: ${fixture.name}`);
        if (!rejected) assert.deepEqual(value,
          stack === 'postgres-app' && ['empty', 'text'].includes(fixture.name) ? null : fixture.expected);
      });
    }
  } finally {
    server.closeAllConnections();
    await new Promise<void>(resolve => server.close(() => resolve()));
    if (process.env.STACK_BENCH_REFERENCE_REQUEST_EVIDENCE) {
      const path = process.env.STACK_BENCH_REFERENCE_REQUEST_EVIDENCE;
      mkdirSync(dirname(path), { recursive: true });
      writeFileSync(path, JSON.stringify({ rerun: 'node --test dist/tests/reference-request.integration.js',
        sources, observations, cleanup: { serverClosed: !server.listening } }, null, 2));
    }
  }
});
