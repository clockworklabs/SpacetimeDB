export interface ProgressionAgentRequest {
  selection: {
    requested: {
      specifications: {
        requested: unknown[];
        expected: unknown[];
        observed: unknown[];
      };
    };
  };
  [key: string]: unknown;
}

export function resolveProgressionRecipeLevelSelection(
  binding: unknown,
  catalog: unknown,
  level: number,
  options: { cumulative: boolean },
): { agent: { request: ProgressionAgentRequest } };
