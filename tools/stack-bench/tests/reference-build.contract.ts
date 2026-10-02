import assert from 'node:assert/strict';
import test from 'node:test';
import { parseReferenceBuildArgs } from '../src/references/reference-build.js';

const condition = { id: 'local-reference', guidanceProfile: 'neutral-dev', repairPolicy: 'scored-only' };

test('reference builds reject the removed provider condition option', () => {
  assert.deepEqual(parseReferenceBuildArgs(['node', 'reference', '--backend', 'spacetime']),
    { backend: 'spacetime', fixture: null, out: null });
  assert.throws(() => parseReferenceBuildArgs(['node', 'reference', '--condition-json',
    JSON.stringify(condition)]), /Unknown option.*condition-json/);
});
