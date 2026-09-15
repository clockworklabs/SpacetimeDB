import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash } from 'node:crypto';
import { createBackendLease } from '../src/runtime/backend-lease.js';
import { hashAppSource } from '../src/runtime/source-snapshot.js';
import { validatePopulatedCheckpoint } from '../src/runtime/source-materialization.js';

// The invariant: corrupt, incomplete, or cross-lease checkpoints fail validation
// without any lifecycle call. A matching source alone cannot authorize restore.
test('populated checkpoint requires matching source, archive, app and lease', async () => {
  const root = mkdtempSync(join(tmpdir(), 'populated-checkpoint-'));
  try {
    const app = join(root, 'app'), checkpoint = join(root, 'checkpoint');
    mkdirSync(app); mkdirSync(checkpoint); mkdirSync(join(checkpoint, 'source'));
    writeFileSync(join(checkpoint, 'source', 'start.sh'), '#!/bin/sh\n');
    const archive = Buffer.from('test archive');
    writeFileSync(join(checkpoint, 'database.tar'), archive);
    for (const backend of ['postgres', 'mongodb', 'spacetime']) {
      const lease = createBackendLease({ runId: `test-${backend}`, backend, track: 'ecommerce', runIndex: 0,
        database: 'checkpoint_test', module: 'checkpoint-test', serverUri: 'http://127.0.0.1:9000',
        dataDir: join(root, 'data') });
      lease.resources.container = { name: 'test', id: 'test-id', image: 'test-image', owned: true };
      const application = { backend, app, port: 5000, probe: '/' };
      const receipt = { version: 1, backend, runId: lease.runId,
        ownershipSha256: createHash('sha256').update(lease.ownershipToken).digest('hex'),
        container: 'test-id', image: 'test-image', app, createdAt: new Date().toISOString(),
        sourceSha256: hashAppSource(join(checkpoint, 'source')).sha256,
        dataSha256: createHash('sha256').update(archive).digest('hex'), dataBytes: archive.length };
      const save = (value: object) => writeFileSync(join(checkpoint, 'checkpoint.json'), JSON.stringify(value));
      save(receipt);
      assert.deepEqual(await validatePopulatedCheckpoint(checkpoint, application, lease), receipt);
      for (const nested of [join(app, 'checkpoint'), join(app, '..checkpoint')]) {
        mkdirSync(nested, { recursive: true });
        await assert.rejects(validatePopulatedCheckpoint(nested, application, lease), /separate private/);
      }
      for (const field of ['runId', 'ownershipSha256', 'container', 'image', 'app'] as const) {
        save({ ...receipt, [field]: field === 'ownershipSha256' ? '0'.repeat(64) : 'other' });
        await assert.rejects(validatePopulatedCheckpoint(checkpoint, application, lease), /does not belong/);
      }
      save({ ...receipt, sourceSha256: '0'.repeat(64) });
      await assert.rejects(validatePopulatedCheckpoint(checkpoint, application, lease), /changed/);
      save({ ...receipt, dataSha256: '0'.repeat(64) });
      await assert.rejects(validatePopulatedCheckpoint(checkpoint, application, lease), /changed/);
      save({ ...receipt, dataBytes: archive.length + 1 });
      await assert.rejects(validatePopulatedCheckpoint(checkpoint, application, lease), /changed/);
      save(receipt);
      writeFileSync(join(checkpoint, 'database.tar'), Buffer.alloc(archive.length));
      await assert.rejects(validatePopulatedCheckpoint(checkpoint, application, lease), /changed/);
      writeFileSync(join(checkpoint, 'database.tar'), archive);
      save({ version: 1 });
      await assert.rejects(validatePopulatedCheckpoint(checkpoint, application, lease));
    }
  } finally { rmSync(root, { recursive: true, force: true }); }
});
