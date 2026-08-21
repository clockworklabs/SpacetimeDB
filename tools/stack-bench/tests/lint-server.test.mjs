import assert from 'node:assert/strict';
import { spawn } from 'node:child_process';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const ROOT = dirname(dirname(fileURLToPath(import.meta.url)));
const SERVER = join(ROOT, 'commands', 'lint-server.mjs');

async function waitForPort(path) {
  const deadline = Date.now() + 5_000;
  while (Date.now() < deadline) {
    if (existsSync(path)) {
      const value = Number(readFileSync(path, 'utf8'));
      if (Number.isInteger(value) && value > 0) return value;
    }
    await new Promise(resolve => setTimeout(resolve, 10));
  }
  throw new Error('lint server did not publish its port');
}

test('lint endpoint requires its exact route and per-session token', async () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-lint-server-'));
  const portFile = join(root, 'port');
  const token = 'a'.repeat(64);
  const command = `${JSON.stringify(process.execPath)} -e "process.stdout.write('lint-ok')"`;
  const child = spawn(process.execPath, [SERVER, '--port-file', portFile,
    '--cmd', command, '--token', token, '--host', '127.0.0.1'],
  { stdio: ['ignore', 'pipe', 'pipe'] });

  try {
    const port = await waitForPort(portFile);
    const endpoint = `http://127.0.0.1:${port}`;
    const missing = await fetch(`${endpoint}/lint`);
    assert.equal(missing.status, 403);
    assert.equal(await missing.text(), 'forbidden\n');

    const wrong = await fetch(`${endpoint}/lint`, {
      headers: { 'x-stack-bench-lint-token': 'b'.repeat(64) },
    });
    assert.equal(wrong.status, 403);

    const route = await fetch(`${endpoint}/not-lint`, {
      headers: { 'x-stack-bench-lint-token': token },
    });
    assert.equal(route.status, 404);

    const accepted = await fetch(`${endpoint}/lint`, {
      headers: { 'x-stack-bench-lint-token': token },
    });
    assert.equal(accepted.status, 200);
    assert.equal(await accepted.text(), 'lint-ok');
  } finally {
    child.kill();
    rmSync(root, { recursive: true, force: true });
  }
});
