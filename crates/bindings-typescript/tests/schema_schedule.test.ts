import { describe, expect, it, vi } from 'vitest';
import { moduleHooks } from 'spacetime:sys@2.0';

const sysMock = vi.hoisted(() => ({
  moduleHooks: Symbol('moduleHooks'),
}));

vi.mock('spacetime:sys@2.0', () => ({
  moduleHooks: sysMock.moduleHooks,
}));

vi.mock('spacetime:sys@2.1', () => ({
  moduleHooks: sysMock.moduleHooks,
}));

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
  it('emits reducer schedules registered with onSchedule', () => {
    const scheduledMessages = table(
      { name: 'scheduled_messages' },
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
        text: t.string(),
      }
    );

    const spacetimedb = schema({ scheduledMessages });
    const processScheduledMessage = spacetimedb.reducer(
      { onSchedule: scheduledMessages },
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );

    spacetimedb[moduleHooks]({ processScheduledMessage });

    expect(spacetimedb.moduleDef.schedules).toEqual([
      {
        sourceName: undefined,
        tableName: 'scheduledMessages',
        scheduleAtCol: 1,
        functionName: 'processScheduledMessage',
      },
    ]);
  });

  it('emits procedure schedules registered with onSchedule', () => {
    const scheduledMessages = table(
      {},
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
        text: t.string(),
      }
    );

    const spacetimedb = schema({ scheduledMessages });
    const processScheduledMessage = spacetimedb.procedure(
      { onSchedule: scheduledMessages },
      { scheduledMessage: scheduledMessages.rowType },
      t.unit(),
      () => ({})
    );

    spacetimedb[moduleHooks]({ processScheduledMessage });

    expect(spacetimedb.moduleDef.schedules).toEqual([
      {
        sourceName: undefined,
        tableName: 'scheduledMessages',
        scheduleAtCol: 1,
        functionName: 'processScheduledMessage',
      },
    ]);
  });

  it('keeps legacy table scheduled option working', () => {
    const processScheduledMessageRef = { current: undefined as any };
    const scheduledMessages = table(
      {
        scheduled: () => processScheduledMessageRef.current,
      },
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
        text: t.string(),
      }
    );
    expect(scheduledMessages.schedule?.scheduleAtCol).toBe(1);
    const spacetimedb = schema({ scheduledMessages });
    processScheduledMessageRef.current = spacetimedb.reducer(
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );

    spacetimedb[moduleHooks]({
      processScheduledMessage: processScheduledMessageRef.current,
    });

    expect(spacetimedb.moduleDef.schedules).toEqual([
      {
        sourceName: undefined,
        tableName: 'scheduledMessages',
        scheduleAtCol: 1,
        functionName: 'processScheduledMessage',
      },
    ]);
  });

  it('keeps legacy scheduled duplicate table handles working', () => {
    const processScheduledMessageRef = { current: undefined as any };
    const scheduledMessages = table(
      {
        scheduled: () => processScheduledMessageRef.current,
      },
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
        text: t.string(),
      }
    );
    const spacetimedb = schema({
      first: scheduledMessages,
      second: scheduledMessages,
    });
    processScheduledMessageRef.current = spacetimedb.reducer(
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );

    spacetimedb[moduleHooks]({
      processScheduledMessage: processScheduledMessageRef.current,
    });

    expect(spacetimedb.moduleDef.schedules).toEqual([
      {
        sourceName: undefined,
        tableName: 'first',
        scheduleAtCol: 1,
        functionName: 'processScheduledMessage',
      },
      {
        sourceName: undefined,
        tableName: 'second',
        scheduleAtCol: 1,
        functionName: 'processScheduledMessage',
      },
    ]);
  });

  it('keeps legacy table scheduled option working for procedures', () => {
    const processScheduledMessageRef = { current: undefined as any };
    const scheduledMessages = table(
      {
        scheduled: () => processScheduledMessageRef.current,
      },
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
        text: t.string(),
      }
    );
    const spacetimedb = schema({ scheduledMessages });
    processScheduledMessageRef.current = spacetimedb.procedure(
      { scheduledMessage: scheduledMessages.rowType },
      t.unit(),
      () => ({})
    );

    spacetimedb[moduleHooks]({
      processScheduledMessage: processScheduledMessageRef.current,
    });

    expect(spacetimedb.moduleDef.schedules).toEqual([
      {
        sourceName: undefined,
        tableName: 'scheduledMessages',
        scheduleAtCol: 1,
        functionName: 'processScheduledMessage',
      },
    ]);
  });

  it('keeps legacy table scheduled option as a no-op without ScheduleAt', () => {
    const processScheduledMessageRef = { current: undefined as any };
    const scheduledMessages = table(
      {
        scheduled: () => processScheduledMessageRef.current,
      },
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        text: t.string(),
      }
    );
    const spacetimedb = schema({ scheduledMessages });
    processScheduledMessageRef.current = spacetimedb.reducer(
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );

    spacetimedb[moduleHooks]({
      processScheduledMessage: processScheduledMessageRef.current,
    });

    expect(spacetimedb.moduleDef.schedules).toEqual([]);
  });

  it('rejects a legacy scheduled function that was not exported', () => {
    const processScheduledMessageRef = { current: undefined as any };
    const scheduledMessages = table(
      {
        scheduled: () => processScheduledMessageRef.current,
      },
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
      }
    );
    const spacetimedb = schema({ scheduledMessages });
    processScheduledMessageRef.current = spacetimedb.reducer(
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );

    expect(() => spacetimedb[moduleHooks]({})).toThrow(
      'Table scheduledMessages defines a schedule, but it seems like the associated function was not exported.'
    );
  });

  it('rejects an onSchedule table that is not in the schema', () => {
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

    const spacetimedb = schema({ scheduledMessages });
    const processScheduledMessage = spacetimedb.reducer(
      { onSchedule: otherScheduledMessages },
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );

    expect(() => spacetimedb[moduleHooks]({ processScheduledMessage })).toThrow(
      'Schedule target table is not part of this schema.'
    );
  });

  it('rejects an onSchedule table with no ScheduleAt column', () => {
    const scheduledMessages = table(
      {},
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        text: t.string(),
      }
    );

    const spacetimedb = schema({ scheduledMessages });
    const processScheduledMessage = spacetimedb.reducer(
      { onSchedule: scheduledMessages },
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );

    expect(() => spacetimedb[moduleHooks]({ processScheduledMessage })).toThrow(
      'Table scheduledMessages defines a schedule, but it does not have a ScheduleAt column.'
    );
  });

  it('rejects multiple scheduled functions for the same table', () => {
    const scheduledMessages = table(
      {},
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
      }
    );

    const spacetimedb = schema({ scheduledMessages });
    const firstScheduledMessage = spacetimedb.reducer(
      { onSchedule: scheduledMessages },
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );
    const secondScheduledMessage = spacetimedb.reducer(
      { onSchedule: scheduledMessages },
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );

    expect(() =>
      spacetimedb[moduleHooks]({
        firstScheduledMessage,
        secondScheduledMessage,
      })
    ).toThrow(
      'Table scheduledMessages defines multiple schedules: firstScheduledMessage and secondScheduledMessage. A schedule table can only be used by one reducer or procedure.'
    );
  });

  it('rejects onSchedule targets registered under multiple schema keys', () => {
    const scheduledMessages = table(
      {},
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
      }
    );

    const spacetimedb = schema({
      first: scheduledMessages,
      second: scheduledMessages,
    });
    const processScheduledMessage = spacetimedb.reducer(
      { onSchedule: scheduledMessages },
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );

    expect(() => spacetimedb[moduleHooks]({ processScheduledMessage })).toThrow(
      'Schedule target table is registered more than once in this schema. Use a distinct table handle for each scheduled table.'
    );
  });

  it('rejects mixed legacy and onSchedule registrations for the same table', () => {
    const legacyScheduledMessageRef = { current: undefined as any };
    const scheduledMessages = table(
      {
        scheduled: () => legacyScheduledMessageRef.current,
      },
      {
        scheduledId: t.u64().primaryKey().autoInc(),
        scheduledAt: t.scheduleAt(),
      }
    );

    const spacetimedb = schema({ scheduledMessages });
    legacyScheduledMessageRef.current = spacetimedb.reducer(
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );
    const newScheduledMessage = spacetimedb.reducer(
      { onSchedule: scheduledMessages },
      { scheduledMessage: scheduledMessages.rowType },
      () => {}
    );

    expect(() =>
      spacetimedb[moduleHooks]({
        legacyScheduledMessage: legacyScheduledMessageRef.current,
        newScheduledMessage,
      })
    ).toThrow(
      'Table scheduledMessages defines multiple schedules: legacyScheduledMessage and newScheduledMessage. A schedule table can only be used by one reducer or procedure.'
    );
  });

  it('allows reducer params named onSchedule without treating them as options', () => {
    const messages = table(
      {},
      {
        id: t.u64().primaryKey(),
        text: t.string(),
      }
    );

    const spacetimedb = schema({ messages });
    const updateMessage = spacetimedb.reducer(
      { onSchedule: t.string() },
      () => {}
    );

    spacetimedb[moduleHooks]({ updateMessage });

    expect(spacetimedb.moduleDef.reducers).toEqual([
      expect.objectContaining({
        sourceName: 'updateMessage',
        params: {
          elements: [
            expect.objectContaining({
              name: 'onSchedule',
            }),
          ],
        },
      }),
    ]);
    expect(spacetimedb.moduleDef.schedules).toEqual([]);
  });

  it('allows procedure params named onSchedule without treating them as options', () => {
    const messages = table(
      {},
      {
        id: t.u64().primaryKey(),
        text: t.string(),
      }
    );

    const spacetimedb = schema({ messages });
    const getMessage = spacetimedb.procedure(
      { onSchedule: t.string() },
      t.unit(),
      () => ({})
    );

    spacetimedb[moduleHooks]({ getMessage });

    expect(spacetimedb.moduleDef.procedures).toEqual([
      expect.objectContaining({
        sourceName: 'getMessage',
        params: {
          elements: [
            expect.objectContaining({
              name: 'onSchedule',
            }),
          ],
        },
      }),
    ]);
    expect(spacetimedb.moduleDef.schedules).toEqual([]);
  });
});
