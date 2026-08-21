import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { ACTION_IDS, compileScenarioDefinition } from '../src/composition/definition-compiler.mjs';
import { checkDefinitionGoldens } from '../commands/definition-goldens.mjs';
import { canonicalDefinitionJson } from '../src/composition/definition-plan.mjs';
import { TRACKS_DIR } from '../src/composition/tracks.mjs';

test('committed normalized definition plans have no semantic drift', () => {
  assert.deepEqual(checkDefinitionGoldens(), { checked: 4, changed: [] });
});

test('the compatibility fixture represents every registered action', () => {
  const path = join(TRACKS_DIR, '..', 'tests', 'fixtures', 'definitions', 'all-actions.json');
  const compiled = compileScenarioDefinition(JSON.parse(readFileSync(path, 'utf8')), { source: path });
  const represented = compiled.features[0].criteria[0].steps.map(step => step.do).sort();
  assert.deepEqual(represented, ACTION_IDS);
});

test('canonical definition JSON ignores object-key order but preserves execution order', () => {
  assert.equal(canonicalDefinitionJson({ b: 2, a: { d: 4, c: 3 } }),
    canonicalDefinitionJson({ a: { c: 3, d: 4 }, b: 2 }));
  assert.notEqual(canonicalDefinitionJson({ steps: ['first', 'second'] }),
    canonicalDefinitionJson({ steps: ['second', 'first'] }));
});
