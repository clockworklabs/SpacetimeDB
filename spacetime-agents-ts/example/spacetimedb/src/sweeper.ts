const ONE_SECOND_MICROS = 1_000_000n;
const ONE_MINUTE_MICROS = 60n * ONE_SECOND_MICROS;

export const SWEEPER_INTERVAL_MICROS = ONE_MINUTE_MICROS;

export function isStaleLock(
  nowMicros: bigint,
  lockedAtMicros: bigint,
  thresholdMicros: bigint
): boolean {
  return lockedAtMicros < nowMicros - thresholdMicros;
}
