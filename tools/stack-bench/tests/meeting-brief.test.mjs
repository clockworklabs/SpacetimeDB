import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { buildRecipeRelease } from '../recipe-release.mjs';

const root = join(import.meta.dirname, '..');
const trackRoot = join(root, 'tracks', 'ecommerce');
const html = readFileSync(join(root, 'MEETING-BRIEF.html'), 'utf8');

function embeddedCatalog() {
  const match = html.match(/<script type="application\/json" id="test-catalog-data">([^<]+)<\/script>/);
  assert(match, 'meeting brief must embed its test catalog');
  return JSON.parse(match[1]);
}

function releaseCatalog(recipe) {
  return buildRecipeRelease(join(trackRoot, 'composition', 'recipes', recipe), { trackRoot })
    .checkCatalog.map(check => ({
      key: check.stableKey,
      area: check.executionId,
      description: check.description,
      points: check.points,
    }));
}

test('meeting brief enumerates the exact promoted L1 and L2 checks', () => {
  const catalog = embeddedCatalog();
  assert.deepEqual(catalog['1'], releaseCatalog('l1-modular-2.3.0.json'));
  assert.deepEqual(catalog['2'], releaseCatalog('l2-standard-1.4.0.json'));
});

test('meeting brief keeps retired framing and obsolete comparison data out of view', () => {
  assert.doesNotMatch(html, /unmentioned|unprescribed|withheld|hidden|undisclosed/i);
  assert.doesNotMatch(html, /104\/106|\$65\.1764|\$22\.1678/);
  assert.match(html, /46 scored checks/);
  assert.match(html, /74 scored checks/);
  assert.match(html, /Browse all 46 scored L1 checks/);
  assert.match(html, /2 checks · included in every run/);
  assert.match(html, /105\/105 defects caught/);
  assert.doesNotMatch(html, /<th>Status<\/th>|additional checks run but do not affect the score/);
});
