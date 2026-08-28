export interface RecoveryResult {
  ok: boolean;
  [key: string]: unknown;
}

export function recoverBackendLease(statePath: string, outputDirectory: string): RecoveryResult;
export function recoverSupervisedRun(statePath: string): RecoveryResult;
