import { AlgebraicType, SumTypeVariant } from './algebraic_type';
import type { AlgebraicValue } from './algebraic_value';

export namespace ScheduleAt {
  export function getAlgebraicType(): AlgebraicType {
    return AlgebraicType.createSumType([
      new SumTypeVariant('Interval', AlgebraicType.createTimeDurationType()),
      new SumTypeVariant('Time', AlgebraicType.createTimestampType()),
    ]);
  }

  export type Interval = {
    tag: 'Interval';
    value: { __time_duration_micros__: BigInt };
  };
  export const Interval = (value: BigInt): Interval => ({
    tag: 'Interval',
    value: { __time_duration_micros__: value },
  });
  export type Time = {
    tag: 'Time';
    value: { __timestamp_micros_since_unix_epoch__: BigInt };
  };
  export const Time = (value: BigInt): Time => ({
    tag: 'Time',
    value: { __timestamp_micros_since_unix_epoch__: value },
  });

  export function fromValue(value: AlgebraicValue): ScheduleAt {
    let sumValue = value.asSumValue();
    switch (sumValue.tag) {
      case 0:
        return {
          tag: 'Interval',
          value: {
            __time_duration_micros__: sumValue.value
              .asProductValue()
              .elements[0].asBigInt(),
          },
        };
      case 1:
        return {
          tag: 'Time',
          value: {
            __timestamp_micros_since_unix_epoch__: sumValue.value.asBigInt(),
          },
        };
      default:
        throw 'unreachable';
    }
  }
}

export type ScheduleAt = ScheduleAt.Interval | ScheduleAt.Time;
export default ScheduleAt;