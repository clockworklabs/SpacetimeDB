import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

const root = join(import.meta.dirname, '..');
const files = ['mongodb', 'postgres', 'spacetime']
  .map(stack => join(root, 'backends', 'minimal', `${stack}.md`));

test('neutral backend documents contain access facts without implementation prescriptions', () => {
  const forbidden = /\b(express|socket\.io|mongoose|drizzle|orm|polling|transaction|locking)\b|project layout/i;
  for (const file of files) {
    const content = readFileSync(file, 'utf8');
    assert.doesNotMatch(content, forbidden, file);
    assert.match(content, /Make the remaining implementation choices\s+yourself/);
  }
});
