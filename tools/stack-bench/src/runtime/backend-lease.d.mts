export interface BackendLeaseResource {
  serverUri: string | null;
  dataDir: string | null;
  module: string | null;
  database: string | null;
  container: { name: string; id: string } | null;
  buildContainer: { name: string; id: string } | null;
}

export interface BackendLease {
  backend: string;
  runId: string;
  ownershipToken: string;
  state: string;
  resources: BackendLeaseResource;
}

export interface BackendLeaseExpectation {
  backend?: string;
  runId?: string;
  active?: boolean;
}

export function leaseFromEnv(
  env?: NodeJS.ProcessEnv,
  expected?: BackendLeaseExpectation,
): { path: string; lease: BackendLease };

export function updateBackendLease(
  path: string,
  expected: { token?: string; backend?: string; runId?: string },
  update: (lease: BackendLease) => BackendLease,
): BackendLease;
