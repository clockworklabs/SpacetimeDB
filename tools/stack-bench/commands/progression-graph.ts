import { dirname, join, resolve } from 'node:path';

import { STACK_BENCH_ROOT } from '../src/package-root.js';
import { writeProgressionGraph } from '../src/progression/progression-graph.js';

interface ProgressionGraph {
  nodes: unknown[];
  levels: number;
}

const definitionPath = process.argv[2];
if (!definitionPath) {
  throw new Error('usage: progression-graph <definition-path> [html-path]');
}
const resolvedDefinitionPath = resolve(definitionPath);
const graph: ProgressionGraph = writeProgressionGraph({
  definitionPath: resolvedDefinitionPath,
  htmlPath: process.argv[3] ?? join(STACK_BENCH_ROOT, 'docs', 'dependency-graph.html'),
  trackRoot: dirname(dirname(resolvedDefinitionPath)),
});
console.log(`Rendered ${graph.nodes.length} nodes across ${graph.levels} levels.`);
