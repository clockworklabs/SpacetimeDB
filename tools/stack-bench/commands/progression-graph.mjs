import { join } from 'node:path';

import { writeProgressionGraph } from '../src/progression/progression-graph.mjs';

const root = join(import.meta.dirname, '..');
const trackRoot = join(root, 'tracks', 'ecommerce');
const graph = writeProgressionGraph({
  definitionPath: process.argv[2] ?? join(trackRoot, 'progression', 'ecommerce-1.0.0.json'),
  htmlPath: process.argv[3] ?? join(root, 'docs', 'dependency-graph.html'),
  trackRoot,
});
console.log(`Rendered ${graph.nodes.length} nodes across ${graph.levels} levels.`);
