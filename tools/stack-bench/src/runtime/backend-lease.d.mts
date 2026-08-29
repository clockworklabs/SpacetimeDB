export interface BackendLeaseContainer {
  name: string;
  id: string;
  image?: string;
  owned?: boolean;
  running?: boolean;
  removedAt?: string;
  networkMode?: 'bridge' | 'host';
  resourceLimits?: {
    cpuCount: number;
    memoryBytes: number;
    memorySwapBytes: number;
    pids: number;
  };
}

export interface BackendResourceLock {
  path: string;
  key: string;
  digest: string;
  releasedAt?: string;
}

export interface BackendLeaseResource {
  serverUri: string | null;
  dataDir: string | null;
  module: string | null;
  database: string | null;
  container: BackendLeaseContainer | null;
  buildContainer: BackendLeaseContainer | null;
  locks: BackendResourceLock[];
  launchedPid: number | null;
  listenerPids: Array<number | string>;
}

export interface BackendLease {
  backend: string;
  runId: string;
  ownershipToken: string;
  state: string;
  releasedAt?: string;
  resources: BackendLeaseResource;
}

export interface SpacetimeBackendLease extends BackendLease {
  backend: 'spacetime';
  resources: BackendLeaseResource & {
    serverUri: string;
    module: string;
  };
}

export interface BackendLeaseExpectation {
  backend?: string;
  runId?: string;
  active?: boolean;
}

export function leaseFromEnv(
  env: NodeJS.ProcessEnv | undefined,
  expected: BackendLeaseExpectation & { backend: 'spacetime' },
): { path: string; lease: SpacetimeBackendLease };

export function leaseFromEnv(
  env?: NodeJS.ProcessEnv,
  expected?: BackendLeaseExpectation,
): { path: string; lease: BackendLease };

export function readBackendLease(
  path: string,
  expected?: BackendLeaseExpectation & { token?: string },
): BackendLease;

export function releaseResourceLocks(lease: BackendLease): void;

export function updateBackendLease(
  path: string,
  expected: { token?: string; backend?: string; runId?: string },
  update: (lease: BackendLease) => BackendLease,
): BackendLease;
