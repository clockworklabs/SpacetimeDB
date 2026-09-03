import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { compileProgressionDefinition } from '../src/progression/progression-definition.js';
import { STACK_BENCH_ROOT } from '../src/package-root.js';

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const source = join(trackRoot, 'progression', 'ecommerce-2.0.2.json');
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
  assert.equal(compiled.nodes.length, 43);
  assert.equal(compiled.nodes.flatMap(node => node.gradingChecks).length, 157);
  assert(compiled.nodes.every(node => node.promptModules.length === node.featureRefs.length));
  assert(compiled.nodes.every(node => node.gradingChecks.some(check => check.role === 'feature')));
  assert(compiled.nodes.flatMap(node => node.gradingChecks)
    .every(check => check.role === 'feature' || check.role === 'guarantee'));
});

test('a test-driven product edge is rejected', () => {
  const input = definition();
  const dashboard = input.nodes.find(node => node.id === 'inventory-dashboard');
  assert.ok(dashboard);
  dashboard.dependencies.push({
    id: 'purchasing', reason: 'A grading scenario uses purchase data.',
  });
  assert.throws(() => compileProgressionDefinition(input, { trackRoot }),
    /inventory-dashboard\.dependencies: must equal minimal product dependencies: warehouse-admin/);
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
  await t.test('feature references and grading groups use the same exact version', () => {
    const input = definition();
    const reviews = input.nodes.find(node => node.id === 'reviews');
    assert.ok(reviews);
    reviews.featureRefs = ['ecommerce.feature.reviews@1.1.0'];
    assert.throws(() => compileProgressionDefinition(input, { trackRoot }),
      /must own feature group ecommerce\.feature\.reviews@1\.1\.0#reviews/);
  });
  await t.test('missing product dependency is rejected', () => {
    const input = definition();
    const purchasing = input.nodes.find(node => node.id === 'purchasing');
    assert.ok(purchasing);
    purchasing.dependencies = purchasing.dependencies.filter(dependency => dependency.id !== 'accounts');
    assert.throws(() => compileProgressionDefinition(input, { trackRoot }),
      /purchasing\.dependencies: must equal minimal product dependencies: accounts, catalog/);
  });
  await t.test('unnecessary product dependency is rejected', () => {
    const input = definition();
    const reviews = input.nodes.find(node => node.id === 'reviews');
    assert.ok(reviews);
    reviews.dependencies.push({ id: 'accounts', reason: 'A grading scenario uses a customer.' });
    assert.throws(() => compileProgressionDefinition(input, { trackRoot }),
      /reviews\.dependencies: must equal minimal product dependencies: purchasing/);
  });
});
