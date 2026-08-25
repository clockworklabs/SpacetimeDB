import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compileProgressionDefinition } from '../src/progression/progression-definition.mjs';

const trackRoot = join(import.meta.dirname, '..', 'tracks', 'ecommerce');
const source = join(trackRoot, 'progression', 'ecommerce-1.0.0.json');
const definition = () => JSON.parse(readFileSync(source, 'utf8'));

test('authored progression groups compile into exact scored checks', () => {
  const compiled = compileProgressionDefinition(definition(), { trackRoot, source });
  assert.equal(compiled.nodes.length, 39);
  assert.equal(compiled.nodes.flatMap(node => node.gradingChecks).length, 135);
  assert(compiled.nodes.every(node => node.promptModules.length === node.featureRefs.length));
});

test('authored progression rejects missing packs, groups, and feature grading ownership', async t => {
  await t.test('missing feature pack', () => {
    const input = definition();
    input.nodes[0].featureRefs = ['ecommerce.missing@1.0.0'];
    assert.throws(() => compileProgressionDefinition(input, { trackRoot }), /missing pack/);
  });
  await t.test('missing grading group', () => {
    const input = definition();
    input.nodes[0].gradingGroups[0] = 'ecommerce.feature.accounts@1.1.0#missing';
    assert.throws(() => compileProgressionDefinition(input, { trackRoot }), /missing group/);
  });
  await t.test('feature checks omitted from its owner', () => {
    const input = definition();
    input.nodes[0].gradingGroups = input.nodes[0].gradingGroups
      .filter(reference => !reference.endsWith('#account-create'));
    assert.throws(() => compileProgressionDefinition(input, { trackRoot }), /must own feature group/);
  });
  await t.test('group owned twice', () => {
    const input = definition();
    input.nodes[1].gradingGroups.push(input.nodes[0].gradingGroups[0]);
    assert.throws(() => compileProgressionDefinition(input, { trackRoot }), /already owned/);
  });
});
