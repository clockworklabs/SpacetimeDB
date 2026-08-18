import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { loadReferenceRegistry, prepareReferenceFixtureSource }
  from '../src/references/reference-fixtures.mjs';

const expected = new Map([
  ['mongodb', '76810d72211fc0182aa31b663ffc153a82ff1918cd34902187873a4b53a4ebf2'],
  ['postgres', '389d778f1835377fd2f92864d6afa20851c65076c80ddecba1e860cf7f4d9ec9'],
  ['spacetime', 'd5cb5af9db96b3ae4ff2b0d928dec4394ac6f88dfdce83201a0d8508b69902e5'],
]);

const fixtures = new Map(loadReferenceRegistry().fixtures
  .filter(fixture => fixture.id.startsWith('ecommerce-l1-action-inputs-2.4-'))
  .map(fixture => [fixture.backend, fixture]));

test('L1 2.4 references expose exact buy, restock, and cart action inputs', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-l1-2.4-overlays-'));
  try {
    for (const [backend, sourceSha256] of expected) {
      const fixture = fixtures.get(backend);
      assert(fixture, `missing ${backend} L1 2.4 fixture`);
      assert.deepEqual(fixture.recipes, ['ecommerce.l1-modular@2.4.0']);
      const first = join(root, `${backend}-first`);
      const second = join(root, `${backend}-second`);
      assert.equal(prepareReferenceFixtureSource(fixture, first).sha256, sourceSha256);
      assert.equal(prepareReferenceFixtureSource(fixture, second).sha256, sourceSha256,
        `${backend} overlay must be deterministic`);
      const client = readFileSync(join(first, 'client', 'src', 'App.tsx'), 'utf8');
      for (const attribute of ['data-buy-input=', 'data-restock-input=', 'data-cart-input=']) {
        assert.equal(client.split(attribute).length - 1, 1,
          `${backend} must expose exactly one ${attribute}`);
      }
      assert.match(client, /data-cart-input=\{JSON\.stringify\(\{ itemId: line\.itemId(?:\.toString\(\))?, quantity: -3 \}\)\}/);
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
