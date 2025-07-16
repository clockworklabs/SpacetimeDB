import { AlgebraicType, SumTypeVariant } from './algebraic_type';
import type { AlgebraicValue } from './algebraic_value';

export namespace ScheduleAt {
  export function getAlgebraicType(): AlgebraicType {
    return AlgebraicType.createSumType([
      new SumTypeVariant('Interval', AlgebraicType.createU64Type()),
      new SumTypeVariant('Time', AlgebraicType.createU64Type()),
    ]);
  }

  export function serialize(value: ScheduleAt): object {
    switch (value.tag) {
      case 'Interval':
        return { Interval: value.value };
      case 'Time':
        return { Time: value.value };
      default:
        throw 'unreachable';
    }
  }

  export type Interval = { tag: 'Interval'; value: BigInt };
  export const Interval = (value: BigInt): Interval => ({
    tag: 'Interval',
    value,
  });
  export type Time = { tag: 'Time'; value: BigInt };
  export const Time = (value: BigInt): Time => ({ tag: 'Time', value });

  export function fromValue(value: AlgebraicValue): ScheduleAt {
    let sumValue = value.asSumValue();
    switch (sumValue.tag) {
      case 0:
        return { tag: 'Interval', value: sumValue.value.asBigInt() };
      case 1:
        return { tag: 'Time', value: sumValue.value.asBigInt() };
      default:
        throw 'unreachable';
    }
  }
}

export type ScheduleAt = ScheduleAt.Interval | ScheduleAt.Time;
export default ScheduleAt;
