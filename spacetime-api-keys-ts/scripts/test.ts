import * as assert from 'node:assert/strict';
import {
  base64Url,
  extractLookupPrefix,
  hashApiKey,
  matchesApiKeyHash,
  hasScope,
} from '../src/keys.ts';

assert.equal(base64Url(new Uint8Array([102, 111, 111])), 'Zm9v');
assert.equal(base64Url(new Uint8Array([255, 255, 255])), '____');
assert.equal(
  extractLookupPrefix('stdb_live_abcdefghijklmnop'),
  'stdb_live_abcdefghij'
);
assert.equal(extractLookupPrefix('missing-secret'), undefined);

const key = 'stdb_live_abcdefghijklmnop';
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
