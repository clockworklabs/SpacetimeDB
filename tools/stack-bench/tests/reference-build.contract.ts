import assert from 'node:assert/strict';
import test from 'node:test';
import { parseReferenceBuildArgs } from '../src/references/reference-build.js';
import { parseReferenceQualificationArgs } from '../src/references/reference-live.js';

const condition = { id: 'local-reference', guidanceProfile: 'neutral-dev', repairPolicy: 'scored-only' };

test('reference qualification preserves current conditions and rejects retired provider selection', () => {
  const parse = parseReferenceQualificationArgs;
  assert.equal(parse(['node', 'reference', '--backend', 'spacetime']).condition, undefined);
  assert.deepEqual(parse(['node', 'reference', '--backend', 'spacetime',
    '--condition-json', JSON.stringify(condition)]).condition, condition);
  assert.throws(() => parse(['node', 'reference', '--condition-json',
    JSON.stringify({ ...condition, authenticationProvider: 'keycloak' })]), /authenticationProvider/);
});

test('reference builds reject the removed provider condition option', () => {
  assert.deepEqual(parseReferenceBuildArgs(['node', 'reference', '--backend', 'spacetime']),
    { backend: 'spacetime', fixture: null, out: null });
  assert.throws(() => parseReferenceBuildArgs(['node', 'reference', '--condition-json',
    JSON.stringify(condition)]), /Unknown option.*condition-json/);
});
