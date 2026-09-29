import * as assert from 'node:assert/strict';
import { fileSha256Hex } from '../src/hash.ts';
import { responseHeaders } from '../src/headers.ts';
import { queryParam } from '../src/query.ts';
import {
  ownerPathKey,
  validateFilePath,
  validateMimeType,
} from '../src/validation.ts';

assert.equal(
  fileSha256Hex(new TextEncoder().encode('abc')),
  'ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad'
);
assert.equal(
  fileSha256Hex([]),
  'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'
);
assert.equal(queryParam('/files?id=42', 'id'), '42');
assert.equal(queryParam('/files?name=hello+world', 'name'), 'hello world');
assert.equal(queryParam('/files?id=%ZZ', 'id'), undefined);
assert.equal(queryParam('/files?other=1', 'id'), undefined);

assert.notEqual(
  ownerPathKey('owner-a', '/avatar.png'),
  ownerPathKey('owner-b', '/avatar.png')
);
assert.notEqual(ownerPathKey('a:b', '/c'), ownerPathKey('a', '/b/c'));
assert.equal(validateFilePath('/docs/readme.txt'), '/docs/readme.txt');
assert.throws(() => validateFilePath('docs/readme.txt'), /files\.invalid_path/);
assert.throws(() => validateFilePath('/docs/../secret'), /files\.invalid_path/);
assert.throws(
  () => validateFilePath('/docs/blocked\u007f.txt'),
  /files\.invalid_path/
);
assert.equal(validateMimeType('Image/SVG+XML'), 'image/svg+xml');
assert.throws(
  () => validateMimeType('text/plain\r\nx-injected: yes'),
  /files\.invalid_mime_type/
);

const served = (mimeType: string) =>
  responseHeaders({ mimeType, size: 1n, sha256Hex: 'x', visibility: 'public' });
assert.equal(served('image/png')['content-disposition'], undefined);
for (const mimeType of ['text/html', 'image/svg+xml', 'application/pdf']) {
  assert.equal(served(mimeType)['content-disposition'], 'attachment');
}
assert.equal(served('image/png')['x-content-type-options'], 'nosniff');
assert.equal(served('text/html')['content-security-policy'], 'sandbox');

console.log('files tests passed');
