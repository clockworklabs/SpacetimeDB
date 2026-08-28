import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

const root = join(import.meta.dirname, '..');
const files = ['mongodb', 'postgres', 'spacetime']
  .map(stack => join(root, 'backends', 'minimal', `${stack}.md`));
const neutral12Files = ['mongodb', 'postgres', 'spacetime']
  .map(stack => join(root, 'backends', 'minimal', `${stack}-1.2.md`));

test('neutral backend documents contain access facts without implementation prescriptions', () => {
  const forbidden = /\b(express|socket\.io|mongoose|drizzle|orm|polling|transaction|locking)\b|project layout/i;
  for (const file of files) {
    const content = readFileSync(file, 'utf8');
    assert.doesNotMatch(content, forbidden, file);
    assert.match(content, /Make the remaining implementation choices\s+yourself/);
  }
});

test('neutral SpacetimeDB guidance states the required packaging interface', () => {
  const content = readFileSync(join(root, 'backends', 'minimal', 'spacetime-1.1.md'), 'utf8');
  assert.match(content, /Module source directory \| `\/app\/backend\/spacetimedb`/);
  assert.match(content, /packaging interface, not an implementation prescription/);
  assert.match(content, /choose the\s+schema, reducers, client architecture, persistence behavior, and project design\s+yourself/);
  assert.doesNotMatch(content, /\b(express|socket\.io|mongoose|drizzle|orm|polling|transaction|locking)\b/i);
});

test('neutral guidance 1.2 contains only stack access facts', () => {
  const implementationAdvice = /\b(express|socket\.io|mongoose|drizzle|prisma|orm|polling|locking)\b/i;
  const evaluationLanguage = /stack bench|benchmark|harness|test(?:ing|ed|s)?|grader|score/i;
  const presentationAdvice = /brand|styling|theme|colour|color|app title/i;
  for (const file of neutral12Files) {
    const content = readFileSync(file, 'utf8');
    assert.doesNotMatch(content, implementationAdvice, file);
    assert.doesNotMatch(content, evaluationLanguage, file);
    assert.doesNotMatch(content, presentationAdvice, file);
    assert.match(content, /Choose the/);
    assert.match(content, /## Connection/);
  }
});
