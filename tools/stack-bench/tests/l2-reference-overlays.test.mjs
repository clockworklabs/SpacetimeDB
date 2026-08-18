import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { loadReferenceRegistry, prepareReferenceFixtureSource } from '../src/references/reference-fixtures.mjs';

const fixtures = new Map(loadReferenceRegistry().fixtures
  .filter(fixture => fixture.id.startsWith('ecommerce-l2-server-actions-'))
  .map(fixture => [fixture.backend, fixture]));

const clientSource = (root, backend) => {
  const paths = backend === 'spacetime'
    ? ['client/src/components/ItemCard.tsx', 'client/src/components/OrdersPanel.tsx',
      'client/src/components/AdminPanel.tsx']
    : ['client/src/App.tsx'];
  return paths.map(path => readFileSync(join(root, ...path.split('/')), 'utf8')).join('\n');
};

test('L2 derived references expose every direct-action input with backend-native ids', () => {
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-l2-overlays-'));
  try {
    for (const backend of ['mongodb', 'postgres', 'spacetime']) {
      const fixture = fixtures.get(backend);
      assert(fixture, `missing ${backend} L2 derived fixture`);
      const first = join(root, `${backend}-first`);
      const second = join(root, `${backend}-second`);
      const firstHash = prepareReferenceFixtureSource(fixture, first);
      const secondHash = prepareReferenceFixtureSource(fixture, second);
      assert.equal(firstHash.sha256, secondHash.sha256, `${backend} overlay is not deterministic`);

      const source = clientSource(first, backend);
      for (const attribute of ['buy', 'ship', 'cancel', 'transfer', 'restock', 'price']) {
        assert.match(source, new RegExp(`data-${attribute}-input=`),
          `${backend} is missing data-${attribute}-input`);
      }
      if (backend === 'spacetime') {
        assert.match(source, /data-restock-input=\{JSON\.stringify\(\{ itemId: item\.id\.toString\(\), warehouseId: wh\.id\.toString\(\), quantity: 1 \}\)\}/);
        assert.match(source, /data-price-input=\{item\.name === 'Gaming Mouse' \? JSON\.stringify\(\{ itemId: item\.id\.toString\(\), price: 1 \}\) : undefined\}/);
        const module = readFileSync(join(first, 'backend', 'spacetimedb', 'src', 'index.ts'), 'utf8');
        assert.match(module,
          /ctx\.db\.item\.iter\(\)\]\.filter\(\(item\) => purchaseCountOf\(item\.id\) > 0\)/,
        'signed-out best sellers must exclude zero-purchase items');
      } else {
        assert.match(source, /data-restock-input=\{JSON\.stringify\(\{ itemId: loc\.itemId, warehouseId: loc\.warehouseId, quantity: 1 \}\)\}/);
        assert.match(source, /data-price-input=\{it\.name === "Gaming Mouse" \? JSON\.stringify\(\{ itemId: it\.id, price: 1 \}\) : undefined\}/);
        if (backend === 'mongodb') {
          const server = readFileSync(join(first, 'server', 'src', 'index.ts'), 'utf8');
          assert.match(server, /itemsLive\.filter\(\(item\) => item\.purchaseCount > 0\)\.slice\(0, 10\)/,
            'signed-out best sellers must exclude zero-purchase items');
          assert.match(server, /socket\.join\("visitors"\)/);
          assert.match(server, /await broadcastRecommendedForUser\(null\)/,
            'signed-out best sellers must receive live ranking updates');
        }
      }
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
