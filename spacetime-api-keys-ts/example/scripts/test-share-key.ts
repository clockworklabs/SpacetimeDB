import * as assert from 'node:assert/strict';
import { parseShareKey, shareKeyFromHash } from '../src/share-key';

assert.equal(parseShareKey('raw-key-value'), 'raw-key-value');
assert.equal(
  parseShareKey('http://127.0.0.1:8798/#key=shared%20key'),
  'shared key'
);
assert.equal(parseShareKey('http://127.0.0.1:8798/?key=query-secret'), null);
assert.equal(parseShareKey('http://127.0.0.1:8798/'), null);
assert.equal(parseShareKey('  '), null);
assert.equal(shareKeyFromHash('#key=abc123'), 'abc123');
assert.equal(shareKeyFromHash('?key=query-secret'), null);

console.log('API key share-link tests passed');
