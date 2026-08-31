import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { compilePackDefinition } from '../src/composition/composition-compiler.js';
import { loadValidatedProgressionSource } from './helpers/progression-source.js';

const root = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const readJson = (path: string): unknown => JSON.parse(readFileSync(path, 'utf8'));
const readPack = (name: string) => compilePackDefinition(
  readJson(join(root, 'composition', 'packs', name)), { source: name });

test('dependency mode owns fulfilment behavior without duplicate authorization scoring', () => {
  const feature = readPack('progression-fulfilment-queue-1.0.1.json');
  const access = readPack('progression-operations-access-specifications-1.0.0.json');
  assert.equal(feature.moduleType, 'feature');
  assert.deepEqual(feature.checks.map(check => check.criteria), [['1a'], ['1b'], ['1c'], ['1d']]);
  assert(!feature.checks.some(check => check.criteria?.includes('1e') === true));
  assert.equal(access.moduleType, 'specification');
  assert.deepEqual(access.checks.map(check => check.requiresFeatures), [
    ['ecommerce.l2.stock-transfers-features'],
    ['ecommerce.l2.price-history-features'],
    ['ecommerce.progression.fulfilment-queue'],
    ['ecommerce.l2.order-cancellation-features'],
  ]);
  const { definition: graph, gradingGroups } = loadValidatedProgressionSource(
    join(root, 'progression', 'ecommerce-2.0.1.json'), root);
  const fulfilment = graph.nodes.find(node => node.id === 'fulfilment-queue');
  const cancellation = graph.nodes.find(node => node.id === 'order-cancellation');
  assert(fulfilment, 'the fulfilment graph node must exist');
  assert(cancellation, 'the cancellation graph node must exist');
  assert.deepEqual(fulfilment.featureRefs, ['ecommerce.progression.fulfilment-queue@1.0.1']);
  assert(gradingGroups(fulfilment.id)
    .some(group => group.endsWith('#operator-authorization-direct')));
  assert(!gradingGroups(fulfilment.id).some(group => group.endsWith('#order-owner-direct')));
  assert(gradingGroups(cancellation.id).some(group => group.endsWith('#order-owner-direct')));
});
