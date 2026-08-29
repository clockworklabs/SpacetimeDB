export interface ResolvedGuidanceProfile {
  documents: Record<string, unknown>;
  credentialAliases: Record<string, string>;
  skills: Record<string, { ids: string[] }>;
}

export function resolveGuidanceProfile(
  requested: string,
  stacks: readonly string[],
): ResolvedGuidanceProfile;
