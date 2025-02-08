/**
 * A difference between two points in time, represented as a number of microseconds.
 */
export class TimeDuration {
  __time_duration_micros__: bigint;

  private static MICROS_PER_MILLIS: bigint = 1000n;

  get micros(): bigint {
    return this.__time_duration_micros__;
  }

  get millis(): number {
    return Number(this.micros / TimeDuration.MICROS_PER_MILLIS);
  }

  constructor(micros: bigint) {
    this.__time_duration_micros__ = micros;
  }

  static fromMillis(millis: number): TimeDuration {
    return new TimeDuration(BigInt(millis) * TimeDuration.MICROS_PER_MILLIS);
  }
}
