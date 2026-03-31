export function isInMeasurementWindow(
  measurementTimeMs: number,
  measureStartMs: number,
  measureEndMs: number
): boolean {
  return (
    measureStartMs > 0 &&
    measureEndMs > measureStartMs &&
    measurementTimeMs >= measureStartMs &&
    measurementTimeMs < measureEndMs
  );
}

export function countTransactionsInMeasurementWindow(
  transactionTimes: number[],
  measureStartMs: number,
  measureEndMs: number
): number {
  if (measureStartMs <= 0 || measureEndMs <= measureStartMs) {
    return 0;
  }

  let count = 0;
  for (const measurementTimeMs of transactionTimes) {
    if (
      isInMeasurementWindow(measurementTimeMs, measureStartMs, measureEndMs)
    ) {
      count += 1;
    }
  }

  return count;
}

export function getTpmC(
  measureStartMs: number,
  measureEndMs: number,
  measuredTransactionCount: number
): number | null {
  const nowMs = Date.now();

  if (measureStartMs <= 0 || measureEndMs <= measureStartMs) {
    return null;
  }

  if (nowMs < measureStartMs) {
    return null;
  }

  const effectiveEndMs = Math.min(nowMs, measureEndMs);
  const elapsedTimeSec = (effectiveEndMs - measureStartMs) / 1000;

  if (elapsedTimeSec <= 1) {
    return null;
  }

  return Math.trunc((measuredTransactionCount / elapsedTimeSec) * 60);
}
