import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import {
  compileProgressionGraph,
  renderProgressionGraphHtml,
} from '../src/progression/progression-graph.js';

const projectRoot = join(import.meta.dirname, '..', '..');
const trackRoot = join(projectRoot, 'tracks', 'ecommerce');
const definitionPath = join(trackRoot, 'progression', 'ecommerce-1.0.0.json');

test('the dependency graph page is generated from the ecommerce definition', () => {
  const htmlPath = join(projectRoot, 'docs', 'dependency-graph.html');
  const html = readFileSync(htmlPath, 'utf8');
  const graph = compileProgressionGraph(definitionPath, { trackRoot });
  assert.equal(graph.nodes.length, 39);
  assert.equal(graph.levels, 5);
  assert.deepEqual(graph.nodes.filter(node => node.level === 1).map(node => node.id),
    ['accounts', 'catalog', 'staff-access', 'support-intake']);
  assert.equal(renderProgressionGraphHtml(html, graph), html,
    'run npm run graph after changing the progression definition');
});

test('graph rendering rejects HTML without generated-data markers', () => {
  const graph = compileProgressionGraph(definitionPath, { trackRoot });
  assert.throws(() => renderProgressionGraphHtml('<html></html>', graph),
    /has no generated-data markers/);
});
