export interface QualificationScopeIdentity {
  schemaVersion: number;
  kind: string;
  executableSha256: string;
  checksSha256: string;
  mutationSha256: string | null;
  recipe: { id: string; version: string; contentSha256: string };
  stack: unknown;
  sha256: string;
}

export function qualificationScopeIdentity(input: {
  kind: string;
  release: unknown;
  stack: string | null;
  reference: unknown;
  mutation: unknown;
  stackBenchRoot: string;
}): QualificationScopeIdentity;
