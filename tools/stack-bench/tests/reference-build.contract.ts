import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { parseReferenceBuildArgs } from '../src/references/reference-build.js';
import { parseReferenceQualificationArgs } from '../src/references/reference-live.js';

const condition = { id: 'local-reference', guidanceProfile: 'neutral-dev', repairPolicy: 'scored-only' };

test('the SpacetimeDB reference uses no supplied identity service', () => {
  const metadata = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'reference-apps/ecommerce/spacetime/reference.json'), 'utf8'));
  assert(!metadata.requiredEnvironment.some((name: string) => /OIDC|KEYCLOAK/.test(name)));
});

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
