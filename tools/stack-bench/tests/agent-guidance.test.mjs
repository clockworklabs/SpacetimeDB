import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { readBackendGuidanceDocument } from '../agent.mjs';
import { sha256 } from '../provenance.mjs';

const documentPath = 'backends/postgres.md';
const absolute = join(import.meta.dirname, '..', documentPath);
const bytes = readFileSync(absolute);
const identity = { path: documentPath, sha256: sha256(bytes), bytes: bytes.length };

test('campaign guidance loads only the exact content-identified document', () => {
  assert.equal(readBackendGuidanceDocument(identity, 'unused.md'), bytes.toString('utf8'));
  assert.throws(() => readBackendGuidanceDocument({ ...identity, sha256: '0'.repeat(64) },
    'unused.md'), /changed after compilation/);
  assert.throws(() => readBackendGuidanceDocument({ ...identity, bytes: bytes.length + 1 },
    'unused.md'), /changed after compilation/);
});

test('campaign guidance rejects malformed and out-of-root paths', () => {
  assert.throws(() => readBackendGuidanceDocument({ ...identity, path: '..\\secret.md' },
    'unused.md'), /identity is invalid/);
  assert.throws(() => readBackendGuidanceDocument({ ...identity, path: '../../Cargo.toml' },
    'unused.md'), /escapes the Stack Bench root/);
});
