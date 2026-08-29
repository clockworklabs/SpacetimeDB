export function writeProgressionState(path: string, input: unknown): unknown;
export function progressionStateExists(path: string): boolean;
export function readProgressionState(path: string, input: unknown): { state: unknown };
export function acquireProgressionStateLock(
  path: string,
  progression: unknown,
  featureCatalogIdentity: unknown,
  dependencyPolicyIdentity: unknown,
  owner: unknown,
): unknown;
export function releaseProgressionStateLock(lock: unknown): void;
