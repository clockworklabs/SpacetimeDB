import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Timestamp } from 'spacetimedb';
import { resolveConfig } from '../src/config.js';
import { storage, type Http } from '../src/storage.js';
import type { FileRecord } from '../src/model.js';

const config = resolveConfig({
  bucket: 'my-private-files',
  region: 'us-east-1',
  prefix: 'app/',
  accountId: '123456789012',
});
const credentials = { accessKeyId: 'key', secretAccessKey: 'secret' };
const http = {
  fetch() {
    throw new Error('Upload grants do not call S3');
  },
} as unknown as Http;
const at = (millis: number) => new Timestamp(BigInt(millis) * 1000n);
const file = (uploadExpiresAt: number): FileRecord => ({
  id: 1n,
  owner: 'owner',
  requestKey: '["owner","request"]',
  filename: 'test.txt',
  contentType: 'text/plain',
  size: 4,
  checksum: 'n4bQgYhMfWWaL+qgxVrQFaO/TxsrC4Is0V1sFbDwCgg=',
  state: { tag: 'Pending' },
  createdAt: at(0),
  uploadExpiresAt: at(uploadExpiresAt),
  nextAttemptAt: at(uploadExpiresAt),
  attempt: 0n,
  lastError: undefined,
});

test('upload grants need at least one usable second', () => {
  const s3 = storage(config, 1n, credentials, http, () => at(1_000_000));
  assert.equal(
    s3.upload(file(1_001_000)).expiresAt.microsSinceUnixEpoch,
    1_001_000_000n
  );
  assert.throws(() => s3.upload(file(1_000_500)), /s3.upload_expired/);
});

test('download URLs save under the original filename', () => {
  const s3 = storage(config, 1n, credentials, http, () => at(1_000_000));
  const url = new URL(
    s3.download({ ...file(1_001_000), filename: "rock 'n' (roll)*.txt" }).url
  );
  assert.equal(url.pathname, '/app/final/1-1');
  assert.equal(
    url.searchParams.get('response-content-disposition'),
    "attachment; filename*=UTF-8''rock%20%27n%27%20%28roll%29%2A.txt"
  );
});
