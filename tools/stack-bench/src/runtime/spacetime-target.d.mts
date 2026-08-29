export interface SpacetimeTarget {
  readonly mod: string;
  readonly uri: string;
  readonly containerUri: string;
  readonly buildContainer: unknown;
}

export function leasedSpacetimeTarget(options?: {
  readonly requireBuildContainer?: boolean;
}): SpacetimeTarget;
