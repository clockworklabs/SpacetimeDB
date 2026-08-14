import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const ROOT = join(dirname(fileURLToPath(import.meta.url)), '..');

test('SpacetimeDB guidance uses the repeatable local authentication path', () => {
  const guidance = readFileSync(join(ROOT, 'backends', 'spacetime.md'), 'utf8');
  const commands = guidance.split(/\r?\n/).filter(line => line.includes('<STDB_BIN>'));
  const stateful = commands.filter(line => /\b(?:publish|dev)\b/.test(line));

  assert.ok(stateful.length >= 3, 'expected publish, dev, and destructive publish examples');
  for (const command of stateful) assert.match(command, /--yes\b/, command);
  assert.doesNotMatch(guidance, /echo\s+y\s*\|/i);
  assert.doesNotMatch(guidance, /<STDB_BIN>\s+(?:publish|dev)[^\n]*--anonymous/);
});
