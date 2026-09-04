import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { readBackendGuidanceDocument } from '../commands/agent.js';
import { normalizePromptText } from '../src/agents/agent-materials.js';
import { sha256 } from '../src/evidence/provenance.js';

const documentPath = 'backends/postgres.md';
const absolute = join(STACK_BENCH_ROOT, documentPath);
const bytes = Buffer.from(normalizePromptText(readFileSync(absolute, 'utf8')), 'utf8');
const identity = {
  path: documentPath,
  sha256: sha256(bytes),
  bytes: bytes.length,
  applicationInterface: 'http' as const,
};

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
