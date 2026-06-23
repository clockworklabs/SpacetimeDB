import { describe, expect, it, vi } from 'vitest';

const { moduleHooks } = vi.hoisted(() => ({
  moduleHooks: Symbol('moduleHooks'),
}));

vi.mock('spacetime:sys@2.0', () => ({
  moduleHooks,
}));

vi.mock('spacetime:sys@2.1', () => ({}));

vi.mock('../src/server/runtime', () => ({
  makeHooks: () => ({}),
  callUserFunction: (fn: (...args: unknown[]) => unknown, ...args: unknown[]) =>
    fn(...args),
  ReducerCtxImpl: class {},
  runWithTx: () => undefined,
  sys: {},
}));

import { schema } from '../src/server/schema';
import { table } from '../src/lib/table';
import { t } from '../src/lib/type_builders';

describe('schema schedules', () => {
  it('emits schedules registered with the separate schedule API', () => {
    const scheduledMessages = table(
      { name: 'scheduled_messages' },
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
        text: t.string(),
      }
    );

    const spacetime = schema({ scheduledMessages });
    const processScheduledMessage = spacetime.reducer(
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );
    const schedules = spacetime.schedule(
      scheduledMessages,
      processScheduledMessage
    );

    spacetime[moduleHooks]({ processScheduledMessage, schedules });

    expect(spacetime.moduleDef.schedules).toEqual([
      {
        sourceName: undefined,
        tableName: 'scheduledMessages',
        scheduleAtCol: 1,
        functionName: 'processScheduledMessage',
      },
    ]);
  });

  it('emits procedure schedules registered with the separate schedule API', () => {
    const scheduledMessages = table(
      {},
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
        text: t.string(),
      }
    );

    const spacetime = schema({ scheduledMessages });
    const processScheduledMessage = spacetime.procedure(
      { scheduledMessage: scheduledMessages.rowType },
      t.unit(),
      () => ({})
    );
    const schedules = spacetime.schedule(
      scheduledMessages,
      processScheduledMessage
    );

    spacetime[moduleHooks]({ processScheduledMessage, schedules });

    expect(spacetime.moduleDef.schedules).toEqual([
      {
        sourceName: undefined,
        tableName: 'scheduledMessages',
        scheduleAtCol: 1,
        functionName: 'processScheduledMessage',
      },
    ]);
  });

  it('keeps legacy table scheduled option working', () => {
    const scheduledMessages = table(
      {
        scheduled: () => processScheduledMessage,
      },
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
        text: t.string(),
      }
    );
    const spacetime = schema({ scheduledMessages });
    const processScheduledMessage = spacetime.reducer(
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );

    spacetime[moduleHooks]({ processScheduledMessage });

    expect(spacetime.moduleDef.schedules).toEqual([
      {
        sourceName: undefined,
        tableName: 'scheduledMessages',
        scheduleAtCol: 1,
        functionName: 'processScheduledMessage',
      },
    ]);
  });

  it('keeps legacy table scheduled option working for procedures', () => {
    const scheduledMessages = table(
      {
        scheduled: () => processScheduledMessage,
      },
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
        text: t.string(),
      }
    );
    const spacetime = schema({ scheduledMessages });
    const processScheduledMessage = spacetime.procedure(
      { scheduledMessage: scheduledMessages.rowType },
      t.unit(),
      () => ({})
    );

    spacetime[moduleHooks]({ processScheduledMessage });

    expect(spacetime.moduleDef.schedules).toEqual([
      {
        sourceName: undefined,
        tableName: 'scheduledMessages',
        scheduleAtCol: 1,
        functionName: 'processScheduledMessage',
      },
    ]);
  });

  it('keeps legacy table scheduled option as a no-op without ScheduleAt', () => {
    const scheduledMessages = table(
      {
        scheduled: () => processScheduledMessage,
      },
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        text: t.string(),
      }
    );
    const spacetime = schema({ scheduledMessages });
    const processScheduledMessage = spacetime.reducer(
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );

    spacetime[moduleHooks]({ processScheduledMessage });

    expect(spacetime.moduleDef.schedules).toEqual([]);
  });

  it('rejects a schedule whose reducer is not exported', () => {
    const scheduledMessages = table(
      {},
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
      }
    );

    const spacetime = schema({ scheduledMessages });
    const processScheduledMessage = spacetime.reducer(
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );
    const schedules = spacetime.schedule(
      scheduledMessages,
      processScheduledMessage
    );

    expect(() => spacetime[moduleHooks]({ schedules })).toThrow(
      'Table scheduledMessages defines a schedule, but it seems like the associated function was not exported.'
    );
  });

  it('rejects a schedule whose table is not in the schema', () => {
    const scheduledMessages = table(
      {},
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
      }
    );
    const otherScheduledMessages = table(
      {},
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
      }
    );

    const spacetime = schema({ scheduledMessages });
    const processScheduledMessage = spacetime.reducer(
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );
    const schedules = spacetime.schedule(
      otherScheduledMessages,
      processScheduledMessage
    );

    expect(() =>
      spacetime[moduleHooks]({ processScheduledMessage, schedules })
    ).toThrow('Schedule target table is not part of this schema.');
  });

  it('rejects a schedule whose table has no ScheduleAt column', () => {
    const scheduledMessages = table(
      {},
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        text: t.string(),
      }
    );

    const spacetime = schema({ scheduledMessages });
    const processScheduledMessage = spacetime.reducer(
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );
    const schedules = spacetime.schedule(
      scheduledMessages,
      processScheduledMessage
    );

    expect(() =>
      spacetime[moduleHooks]({ processScheduledMessage, schedules })
    ).toThrow(
      'Table scheduledMessages defines a schedule, but it does not have a ScheduleAt column.'
    );
  });
});
