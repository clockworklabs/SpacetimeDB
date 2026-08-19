import assert from 'node:assert/strict';
import { mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

import { loadReferenceRegistry, prepareReferenceFixtureSource, selectReferenceFixture }
  from '../src/references/reference-fixtures.mjs';

const recipe = 'ecommerce.l2-standard@1.5.0';
const expected = new Map([
  ['mongodb', '2b678936abe7faa9670a914a425369f478956a2f5ef3326725d32a3b7aacebdb'],
  ['postgres', '7b3d6c936623ed8ff8b4ff3e61204c291b784c2d36c2bc1edbd5bcca47f9c952'],
  ['spacetime', 'b0bfc4405e684511874f5a867a5dc84e28b258e46729b7199a1e8aa5e27b61ce'],
]);

const clientSource = (root, backend) => {
  const paths = backend === 'spacetime'
    ? ['client/src/components/ItemCard.tsx', 'client/src/components/OrdersPanel.tsx',
      'client/src/components/AdminPanel.tsx', 'client/src/components/CartPanel.tsx']
    : ['client/src/App.tsx'];
  return paths.map(path => readFileSync(join(root, ...path.split('/')), 'utf8')).join('\n');
};

test('L2 1.5 references deterministically compose L1 2.4 and L2 action inputs', () => {
  const registry = loadReferenceRegistry();
  const root = mkdtempSync(join(tmpdir(), 'stack-bench-l2-1.5-overlays-'));
  try {
    for (const [backend, sourceSha256] of expected) {
      const fixture = selectReferenceFixture(registry, {
        backend, track: 'ecommerce', level: 2, recipe,
      });
      assert.equal(fixture.id, `ecommerce-l2-cumulative-1.5-${backend}`);
      assert.equal(fixture.status, 'candidate');
      assert.deepEqual(fixture.recipes, [recipe]);
      assert.equal(fixture.imported.sourceSha256, sourceSha256);

      const first = join(root, `${backend}-first`);
      const second = join(root, `${backend}-second`);
      assert.equal(prepareReferenceFixtureSource(fixture, first).sha256, sourceSha256);
      assert.equal(prepareReferenceFixtureSource(fixture, second).sha256, sourceSha256,
        `${backend} overlay must be deterministic`);

      const source = clientSource(first, backend);
      for (const action of ['buy', 'ship', 'cancel', 'transfer', 'restock', 'price', 'cart']) {
        const attribute = `data-${action}-input=`;
        assert.equal(source.split(attribute).length - 1, 1,
          `${backend} must expose exactly one ${attribute}`);
      }

      if (backend === 'spacetime') {
        assert.match(source,
          /data-cart-input=\{JSON\.stringify\(\{ itemId: line\.itemId\.toString\(\), quantity: -3 \}\)\}/);
        assert.match(source,
          /data-restock-input=\{JSON\.stringify\(\{ itemId: item\.id\.toString\(\), warehouseId: wh\.id\.toString\(\), quantity: 1 \}\)\}/);
        assert.match(source,
          /data-price-input=\{item\.name === 'Gaming Mouse' \? JSON\.stringify\(\{ itemId: item\.id\.toString\(\), price: 1 \}\) : undefined\}/);
      } else {
        assert.match(source,
          /data-cart-input=\{JSON\.stringify\(\{ itemId: line\.itemId, quantity: -3 \}\)\}/);
        assert.match(source,
          /data-restock-input=\{JSON\.stringify\(\{ itemId: loc\.itemId, warehouseId: loc\.warehouseId, quantity: 1 \}\)\}/);
        assert.match(source,
          /data-price-input=\{it\.name === "Gaming Mouse" \? JSON\.stringify\(\{ itemId: it\.id, price: 1 \}\) : undefined\}/);
      }
    }
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
});
