export interface BundleOutcome extends Record<string, unknown> {
  kind: string;
  phase?: string;
  reason?: string;
  appFailures: unknown[];
  inconclusive: unknown[];
  harnessFailures: unknown[];
}

export function classifyBundle(bundle: unknown): BundleOutcome;
