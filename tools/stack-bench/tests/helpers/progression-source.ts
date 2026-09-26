import { readFileSync } from 'node:fs';

import { compileProgressionDefinition } from '../../src/progression/progression-definition.js';
import type { CompiledProgressionDefinition }
  from '../../src/progression/progression-definition.js';

export interface ValidatedProgressionSource {
  definition: CompiledProgressionDefinition;
  gradingGroups(nodeId: string): string[];
}

export function loadValidatedProgressionSource(
  path: string,
  trackRoot: string,
): ValidatedProgressionSource {
  const source: unknown = JSON.parse(readFileSync(path, 'utf8'));
  const definition = compileProgressionDefinition(source, { trackRoot, source: path });
  return { definition, gradingGroups: nodeId => authoredGradingGroups(source, nodeId) };
}

function authoredGradingGroups(source: unknown, nodeId: string): string[] {
  if (source === null || typeof source !== 'object' || Array.isArray(source)
    || !('nodes' in source) || !Array.isArray(source.nodes)) {
    throw new Error('validated progression source must contain nodes');
  }
  const node = source.nodes.find(candidate => candidate !== null && typeof candidate === 'object'
    && !Array.isArray(candidate) && 'id' in candidate && candidate.id === nodeId);
  if (!node || !('gradingGroups' in node) || !Array.isArray(node.gradingGroups)) {
    throw new Error(`validated progression node ${nodeId} must contain grading groups`);
  }
  for (const group of node.gradingGroups) {
    if (typeof group !== 'string') {
      throw new Error(`validated progression node ${nodeId} has an invalid grading group`);
    }
  }
  return node.gradingGroups;
}
