import { join } from 'node:path';
import { pathToFileURL } from 'node:url';

import { STACK_BENCH_ROOT } from '../src/package-root.js';

interface ProgressionGraph {
  nodes: unknown[];
  levels: number;
}

type WriteProgressionGraph = (options: {
  definitionPath: string;
  htmlPath: string;
  trackRoot: string;
}) => ProgressionGraph;

async function loadGraphWriter(): Promise<WriteProgressionGraph> {
  const sourceUrl = pathToFileURL(join(
    STACK_BENCH_ROOT,
    'src',
    'progression',
    'progression-graph.mjs',
  )).href;
  const module: unknown = await import(sourceUrl);
  if (typeof module !== 'object' || module === null || !('writeProgressionGraph' in module)
    || typeof module.writeProgressionGraph !== 'function') {
    throw new Error('progression graph writer is unavailable');
  }
  return module.writeProgressionGraph as WriteProgressionGraph;
}

const trackRoot = join(STACK_BENCH_ROOT, 'tracks', 'ecommerce');
const writeProgressionGraph = await loadGraphWriter();
const graph = writeProgressionGraph({
  definitionPath: process.argv[2] ?? join(trackRoot, 'progression', 'ecommerce-1.0.0.json'),
  htmlPath: process.argv[3] ?? join(STACK_BENCH_ROOT, 'docs', 'dependency-graph.html'),
  trackRoot,
});
console.log(`Rendered ${graph.nodes.length} nodes across ${graph.levels} levels.`);
