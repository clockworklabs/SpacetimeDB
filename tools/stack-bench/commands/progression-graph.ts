import { join } from 'node:path';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { writeProgressionGraph } from '../src/progression/progression-graph.js';

interface ProgressionGraph {
  nodes: unknown[];
  levels: number;
}

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const definitionPath = process.argv[2];
if (!definitionPath) {
  throw new Error('usage: progression-graph <definition-path> [html-path]');
}
const graph: ProgressionGraph = writeProgressionGraph({
  definitionPath,
  htmlPath: process.argv[3] ?? join(STACK_BENCH_ROOT, 'docs', 'dependency-graph.html'),
  trackRoot,
});
console.log(`Rendered ${graph.nodes.length} nodes across ${graph.levels} levels.`);
