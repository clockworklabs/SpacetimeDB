import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';

import {
  compilePackDefinition,
  type CompiledPackDefinition,
} from '../composition/composition-compiler.js';
import {
  compileProgressionDefinitionFile,
  type CompiledProgressionDefinition,
} from './progression-definition.js';

export const GRAPH_START = '/* STACK_BENCH_GRAPH_START */';
export const GRAPH_END = '/* STACK_BENCH_GRAPH_END */';

export interface ProgressionGraphNode {
  id: string;
  name: string;
  level: number;
  state: 'draft' | 'qualified';
  parents: string[];
  dependencyReasons: Record<string, string>;
  questline: string;
  featureRefs: string[];
  checks: number;
  points: number;
}

export interface ProgressionGraph {
  schemaVersion: 1;
  definition: { id: string; version: string; state: string };
  levels: number;
  questlines: Array<{ id: string; title: string }>;
  nodes: ProgressionGraphNode[];
}

export interface CompileProgressionGraphOptions {
  trackRoot?: string;
}

export interface WriteProgressionGraphOptions {
  definitionPath: string;
  htmlPath: string;
  trackRoot?: string;
}

function readJson(path: string): unknown {
  return JSON.parse(readFileSync(path, 'utf8')) as unknown;
}

function packCatalog(trackRoot: string): Map<string, CompiledPackDefinition> {
  const root = join(trackRoot, 'composition', 'packs');
  return new Map(
    readdirSync(root)
      .filter(name => name.endsWith('.json'))
      .map(name => {
        const pack = compilePackDefinition(readJson(join(root, name)), { source: name });
        return [`${pack.id}@${pack.version}`, pack];
      }),
  );
}

function averageParentPosition(
  node: ProgressionGraphNode,
  positions: ReadonlyMap<string, number>,
): number {
  if (node.parents.length === 0) return Number.POSITIVE_INFINITY;
  return node.parents.reduce((total, parent) => {
    const position = positions.get(parent);
    if (position === undefined) {
      throw new Error(`progression graph parent ${parent} must precede ${node.id}`);
    }
    return total + position;
  }, 0) / node.parents.length;
}

function orderForDisplay(nodes: ProgressionGraphNode[]): ProgressionGraphNode[] {
  const ordered: ProgressionGraphNode[] = [];
  const position = new Map<string, number>();
  const levels = Math.max(...nodes.map(node => node.level));

  for (let level = 1; level <= levels; level += 1) {
    const atLevel = nodes.filter(node => node.level === level);
    atLevel.sort((left, right) => {
      if (level === 1) return left.name.localeCompare(right.name);
      return averageParentPosition(left, position) - averageParentPosition(right, position)
        || left.questline.localeCompare(right.questline)
        || left.name.localeCompare(right.name);
    });
    for (const node of atLevel) {
      position.set(node.id, ordered.length);
      ordered.push(node);
    }
  }
  return ordered;
}

export function compileProgressionGraph(
  definitionPath: string,
  { trackRoot }: CompileProgressionGraphOptions = {},
): ProgressionGraph {
  const absoluteDefinition = resolve(definitionPath);
  const root = resolve(trackRoot ?? join(dirname(absoluteDefinition), '..'));
  const definition: CompiledProgressionDefinition = compileProgressionDefinitionFile(
    absoluteDefinition,
    { trackRoot: root },
  );
  const packs = packCatalog(root);
  const nodes = orderForDisplay(definition.nodes.map(node => {
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
    } satisfies ProgressionGraphNode;
  }));
  return {
    schemaVersion: 1,
    definition: { id: definition.id, version: definition.version, state: definition.state },
    levels: Math.max(...nodes.map(node => node.level)),
    questlines: definition.questlines.map(({ id, title }) => ({ id, title })),
    nodes,
  };
}

export function renderProgressionGraphHtml(html: string, graph: ProgressionGraph): string {
  const start = html.indexOf(GRAPH_START);
  const end = html.indexOf(GRAPH_END);
  if (start < 0 || end <= start) {
    throw new Error('dependency graph HTML has no generated-data markers');
  }
  const generated = `${GRAPH_START}\n  const graph = ${JSON.stringify(graph, null, 2)};\n  ${GRAPH_END}`;
  return `${html.slice(0, start)}${generated}${html.slice(end + GRAPH_END.length)}`;
}

export function writeProgressionGraph({
  definitionPath,
  htmlPath,
  trackRoot,
}: WriteProgressionGraphOptions): ProgressionGraph {
  const graph = compileProgressionGraph(definitionPath, { trackRoot });
  const html = readFileSync(htmlPath, 'utf8');
  const rendered = renderProgressionGraphHtml(html, graph);
  writeFileSync(htmlPath, rendered);
  return graph;
}
