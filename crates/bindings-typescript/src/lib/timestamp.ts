import { AlgebraicType } from './algebraic_type';
import { TimeDuration } from './time_duration';

export type TimestampAlgebraicType = {
  tag: 'Product';
  value: {
    elements: [
      {
        name: '__timestamp_micros_since_unix_epoch__';
        algebraicType: { tag: 'I64' };
      },
    ];
  };
};

/**
 * A point in time, represented as a number of microseconds since the Unix epoch.
 */
export class Timestamp {
  __timestamp_micros_since_unix_epoch__: bigint;

  private static MICROS_PER_MILLIS: bigint = 1000n;

  get microsSinceUnixEpoch(): bigint {
    return this.__timestamp_micros_since_unix_epoch__;
  }

  constructor(micros: bigint) {
    this.__timestamp_micros_since_unix_epoch__ = micros;
  }

  /**
   * Get the algebraic type representation of the {@link Timestamp} type.
   * @returns The algebraic type representation of the type.
   */
  static getAlgebraicType(): TimestampAlgebraicType {
    return AlgebraicType.Product({
      elements: [
        {
          name: '__timestamp_micros_since_unix_epoch__',
          algebraicType: AlgebraicType.I64,
        },
      ],
    });
  }

  /**
   * The Unix epoch, the midnight at the beginning of January 1, 1970, UTC.
   */
  static UNIX_EPOCH: Timestamp = new Timestamp(0n);

  /**
   * Get a `Timestamp` representing the execution environment's belief of the current moment in time.
   */
  static now(): Timestamp {
    return Timestamp.fromDate(new Date());
  }

  /**
   * Get a `Timestamp` representing the same point in time as `date`.
   */
  static fromDate(date: Date): Timestamp {
    const millis = date.getTime();
    const micros = BigInt(millis) * Timestamp.MICROS_PER_MILLIS;
    return new Timestamp(micros);
  }

  /**
   * Get a `Date` representing approximately the same point in time as `this`.
   *
   * This method truncates to millisecond precision,
   * and throws `RangeError` if the `Timestamp` is outside the range representable as a `Date`.
   */
  toDate(): Date {
    const micros = this.__timestamp_micros_since_unix_epoch__;
    const millis = micros / Timestamp.MICROS_PER_MILLIS;
    if (
      millis > BigInt(Number.MAX_SAFE_INTEGER) ||
      millis < BigInt(Number.MIN_SAFE_INTEGER)
    ) {
      throw new RangeError(
        "Timestamp is outside of the representable range of JS's Date"
      );
    }
    return new Date(Number(millis));
  }

  since(other: Timestamp): TimeDuration {
    return new TimeDuration(
      this.__timestamp_micros_since_unix_epoch__ -
        other.__timestamp_micros_since_unix_epoch__
    );
  }
}
