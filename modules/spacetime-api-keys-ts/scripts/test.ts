import * as assert from 'node:assert/strict';
import {
  deriveKeySecret,
  extractLookupPrefix,
  formatApiKey,
  hashApiKey,
  matchesApiKeyHash,
  hasScope,
} from '../src/keys';

const secret = deriveKeySecret('operator-secret', 1n, 5n);
assert.equal(secret.length, 32);
assert.notDeepEqual(secret, deriveKeySecret('operator-secret', 2n, 5n));
assert.notDeepEqual(secret, deriveKeySecret('other-secret', 1n, 5n));

const formatted = formatApiKey('stdb_live', secret);
assert.match(formatted.key, /^stdb_live_[0-9a-f]{64}$/);
assert.equal(
  formatted.prefix,
  formatted.key.slice(0, 'stdb_live_'.length + 16)
);
assert.equal(extractLookupPrefix(formatted.key), formatted.prefix);
assert.equal(extractLookupPrefix('missing-secret'), undefined);
assert.equal(extractLookupPrefix('stdb_live_short'), undefined);

const key = formatted.key;
const hash = hashApiKey(key);
assert.match(hash, /^[0-9a-f]{64}$/);
assert.equal(matchesApiKeyHash(key, hash), true);
assert.equal(matchesApiKeyHash(`${key}x`, hash), false);
assert.equal(matchesApiKeyHash(key, 'not-hex'), false);

assert.equal(hasScope('["files:*","jobs:read"]', 'files:write'), true);
assert.equal(hasScope('["files:*","jobs:read"]', 'jobs:read'), true);
assert.equal(hasScope('["files:*","jobs:read"]', 'admin:write'), false);
assert.equal(hasScope('not-json', 'files:read'), false);

console.log('api-keys tests passed');
