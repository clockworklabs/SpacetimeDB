import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import test from 'node:test';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import {
  compileProgressionGraph,
  renderProgressionGraphHtml,
  type ProgressionGraph,
} from '../src/progression/progression-graph.js';

const projectRoot = STACK_BENCH_ROOT;
const trackRoot = join(projectRoot, 'tracks', 'ecommerce');
const definitionPath = join(trackRoot, 'progression', 'ecommerce-2.0.2.json');

test('the dependency graph page is generated from the ecommerce definition', () => {
  const htmlPath = join(projectRoot, 'docs', 'dependency-graph.html');
  const html = readFileSync(htmlPath, 'utf8');
  const graph = compileProgressionGraph(definitionPath, { trackRoot });
  assert.equal(graph.nodes.length, 43);
  assert.equal(graph.levels, 6);
  assert(graph.nodes.every(node => node.description.length > 0));
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

test('graph rendering escapes less-than signs in embedded JSON', () => {
  const graph: ProgressionGraph = {
    schemaVersion: 1,
    definition: { id: 'example.graph', version: '1.0.0', state: 'draft' },
    levels: 1,
    questlines: [{ id: 'example', title: 'Example <script>' }],
    nodes: [],
  };
  const html = 'before /* STACK_BENCH_GRAPH_START */ old /* STACK_BENCH_GRAPH_END */ after';
  const rendered = renderProgressionGraphHtml(html, graph);
  assert.match(rendered, /Example \\u003cscript>/);
  assert.doesNotMatch(rendered, /Example <script>/);
});
