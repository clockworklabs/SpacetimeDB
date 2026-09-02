import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { resolveGuidanceProfile } from '../src/campaigns/condition-compiler.js';
const root = STACK_BENCH_ROOT;
const files = Object.values(resolveGuidanceProfile('neutral@1.7.0',
  ['mongodb', 'postgres', 'spacetime']).documents)
  .map(document => resolve(root, document.path));

test('neutral backend documents contain only stack access facts', () => {
  const implementationAdvice = /\b(express|socket\.io|mongoose|drizzle|prisma|orm|polling|locking)\b/i;
  const evaluationLanguage = /stack bench|benchmark|harness|test(?:ing|ed|s)?|grader|score/i;
  const presentationAdvice = /brand|styling|theme|colour|color|app title/i;
  for (const file of files) {
    const content = readFileSync(file, 'utf8');
    assert.doesNotMatch(content, implementationAdvice, file);
    assert.doesNotMatch(content, evaluationLanguage, file);
    assert.doesNotMatch(content, presentationAdvice, file);
    assert.match(content, /Choose the/);
    assert.match(content, /## Connection/);
  }
});
