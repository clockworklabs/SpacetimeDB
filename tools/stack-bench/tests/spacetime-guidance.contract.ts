import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

test('SpacetimeDB guidance uses the repeatable local authentication path', () => {
  const guidance = readFileSync(join(STACK_BENCH_ROOT, 'backends', 'spacetime.md'), 'utf8');
  const commands = guidance.split(/\r?\n/).filter(line => line.includes('<STDB_BIN>'));
  const stateful = commands.filter(line => /\b(?:publish|dev)\b/.test(line));

  assert.equal(stateful.length, 2, 'expected one publish and one development command');
  for (const command of stateful) assert.match(command, /--yes\b/, command);
  assert.doesNotMatch(guidance, /echo\s+y\s*\|/i);
  assert.doesNotMatch(guidance, /--delete-data\b/);
  assert.doesNotMatch(guidance, /<STDB_BIN>\s+(?:publish|dev)[^\n]*--anonymous/);
});
