// Grid helper checks against an in-memory table fake.
import assert from 'node:assert/strict';
import { Timestamp } from 'spacetimedb';
import {
  createGrid,
  deleteGrid,
  moveEntity,
  placeEntity,
  setCellCost,
  computePath,
} from '../src/procedures.ts';

function fakeTable() {
  const rows = new Map();
  let nextId = 1n;
  const byId = {
    find: id => rows.get(id),
    update: row => rows.set(row.id, row),
  };
  return {
    rows,
    id: byId,
    gridId: {
      filter: gridId => [...rows.values()].filter(r => r.gridId === gridId),
    },
    byGridCell: {
      filter: ([gridId, x, y]) =>
        [...rows.values()].filter(
          r => r.gridId === gridId && r.x === x && r.y === y
        ),
    },
    insert: row => {
      const inserted = { ...row, id: nextId++ };
      rows.set(inserted.id, inserted);
      return inserted;
    },
    delete: row => rows.delete(row.id),
  };
}

function fakeTx() {
  return {
    timestamp: new Timestamp(1n),
    db: {
      grid: fakeTable(),
      cellState: fakeTable(),
      gridEntity: fakeTable(),
      entityPath: fakeTable(),
    },
  };
}

function throwsCode(fn, code) {
  assert.throws(fn, error => error.message === code);
}

const gridArgs = {
  name: 'Arena',
  kind: 'square',
  orientation: 'flat',
  width: 5,
  height: 5,
  defaultCost: 1,
  connectivity: 4,
  mode: 'collaborative',
};

{
  const tx = fakeTx();
  const gridId = createGrid(tx, gridArgs, 'alice');
  throwsCode(() => deleteGrid(tx, { gridId }, 'bob'), 'grid.not_owner');
  deleteGrid(tx, { gridId }, 'alice');
  assert.equal(tx.db.grid.rows.size, 0);
}

{
  const tx = fakeTx();
  const gridId = createGrid(tx, gridArgs, 'alice');
  const mover = placeEntity(
    tx,
    {
      gridId,
      x: 1,
      y: 1,
      kind: 'unit',
      blocksMovement: true,
      label: undefined,
    },
    'alice'
  );
  setCellCost(tx, { gridId, x: 2, y: 1, cost: 0, terrain: 'rock' }, 'alice');
  throwsCode(
    () => moveEntity(tx, { entityId: mover, toX: 2, toY: 1 }, 'alice'),
    'grid.move_blocked'
  );
  // A painted cell under a blocking entity still blocks.
  setCellCost(tx, { gridId, x: 1, y: 2, cost: 3, terrain: 'mud' }, 'bob');
  placeEntity(
    tx,
    {
      gridId,
      x: 1,
      y: 2,
      kind: 'wall',
      blocksMovement: true,
      label: undefined,
    },
    'bob'
  );
  throwsCode(
    () => moveEntity(tx, { entityId: mover, toX: 1, toY: 2 }, 'alice'),
    'grid.move_blocked'
  );
  const path = computePath(
    tx,
    {
      gridId,
      startX: 1,
      startY: 1,
      endX: 1,
      endY: 2,
      storeFor: undefined,
      maxExpansions: undefined,
    },
    'alice'
  );
  assert.equal(path.found, false);
  moveEntity(tx, { entityId: mover, toX: 1, toY: 0 }, 'alice');
  assert.equal(tx.db.gridEntity.id.find(mover).y, 0);
  throwsCode(
    () =>
      setCellCost(
        tx,
        { gridId, x: 0, y: 0, cost: 10_001, terrain: undefined },
        'alice'
      ),
    'grid.invalid_cost'
  );
}

process.stdout.write('grid procedure tests passed\n');
