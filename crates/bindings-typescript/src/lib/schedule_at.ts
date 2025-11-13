import { AlgebraicType } from './algebraic_type';
import { TimeDuration, type TimeDurationAlgebraicType } from './time_duration';
import { Timestamp, type TimestampAlgebraicType } from './timestamp';

export type ScheduleAtAlgebraicType = {
  tag: 'Sum';
  value: {
    variants: [
      { name: 'Interval'; algebraicType: TimeDurationAlgebraicType },
      { name: 'Time'; algebraicType: TimestampAlgebraicType },
    ];
  };
};

type ScheduleAtType = Interval | Time;

export const ScheduleAt: {
  interval: (micros: bigint) => ScheduleAtType;
  time: (microsSinceUnixEpoch: bigint) => ScheduleAtType;
  /**
   * Get the algebraic type representation of the {@link ScheduleAt} type.
   * @returns The algebraic type representation of the type.
   */
  getAlgebraicType(): ScheduleAtAlgebraicType;
} = {
  interval(value: bigint): ScheduleAtType {
    return Interval(value);
  },
  time(value: bigint): ScheduleAtType {
    return Time(value);
  },
  getAlgebraicType(): ScheduleAtAlgebraicType {
    return AlgebraicType.Sum({
      variants: [
        {
          name: 'Interval',
          algebraicType: TimeDuration.getAlgebraicType(),
        },
        { name: 'Time', algebraicType: Timestamp.getAlgebraicType() },
      ],
    });
  },
};

export type Interval = {
  tag: 'Interval';
  value: TimeDuration;
};
export const Interval = (micros: bigint): Interval => ({
  tag: 'Interval',
  value: new TimeDuration(micros),
});
export type Time = {
  tag: 'Time';
  value: Timestamp;
};
export const Time = (microsSinceUnixEpoch: bigint): Time => ({
  tag: 'Time',
  value: new Timestamp(microsSinceUnixEpoch),
});

export default ScheduleAt;
export type ScheduleAt = ScheduleAtType;
