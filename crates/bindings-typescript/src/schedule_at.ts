import { AlgebraicType } from './algebraic_type';

export namespace ScheduleAt {
  export function getAlgebraicType(): AlgebraicType {
    return AlgebraicType.Sum({
      variants: [
        {
          name: 'Interval',
          algebraicType: AlgebraicType.createTimeDurationType(),
        },
        { name: 'Time', algebraicType: AlgebraicType.createTimestampType() },
      ],
    });
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
}

export type ScheduleAt = ScheduleAt.Interval | ScheduleAt.Time;
export default ScheduleAt;
