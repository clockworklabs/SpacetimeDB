export type CanonicalDefinition =
  | null
  | boolean
  | number
  | string
  | CanonicalDefinition[]
  | { [key: string]: CanonicalDefinition };

export function canonicalizeDefinition(value: unknown, at?: string): CanonicalDefinition;
export function canonicalDefinitionJson(value: unknown): string;
export function compileTrackPlan(
  name: string,
  options?: { tracksDir?: string },
): CanonicalDefinition;
