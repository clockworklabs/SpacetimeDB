import { AlgebraicType } from './algebraic_type';

export const ScheduleAt: {
  getAlgebraicType(): AlgebraicType;
} = {
  getAlgebraicType(): AlgebraicType {
    return AlgebraicType.Sum({
      variants: [
        {
          name: 'Interval',
          algebraicType: AlgebraicType.createTimeDurationType(),
        },
        { name: 'Time', algebraicType: AlgebraicType.createTimestampType() },
      ],
    });
  },
};

export type Interval = {
  tag: 'Interval';
  value: { __time_duration_micros__: bigint };
};
export const Interval = (value: bigint): Interval => ({
  tag: 'Interval',
  value: { __time_duration_micros__: value },
});
export type Time = {
  tag: 'Time';
  value: { __timestamp_micros_since_unix_epoch__: bigint };
};
export const Time = (value: bigint): Time => ({
  tag: 'Time',
  value: { __timestamp_micros_since_unix_epoch__: value },
});

export type ScheduleAt = Interval | Time;
export default ScheduleAt;
