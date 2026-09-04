export const DEFAULT_STALE_LOCK_SWEEP_BATCH = 500;

export interface ThreadLockLike {
  lockedAt: {
    microsSinceUnixEpoch: bigint;
  };
}

export function staleLockCutoffMicros(
  nowMicros: bigint,
  thresholdMicros: bigint
): bigint {
  return nowMicros - thresholdMicros;
}

export function deleteStaleThreadLocks<T extends ThreadLockLike>(
  expiredLocks: Iterable<T>,
  cutoffMicros: bigint,
  deleteLock: (lock: T) => void,
  maxRows = DEFAULT_STALE_LOCK_SWEEP_BATCH
): number {
  if (!Number.isInteger(maxRows) || maxRows <= 0) {
    throw new Error('agents.invalid_stale_lock_sweep_batch');
  }

  let deleted = 0;
  for (const lock of expiredLocks) {
    if (deleted >= maxRows) break;
    if (lock.lockedAt.microsSinceUnixEpoch >= cutoffMicros) break;
    deleteLock(lock);
    deleted++;
  }
  return deleted;
}
