import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compileProgressionDefinition } from '../src/progression/progression-definition.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const source = join(trackRoot, 'progression', 'ecommerce-1.0.0.json');
interface AuthoredNode {
  id: string;
  featureRefs: string[];
  gradingGroups: string[];
  dependencies: Array<{ id: string; reason: string }>;
}

interface AuthoredDefinition {
  nodes: AuthoredNode[];
}

const definition = (): AuthoredDefinition =>
  JSON.parse(readFileSync(source, 'utf8')) as AuthoredDefinition;

test('authored progression groups compile into exact scored checks', () => {
  const compiled = compileProgressionDefinition(definition(), { trackRoot, source });
  assert.equal(compiled.nodes.length, 39);
  assert.equal(compiled.nodes.flatMap(node => node.gradingChecks).length, 146);
  assert(compiled.nodes.every(node => node.promptModules.length === node.featureRefs.length));
});

test('authored progression rejects missing packs, groups, and feature grading ownership', async t => {
  await t.test('missing feature pack', () => {
    const input = definition();
    input.nodes[0]!.featureRefs = ['ecommerce.missing@1.0.0'];
    assert.throws(() => compileProgressionDefinition(input, { trackRoot }), /missing pack/);
  });
  await t.test('missing grading group', () => {
    const input = definition();
    input.nodes[0]!.gradingGroups[0] = 'ecommerce.feature.accounts@1.1.0#missing';
    assert.throws(() => compileProgressionDefinition(input, { trackRoot }), /missing group/);
  });
  await t.test('feature checks omitted from its owner', () => {
    const input = definition();
    input.nodes[0]!.gradingGroups = input.nodes[0]!.gradingGroups
      .filter(reference => !reference.endsWith('#account-create'));
    assert.throws(() => compileProgressionDefinition(input, { trackRoot }), /must own feature group/);
  });
  await t.test('group owned twice', () => {
    const input = definition();
    input.nodes[1]!.gradingGroups.push(input.nodes[0]!.gradingGroups[0]!);
    assert.throws(() => compileProgressionDefinition(input, { trackRoot }), /already owned/);
  });
  await t.test('prompt modules must support their calculated build mode', () => {
    const input = definition();
    const reviews = input.nodes.find(node => node.id === 'reviews');
    assert.ok(reviews);
    reviews.featureRefs = ['ecommerce.feature.reviews@1.1.0'];
    reviews.gradingGroups = reviews.gradingGroups.map(reference =>
      reference.replace('ecommerce.feature.reviews@1.2.0', 'ecommerce.feature.reviews@1.1.0'));
    assert.throws(() => compileProgressionDefinition(input, { trackRoot }),
      /compose no upgrade requirements/);
  });
  await t.test('feature pack dependencies must be graph ancestors', () => {
    const input = definition();
    const warehouse = input.nodes.find(node => node.id === 'warehouse-admin');
    assert.ok(warehouse);
    warehouse.dependencies = warehouse.dependencies.filter(dependency => dependency.id !== 'staff-access');
    assert.throws(() => compileProgressionDefinition(input, { trackRoot }),
      /requires feature .* outside the node and its ancestors/);
  });
});
