import { type Operation, TableCache } from '../src/sdk/table_cache';
import { describe, expect, test } from 'vitest';

import { Player } from '../test-app/src/module_bindings/player_type.ts';

import { AlgebraicType, type AlgebraicTypeVariants } from 'spacetimedb';

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
  tableCache: TableCache<any>;
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

function runTest(tableCache: TableCache<any>, testSteps: TestStep[]) {
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
    const pointType = AlgebraicType.Product({
      elements: [
        { name: 'x', algebraicType: AlgebraicType.U16 },
        { name: 'y', algebraicType: AlgebraicType.U16 },
      ],
    });
    const playerType = AlgebraicType.Product({
      elements: [
        { name: 'ownerId', algebraicType: AlgebraicType.String },
        { name: 'name', algebraicType: AlgebraicType.String },
        { name: 'location', algebraicType: pointType },
      ],
    });
    const tableTypeInfo: TableRuntimeTypeInfo = {
      tableName: 'player',
      rowType: playerType,
    };
    const newTable = () => new TableCache<Player>(tableTypeInfo);
    const mkOperation = (type: 'insert' | 'delete', row: Player) => {
      const rowId = AlgebraicType.intoMapKey(tableTypeInfo.rowType, row);
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
        ownerId: '1',
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter().length).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
            expect(callbackHistory.length).toBe(1);
            expect(callbackHistory[0].type).toBe('insert');
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
        ownerId: '1',
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter().length).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
            expect(callbackHistory.length).toBe(1);
            expect(callbackHistory[0].type).toBe('insert');
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
        ownerId: '1',
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter().length).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
            expect(callbackHistory.length).toBe(1);
            expect(callbackHistory[0].type).toBe('insert');
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter().length).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
            expect(callbackHistory.length).toBe(0);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Insert once and delete', () => {
      const tableCache = newTable();
      const steps: TestStep[] = [];
      const player = {
        ownerId: '1',
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
            expect(callbackHistory.length).toBe(1);
            expect(callbackHistory[0].type).toBe('insert');
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
            expect(tableCache.count()).toBe(0);
            expect(callbackHistory.length).toBe(1);
            expect(callbackHistory[0].type).toBe('delete');
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
        ownerId: '1',
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      });
      const player = {
        ownerId: '1',
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
            expect(callbackHistory.length).toBe(1);
            expect(callbackHistory[0].type).toBe('insert');
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
            expect(callbackHistory.length).toBe(0);
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
            expect(tableCache.count()).toBe(0);
            expect(callbackHistory.length).toBe(1);
            expect(callbackHistory[0].type).toBe('delete');
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
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
            expect(tableCache.count()).toBe(0);
            expect(callbackHistory).toEqual([deleteEvent(mkPlayer())]);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Insert one', () => {
      const tableCache = newTable();
      const op = mkOperation('insert', {
        ownerId: '1',
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
      expect(callbacks.length).toBe(1);
      expect(tableCache.count()).toBe(1);
      callbacks.forEach(cb => {
        cb.cb();
      });
      expect(rowsInserted).toBe(1);
    });
  });
  describe('Indexed player table', () => {
    const pointType = AlgebraicType.Product({
      elements: [
        { name: 'x', algebraicType: AlgebraicType.U16 },
        { name: 'y', algebraicType: AlgebraicType.U16 },
      ],
    });
    const playerType: AlgebraicType = AlgebraicType.Product({
      elements: [
        { name: 'ownerId', algebraicType: AlgebraicType.String },
        { name: 'name', algebraicType: AlgebraicType.String },
        { name: 'location', algebraicType: pointType },
      ],
    });
    const tableTypeInfo: TableRuntimeTypeInfo = {
      tableName: 'player',
      rowType: playerType,
      primaryKeyInfo: {
        colName: 'ownerId',
        colType: (playerType as AlgebraicTypeVariants.Product).value.elements[0]
          .algebraicType,
      },
    };
    const newTable = () => new TableCache<Player>(tableTypeInfo);
    const mkOperation = (type: 'insert' | 'delete', row: Player) => {
      const rowId = AlgebraicType.intoMapKey(
        tableTypeInfo.primaryKeyInfo!.colType,
        row['ownerId']
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
        ownerId: '1',
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter().length).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
            expect(callbackHistory.length).toBe(1);
            expect(callbackHistory[0].type).toBe('insert');
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
        ownerId: '1',
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter().length).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
            expect(callbackHistory.length).toBe(1);
            expect(callbackHistory[0].type).toBe('insert');
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
        ownerId: '1',
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter().length).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
            expect(callbackHistory.length).toBe(1);
            expect(callbackHistory[0].type).toBe('insert');
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter().length).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
            expect(callbackHistory.length).toBe(0);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Insert once and delete', () => {
      const tableCache = newTable();
      const steps: TestStep[] = [];
      const player = {
        ownerId: '1',
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
            expect(callbackHistory.length).toBe(1);
            expect(callbackHistory[0].type).toBe('insert');
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
            expect(tableCache.count()).toBe(0);
            expect(callbackHistory.length).toBe(1);
            expect(callbackHistory[0].type).toBe('delete');
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
        ownerId: '1',
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter()[0]).toEqual(mkPlayer('jeff'));
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter()[0]).toEqual(mkPlayer('jeffv2'));
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
        ownerId: '1',
        name: 'Player 1',
        location: {
          x: 1,
          y: 2,
        },
      });
      const player = {
        ownerId: '1',
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
            expect(callbackHistory.length).toBe(1);
            expect(callbackHistory[0].type).toBe('insert');
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
            expect(callbackHistory.length).toBe(0);
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
            expect(tableCache.count()).toBe(0);
            expect(callbackHistory.length).toBe(1);
            expect(callbackHistory[0].type).toBe('delete');
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
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
            expect(tableCache.count()).toBe(1);
            expect(tableCache.iter()[0]).toEqual(player);
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
            expect(tableCache.count()).toBe(0);
            expect(callbackHistory).toEqual([deleteEvent(mkPlayer())]);
          },
        ],
      });
      runTest(tableCache, steps);
    });

    test('Insert one', () => {
      const tableCache = newTable();
      const op = mkOperation('insert', {
        ownerId: '1',
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
      expect(callbacks.length).toBe(1);
      expect(tableCache.count()).toBe(1);
      callbacks.forEach(cb => {
        cb.cb();
      });
      expect(rowsInserted).toBe(1);
    });
  });

  const pointType = AlgebraicType.Product({
    elements: [
      { name: 'x', algebraicType: AlgebraicType.U16 },
      { name: 'y', algebraicType: AlgebraicType.U16 },
    ],
  });
  const playerType = AlgebraicType.Product({
    elements: [
      { name: 'ownerId', algebraicType: AlgebraicType.String },
      { name: 'name', algebraicType: AlgebraicType.String },
      { name: 'location', algebraicType: pointType },
    ],
  });

  test('should be empty on creation', () => {
    const tableTypeInfo: TableRuntimeTypeInfo = {
      tableName: 'player',
      rowType: playerType,
      primaryKeyInfo: {
        colName: 'ownerId',
        colType: (playerType as AlgebraicTypeVariants.Product).value.elements[0]
          .algebraicType,
      },
    };
    const tableCache = new TableCache<Player>(tableTypeInfo);
    expect(tableCache.count()).toBe(0);
  });
});
