import { type Operation, TableCacheImpl } from '../src/sdk/table_cache';
import { describe, expect, test } from 'vitest';
import Player from '../test-app/src/module_bindings/player_type.ts';
import { AlgebraicType, Identity, type Infer } from '../src';
import {
  tables,
  UnindexedPlayer,
} from '../test-app/src/module_bindings/index.ts';

interface ApplyOperations {
  ops: Operation[];
  ctx: any;
}

interface CallbackEvent {
  type: 'insert' | 'delete' | 'update';
  ctx: any;
  row: any;
  oldRow?: any; // Only there for updates.
}

function insertEvent(row: any, ctx: any = {}): CallbackEvent {
  return {
    type: 'insert',
    ctx,
    row,
  };
}

function updateEvent(oldRow: any, row: any, ctx: any = {}): CallbackEvent {
  return {
    type: 'update',
    ctx,
    row,
    oldRow,
  };
}

function deleteEvent(row: any, ctx: any = {}): CallbackEvent {
  return {
    type: 'delete',
    ctx,
    row,
  };
}

interface AssertionInput {
  // The state of the table cache.
  tableCache: TableCacheImpl<any, string>;
  // The sequence of callbacks that were fired from the last applyOperations.
  callbackHistory: CallbackEvent[];
}

type Assertion = (arg0: AssertionInput) => void;

interface TestStep {
  // The operations to apply.
  ops: ApplyOperations;
  // The assertions to make after applying the operations.
  assertions: Assertion[];
}

function runTest(
  tableCache: TableCacheImpl<any, string>,
  testSteps: TestStep[]
) {
  const callbackHistory: CallbackEvent[] = [];
  tableCache.onInsert((ctx, row) => {
    callbackHistory.push({
      type: 'insert',
      ctx,
      row,
    });
  });
  tableCache.onDelete((ctx, row) => {
    callbackHistory.push({
      type: 'delete',
      ctx,
      row,
    });
  });
  tableCache.onUpdate((ctx, oldRow, row) => {
    callbackHistory.push({
      type: 'update',
      ctx,
      row,
      oldRow,
    });
  });

  for (const step of testSteps) {
    const { ops: applyOperations, assertions } = step;
    const { ops, ctx } = applyOperations;
    const callbacks = tableCache.applyOperations(ops, ctx);
    callbacks.forEach(cb => cb.cb());
    for (const assertion of assertions) {
      assertion({ tableCache, callbackHistory });
    }
    // Clear the callback history for the next step.
    callbackHistory.length = 0;
  }
}

