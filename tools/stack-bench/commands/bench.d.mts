export interface BenchArgs {
  pricing: unknown;
  runMode: unknown;
  experimentIdentity: unknown;
  featureCatalog: unknown;
  dependencyPolicy: unknown;
  progression: { definition: { policy: string } };
  progressionOwner: unknown;
  levelList: number[];
  campaignAdmission: { id: string; reusable: boolean };
  [key: string]: unknown;
}

export function parseArgs(argv: string[]): BenchArgs;
