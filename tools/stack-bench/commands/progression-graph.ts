import { join } from 'node:path';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { writeProgressionGraph } from '../src/progression/progression-graph.js';

interface ProgressionGraph {
  nodes: unknown[];
  levels: number;
}

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const graph: ProgressionGraph = writeProgressionGraph({
  definitionPath: process.argv[2] ?? join(trackRoot, 'progression', 'ecommerce-1.0.0.json'),
  htmlPath: process.argv[3] ?? join(STACK_BENCH_ROOT, 'docs', 'dependency-graph.html'),
  trackRoot,
});
console.log(`Rendered ${graph.nodes.length} nodes across ${graph.levels} levels.`);