describe('TableCache', () => {
  describe('Unindexed player table', () => {
    const newTable = () => new TableCacheImpl(tables.unindexedPlayer);
    const mkOperation = (
      type: 'insert' | 'delete',
      row: Infer<typeof UnindexedPlayer>
    ) => {
      const rowId = AlgebraicType.intoMapKey(
        { tag: 'Product', value: tables.unindexedPlayer.rowType },
        row
      );
      return {
        type,
        rowId,
        row,
      };
    };

    test('Insert one', () => {
      const tableCache = newTable();
      const steps: TestStep[] = [];
      const player = {
        id: 1,
        ownerId: Identity.zero(),
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      };
      steps.push({
        ops: {
          ops: [mkOperation('insert', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter()).length).toEqual(1);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory.length).toEqual(1);
            expect(callbackHistory[0].type).toEqual('insert');
            expect(callbackHistory[0].row).toEqual(player);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Inserting one twice only triggers one event', () => {
      const tableCache = newTable();
      const steps: TestStep[] = [];
      const player = {
        id: 1,
        ownerId: Identity.zero(),
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      };
      steps.push({
        ops: {
          ops: [mkOperation('insert', player), mkOperation('insert', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter()).length).toEqual(1);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory.length).toEqual(1);
            expect(callbackHistory[0].type).toEqual('insert');
            expect(callbackHistory[0].row).toEqual(player);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Insert dupe is a noop', () => {
      const tableCache = newTable();
      const steps: TestStep[] = [];
      const player = {
        id: 1,
        ownerId: Identity.zero(),
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      };
      steps.push({
        ops: {
          ops: [mkOperation('insert', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter()).length).toEqual(1);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory.length).toEqual(1);
            expect(callbackHistory[0].type).toEqual('insert');
            expect(callbackHistory[0].row).toEqual(player);
          },
        ],
      });
      steps.push({
        ops: {
          ops: [mkOperation('insert', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter()).length).toEqual(1);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory.length).toEqual(0);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Insert once and delete', () => {
      const tableCache = newTable();
      const steps: TestStep[] = [];
      const player = {
        id: 1,
        ownerId: Identity.zero(),
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      };
      steps.push({
        ops: {
          ops: [mkOperation('insert', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory.length).toEqual(1);
            expect(callbackHistory[0].type).toEqual('insert');
            expect(callbackHistory[0].row).toEqual(player);
          },
        ],
      });
      steps.push({
        ops: {
          ops: [mkOperation('delete', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(0n);
            expect(callbackHistory.length).toEqual(1);
            expect(callbackHistory[0].type).toEqual('delete');
            expect(callbackHistory[0].row).toEqual(player);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Insert twice and delete', () => {
      const tableCache = newTable();
      const steps: TestStep[] = [];
      const mkPlayer = () => ({
        id: 1,
        ownerId: Identity.zero(),
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      });
      const player = {
        id: 1,
        ownerId: Identity.zero(),
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      };
      steps.push({
        ops: {
          ops: [
            mkOperation('insert', mkPlayer()),
            mkOperation('insert', player),
          ],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory.length).toEqual(1);
            expect(callbackHistory[0].type).toEqual('insert');
            expect(callbackHistory[0].row).toEqual(player);
          },
        ],
      });
      steps.push({
        ops: {
          ops: [mkOperation('delete', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            // We still have one reference left, so it isn't actually deleted.
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory.length).toEqual(0);
          },
        ],
      });
      steps.push({
        ops: {
          ops: [mkOperation('delete', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            // Now it is actually deleted.
            expect(tableCache.count()).toEqual(0n);
            expect(callbackHistory.length).toEqual(1);
            expect(callbackHistory[0].type).toEqual('delete');
            expect(callbackHistory[0].row).toEqual(player);
          },
        ],
      });
      // Now we are going to insert again, so we can delete both refs at once.
      steps.push({
        ops: {
          ops: [mkOperation('insert', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory).toEqual([insertEvent(player)]);
          },
        ],
      });
      steps.push({
        ops: {
          ops: [mkOperation('insert', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory).toEqual([]);
          },
        ],
      });
      steps.push({
        ops: {
          ops: [mkOperation('delete', player), mkOperation('delete', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(0n);
            expect(callbackHistory).toEqual([deleteEvent(mkPlayer())]);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Insert one', () => {
      const tableCache = newTable();
      const op = mkOperation('insert', {
        id: 1,
        ownerId: Identity.zero(),
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      });
      let rowsInserted = 0;
      const callbacks = tableCache.applyOperations([op], {} as any);
      tableCache.onInsert((ctx, row) => {
        rowsInserted++;
        expect(row).toEqual(op.row);
      });
      expect(callbacks.length).toEqual(1);
      expect(tableCache.count()).toEqual(1n);
      callbacks.forEach(cb => {
        cb.cb();
      });
      expect(rowsInserted).toEqual(1);
    });
  });
  describe('Indexed player table', () => {
    const newTable = () => new TableCacheImpl(tables.player);
    const mkOperation = (
      type: 'insert' | 'delete',
      row: Infer<typeof Player>
    ) => {
      const rowId = AlgebraicType.intoMapKey(
        Player.elements['id'].algebraicType,
        row['id']
      );
      return {
        type,
        rowId,
        row,
      };
    };

    test('Insert one', () => {
      const tableCache = newTable();
      const steps: TestStep[] = [];
      const player = {
        id: 1,
        userId: Identity.zero(),
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      };
      steps.push({
        ops: {
          ops: [mkOperation('insert', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter()).length).toEqual(1);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory.length).toEqual(1);
            expect(callbackHistory[0].type).toEqual('insert');
            expect(callbackHistory[0].row).toEqual(player);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Inserting one twice only triggers one event', () => {
      const tableCache = newTable();
      const steps: TestStep[] = [];
      const player = {
        id: 1,
        userId: Identity.zero(),
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      };
      steps.push({
        ops: {
          ops: [mkOperation('insert', player), mkOperation('insert', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter()).length).toEqual(1);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory.length).toEqual(1);
            expect(callbackHistory[0].type).toEqual('insert');
            expect(callbackHistory[0].row).toEqual(player);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Insert dupe is a noop', () => {
      const tableCache = newTable();
      const steps: TestStep[] = [];
      const player = {
        id: 1,
        userId: Identity.zero(),
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      };
      steps.push({
        ops: {
          ops: [mkOperation('insert', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter()).length).toEqual(1);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory.length).toEqual(1);
            expect(callbackHistory[0].type).toEqual('insert');
            expect(callbackHistory[0].row).toEqual(player);
          },
        ],
      });
      steps.push({
        ops: {
          ops: [mkOperation('insert', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter()).length).toEqual(1);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory.length).toEqual(0);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Insert once and delete', () => {
      const tableCache = newTable();
      const steps: TestStep[] = [];
      const player = {
        id: 1,
        userId: Identity.zero(),
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      };
      steps.push({
        ops: {
          ops: [mkOperation('insert', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory.length).toEqual(1);
            expect(callbackHistory[0].type).toEqual('insert');
            expect(callbackHistory[0].row).toEqual(player);
          },
        ],
      });
      steps.push({
        ops: {
          ops: [mkOperation('delete', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(0n);
            expect(callbackHistory.length).toEqual(1);
            expect(callbackHistory[0].type).toEqual('delete');
            expect(callbackHistory[0].row).toEqual(player);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Update smoke test', () => {
      const tableCache = newTable();
      const steps: TestStep[] = [];
      const mkPlayer = (name: string) => ({
        id: 1,
        userId: Identity.zero(),
        name: name,
        location: {
          x: 1,
          y: 2,
        },
      });
      steps.push({
        ops: {
          ops: [mkOperation('insert', mkPlayer('jeff'))],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter())[0]).toEqual(mkPlayer('jeff'));
            expect(callbackHistory).toEqual([insertEvent(mkPlayer('jeff'))]);
          },
        ],
      });
      steps.push({
        ops: {
          ops: [
            mkOperation('delete', mkPlayer('jeff')),
            mkOperation('insert', mkPlayer('jeffv2')),
          ],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter())[0]).toEqual(
              mkPlayer('jeffv2')
            );
            expect(callbackHistory).toEqual([
              updateEvent(mkPlayer('jeff'), mkPlayer('jeffv2')),
            ]);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Insert twice and delete', () => {
      const tableCache = newTable();
      const steps: TestStep[] = [];
      const mkPlayer = () => ({
        id: 1,
        userId: Identity.zero(),
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      });
      const player = {
        id: 1,
        userId: Identity.zero(),
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      };
      steps.push({
        ops: {
          ops: [
            mkOperation('insert', mkPlayer()),
            mkOperation('insert', player),
          ],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory.length).toEqual(1);
            expect(callbackHistory[0].type).toEqual('insert');
            expect(callbackHistory[0].row).toEqual(player);
          },
        ],
      });
      steps.push({
        ops: {
          ops: [mkOperation('delete', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            // We still have one reference left, so it isn't actually deleted.
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory.length).toEqual(0);
          },
        ],
      });
      steps.push({
        ops: {
          ops: [mkOperation('delete', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            // Now it is actually deleted.
            expect(tableCache.count()).toEqual(0n);
            expect(callbackHistory.length).toEqual(1);
            expect(callbackHistory[0].type).toEqual('delete');
            expect(callbackHistory[0].row).toEqual(player);
          },
        ],
      });
      // Now we are going to insert again, so we can delete both refs at once.
      steps.push({
        ops: {
          ops: [mkOperation('insert', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory).toEqual([insertEvent(player)]);
          },
        ],
      });
      steps.push({
        ops: {
          ops: [mkOperation('insert', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(1n);
            expect(Array.from(tableCache.iter())[0]).toEqual(player);
            expect(callbackHistory).toEqual([]);
          },
        ],
      });
      steps.push({
        ops: {
          ops: [mkOperation('delete', player), mkOperation('delete', player)],
          ctx: {} as any,
        },
        assertions: [
          ({ tableCache, callbackHistory }) => {
            expect(tableCache.count()).toEqual(0n);
            expect(callbackHistory).toEqual([deleteEvent(mkPlayer())]);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Insert one', () => {
      const tableCache = newTable();
      const op = mkOperation('insert', {
        id: 1,
        userId: Identity.zero(),
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      });
      let rowsInserted = 0;
      const callbacks = tableCache.applyOperations([op], {} as any);
      tableCache.onInsert((ctx, row) => {
        rowsInserted++;
        expect(row).toEqual(op.row);
      });
      expect(callbacks.length).toEqual(1);
      expect(tableCache.count()).toEqual(1n);
      callbacks.forEach(cb => {
        cb.cb();
      });
      expect(rowsInserted).toEqual(1);
    });
  });

  test('should be empty on creation', () => {
    const tableCache = new TableCacheImpl(tables.player);
    expect(tableCache.count()).toEqual(0n);
  });
});
