import type { ProgressionNodeState } from './progression-state.js';

export function dependencyNodeState(
  state: { nodes: Record<string, ProgressionNodeState> },
  nodeId: string,
): ProgressionNodeState {
  const node = state.nodes[nodeId];
  if (!node) throw new Error(`dependency mode state is missing node ${nodeId}`);
  return node;
}
