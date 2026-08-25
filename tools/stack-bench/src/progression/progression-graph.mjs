import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';

import { compilePackDefinition } from '../composition/composition-compiler.mjs';
import { compileProgressionDefinitionFile } from './progression-definition.mjs';

export const GRAPH_START = '/* STACK_BENCH_GRAPH_START */';
export const GRAPH_END = '/* STACK_BENCH_GRAPH_END */';

const readJson = path => JSON.parse(readFileSync(path, 'utf8'));

function packCatalog(trackRoot) {
  const root = join(trackRoot, 'composition', 'packs');
  return new Map(readdirSync(root).filter(name => name.endsWith('.json')).map(name => {
    const pack = compilePackDefinition(readJson(join(root, name)), { source: name });
    return [`${pack.id}@${pack.version}`, pack];
  }));
}

export function compileProgressionGraph(definitionPath, { trackRoot } = {}) {
  const absoluteDefinition = resolve(definitionPath);
  const root = resolve(trackRoot ?? join(dirname(absoluteDefinition), '..'));
  const definition = compileProgressionDefinitionFile(absoluteDefinition, { trackRoot: root });
  const packs = packCatalog(root);
  const nodes = definition.nodes.map(node => {
    const selected = node.featureRefs.map(reference => {
      const pack = packs.get(reference);
      if (!pack) throw new Error(`${node.id} references missing feature pack ${reference}`);
      return pack;
    });
    return {
      id: node.id,
      name: node.title,
      level: node.level,
      state: selected.every(pack => pack.state === 'qualified') ? 'qualified' : 'draft',
      parents: node.dependencies,
      dependencyReasons: node.dependencyReasons,
      questline: node.questline,
      featureRefs: node.featureRefs,
      checks: node.gradingChecks.length,
      points: node.gradingChecks.reduce((total, check) => total + check.points, 0),
    };
  });
  return {
    schemaVersion: 1,
    definition: { id: definition.id, version: definition.version, state: definition.state },
    levels: Math.max(...nodes.map(node => node.level)),
    questlines: definition.questlines.map(({ id, title }) => ({ id, title })),
    nodes,
  };
}

export function renderProgressionGraphHtml(html, graph) {
  const start = html.indexOf(GRAPH_START);
  const end = html.indexOf(GRAPH_END);
  if (start < 0 || end <= start) throw new Error('dependency graph HTML has no generated-data markers');
  const generated = `${GRAPH_START}\n  const graph = ${JSON.stringify(graph, null, 2)};\n  ${GRAPH_END}`;
  return `${html.slice(0, start)}${generated}${html.slice(end + GRAPH_END.length)}`;
}

export function writeProgressionGraph({ definitionPath, htmlPath, trackRoot } = {}) {
  const graph = compileProgressionGraph(definitionPath, { trackRoot });
  const html = readFileSync(htmlPath, 'utf8');
  const rendered = renderProgressionGraphHtml(html, graph);
  writeFileSync(htmlPath, rendered);
  return graph;
}
