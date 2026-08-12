import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

test('a mismatched mutation fixture fails before acquiring any backend resource', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-mutation-preflight-'));
  const output = join(root, 'output');
  const manifest = join(root, 'mutations.json');
  const locks = join(tmpdir(), 'stack-bench-resource-locks');
  const lock = join(locks, `${createHash('sha256').update('slot:ecommerce:mongodb:run19').digest('hex')}.lock.json`);
  try {
    assert.equal(existsSync(lock), false, 'test slot is already leased');
    writeFileSync(join(root, 'source.txt'), 'fixture\n');
    // Use root as the explicit app; the manifest intentionally targets other
    // bytes. No Docker or database lookup should happen before this rejection.
    writeFileSync(manifest, JSON.stringify({ schemaVersion: 1, status: 'candidate',
      fixtureSha256: '0'.repeat(64), backend: 'mongodb', track: 'ecommerce', level: 1,
      scenario: 'tracks/ecommerce/scenarios/01-contention.json', mutations: [] }));
    assert.throws(() => execFileSync(process.execPath, [join(ROOT, 'bench.mjs'),
      '--backend', 'mongodb', '--track', 'ecommerce', '--levels', '1',
      '--run-index', '19', '--app', root, '--out', output,
      '--mutations', manifest, '--skip-probe', '--no-media'],
    { encoding: 'utf8', stdio: 'pipe', timeout: 30_000 }), /targets fixture/);
    assert.equal(existsSync(lock), false);
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
