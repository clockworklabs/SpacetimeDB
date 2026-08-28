export interface ProgressionGraph {
  nodes: unknown[];
  levels: number;
}

export function writeProgressionGraph(options: {
  definitionPath: string;
  htmlPath: string;
  trackRoot: string;
}): ProgressionGraph;
