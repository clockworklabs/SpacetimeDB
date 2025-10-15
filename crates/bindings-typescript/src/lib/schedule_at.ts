import { AlgebraicType } from './algebraic_type';
import { TimeDuration } from './time_duration';
import { Timestamp } from './timestamp';

export const ScheduleAt: {
  /**
   * Get the algebraic type representation of the {@link ScheduleAt} type.
   * @returns The algebraic type representation of the type.
   */
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
  value: TimeDuration;
};
export const Interval = (value: bigint): Interval => ({
  tag: 'Interval',
  value: new TimeDuration(value),
});
export type Time = {
  tag: 'Time';
  value: Timestamp;
};
export const Time = (value: bigint): Time => ({
  tag: 'Time',
  value: new Timestamp(value),
});

export type ScheduleAt = Interval | Time;
export default ScheduleAt;
