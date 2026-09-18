import assert from 'node:assert/strict';
import test from 'node:test';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { parseReferenceBuildArgs } from '../src/references/reference-build.js';
import { parseReferenceQualificationArgs } from '../src/references/reference-live.js';
import { assertReferenceAuthentication } from '../src/references/reference-selection.js';

const condition = { id: 'provider-reference', guidanceProfile: 'neutral-dev',
  repairPolicy: 'scored-only', authenticationProvider: 'keycloak' as const };

test('the SpacetimeDB reference can build and qualify without an identity service', () => {
  const metadata = JSON.parse(readFileSync(join(STACK_BENCH_ROOT,
    'reference-apps/ecommerce/spacetime/reference.json'), 'utf8'));
  assert.doesNotThrow(() => assertReferenceAuthentication('ecommerce-reference-spacetime', metadata.requiredEnvironment, undefined, true));
});

test('reference builds select the provider only through a validated explicit study condition', () => {
  assert.equal(parseReferenceBuildArgs(['node', 'reference-build']).condition, undefined);
  const selected = parseReferenceBuildArgs(['node', 'reference-build', '--backend', 'spacetime',
    '--condition-json', JSON.stringify(condition)]);
  assert.deepEqual(selected.condition, condition);
  assert.throws(() => parseReferenceBuildArgs(['node', 'reference-build', '--condition-json',
    JSON.stringify({ ...condition, authenticationProvider: 'other' })]), /authenticationProvider/);
  assert.throws(() => parseReferenceBuildArgs(['node', 'reference-build', '--condition-json',
    JSON.stringify({ ...condition, implicitProvider: true })]), /unknown/);
});

test('provider-dependent references fail early; provider-on qualification stays ineligible', () => {
  assert.throws(() => assertReferenceAuthentication('oidc-reference', ['OIDC_ISSUER']), /explicitly selected/);
  assert.doesNotThrow(() => assertReferenceAuthentication('oidc-reference', ['OIDC_ISSUER'], condition));
  assert.doesNotThrow(() => assertReferenceAuthentication('local-auth-reference', []));
  const args = parseReferenceQualificationArgs(['node', 'reference-live', '--backend', 'spacetime',
    '--condition-json', JSON.stringify(condition)]);
  assert.deepEqual(args.condition, condition);
  assert.throws(() => assertReferenceAuthentication('oidc-reference', ['OIDC_ISSUER'], args.condition, true),
    /qualification scope.*do not yet bind the study condition/);
  assert.throws(() => assertReferenceAuthentication('local-auth-reference', [], condition, true),
    /qualification is blocked/);
});
