import assert from 'node:assert/strict';
import { test } from 'node:test';
import { resolveConfig, credentialExpiry } from '../src/config.js';

const target = {
  bucket: 'my-private-files',
  region: 'us-east-1',
  prefix: 'app/',
  accountId: '123456789012',
};
test('defaults bound file size, quota, grants, recovery and sweep frequency', () => {
  const config = resolveConfig(target);
  assert.equal(config.maxFileBytes, 100 * 1024 * 1024);
  assert.equal(config.maxFilesPerOwner, 100);
  assert.equal(config.maxBytesPerOwner, 1024 * 1024 * 1024);
  assert.equal(config.uploadSeconds, 300);
  assert.equal(config.downloadSeconds, 60);
  assert.equal(config.recoverySeconds, 3600);
  assert.equal(config.receiptSeconds, 86400);
  assert.equal(config.sweepSeconds, 300);
  assert.deepEqual(
    resolveConfig({ ...target, uploadSeconds: undefined }),
    config
  );
});
test('reject unsafe targets and unbounded or inconsistent limits', () => {
  for (const patch of [
    { bucket: 'a.b' },
    { bucket: 'https://example.com' },
    { bucket: 'files-ab12cd34-s3alias' },
    { region: '../host' },
    { region: 'cn-north-1' },
    { prefix: '' },
    { prefix: '../' },
    { prefix: 'app' },
    { accountId: '123' },
    { maxFileBytes: -1 },
    { maxFileBytes: Infinity },
    { maxFilesPerOwner: 0 },
    { maxFilesPerOwner: 1.5 },
    { maxBytesPerOwner: 1 },
    { uploadSeconds: 0 },
    { uploadSeconds: 3601 },
    { downloadSeconds: NaN },
    { httpTimeoutSeconds: 61 },
    { sweepSeconds: 0 },
    { recoverySeconds: -1 },
  ])
    assert.throws(
      () => resolveConfig({ ...target, ...patch }),
      /s3.invalid_config/
    );
});
test('temporary credentials need a known future expiry; rotation uses the supplied credential', () => {
  const credentials = { accessKeyId: 'key', secretAccessKey: 'secret' };
  assert.equal(credentialExpiry(credentials, 1000), Infinity);
  assert.throws(
    () => credentialExpiry({ ...credentials, sessionToken: 'token' }, 1000),
    /s3.invalid_credentials/
  );
  for (const expiresAt of [30000, 31500])
    assert.throws(
      () => credentialExpiry({ ...credentials, expiresAt }, 1000),
      /s3.credentials_expiring/
    );
  assert.equal(
    credentialExpiry(
      { ...credentials, sessionToken: 'new-token', expiresAt: 120000 },
      1000
    ),
    90000
  );
});
