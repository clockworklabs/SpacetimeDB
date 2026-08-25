// Owner is passed explicitly so the submodule is identity-scheme-agnostic.
import type { Timestamp } from 'spacetimedb';
import {
  t,
  SenderError,
  type Infer,
  type InferTypeOfParams,
} from 'spacetimedb/server';
import {
  gridRow,
  pathResult,
  reachableCell,
  GRID_KIND_SQUARE,
  GRID_KIND_HEX,
  GRID_ORIENTATION_FLAT,
  GRID_ORIENTATION_POINTY,
  GRID_MODE_OWNER,
  GRID_MODE_COLLABORATIVE,
} from './rows.ts';
import {
  type Coord,
  type GridKind,
  type Connectivity,
  coordKey,
  neighbors,
  distance,
  findPathAstar,
  dijkstra,
} from './math/index.ts';
import type {
  ProcedureModuleCtx,
  TransactionModuleCtx,
} from './submodule/schema.ts';

type GridRow = Infer<typeof gridRow>;

const VALID_KINDS = new Set([GRID_KIND_SQUARE, GRID_KIND_HEX]);
const VALID_ORIENTATIONS = new Set([
  GRID_ORIENTATION_FLAT,
  GRID_ORIENTATION_POINTY,
]);
const VALID_MODES = new Set([GRID_MODE_OWNER, GRID_MODE_COLLABORATIVE]);
const VALID_CONNECTIVITY = new Set([4, 8]);

const GRID_MAX_DIM = 1024;
const PATH_DEFAULT_MAX_EXPANSIONS = 50_000;
const PATH_MAX_EXPANSIONS = 50_000;
const MAX_RESULT_CELLS = 5_000;
const MAX_PAINT_CELLS = 1_000;
const MAX_NAME_LENGTH = 128;
const MAX_KIND_LENGTH = 64;
const MAX_LABEL_LENGTH = 256;
const MAX_TERRAIN_LENGTH = 64;

export const createGridParams = {
  name: t.string(),
  kind: t.string(),
  orientation: t.string(),
  width: t.i32(),
  height: t.i32(),
  defaultCost: t.i32(),
  connectivity: t.i32(),
  mode: t.string(),
};

export function createGridImpl(
  ctx: ProcedureModuleCtx,
  args: InferTypeOfParams<typeof createGridParams>,
  owner: string
): bigint {
  if (
    typeof args.name !== 'string' ||
    args.name.length === 0 ||
    args.name.length > MAX_NAME_LENGTH
  ) {
    throw new SenderError('grid.invalid_name');
  }
  if (!VALID_KINDS.has(args.kind)) {
    throw new SenderError(`grid.invalid_kind:${args.kind}`);
  }
  if (!VALID_ORIENTATIONS.has(args.orientation)) {
    throw new SenderError(`grid.invalid_orientation:${args.orientation}`);
  }
  if (!VALID_MODES.has(args.mode)) {
    throw new SenderError(`grid.invalid_mode:${args.mode}`);
  }
  if (
    args.width < 1 ||
    args.width > GRID_MAX_DIM ||
    args.height < 1 ||
    args.height > GRID_MAX_DIM
  ) {
    throw new SenderError(
      `grid.invalid_dimensions:${args.width}x${args.height}`
    );
  }
  if (args.defaultCost < 1) {
    throw new SenderError(`grid.invalid_default_cost:${args.defaultCost}`);
  }
  if (
    args.kind === GRID_KIND_SQUARE &&
    !VALID_CONNECTIVITY.has(args.connectivity)
  ) {
    throw new SenderError(`grid.invalid_connectivity:${args.connectivity}`);
  }
  return ctx.withTx(tx => {
    const row = tx.db.grid.insert({
      id: 0n,
      ownerUserId: owner,
      name: args.name,
      kind: args.kind,
      orientation: args.orientation,
      width: args.width,
      height: args.height,
      defaultCost: args.defaultCost,
      connectivity: args.kind === GRID_KIND_HEX ? 6 : args.connectivity,
      mode: args.mode,
      createdAt: ctx.timestamp,
      updatedAt: ctx.timestamp,
    });
    return row.id;
  });
}

// delete_grid cascades cell_state, grid_entity, entity_path.
export const deleteGridParams = {
  gridId: t.u64(),
};

export function deleteGridImpl(
  ctx: ProcedureModuleCtx,
  args: InferTypeOfParams<typeof deleteGridParams>,
  owner: string
): void {
  ctx.withTx(tx => {
    const grid = requireGridForMutation(tx, args.gridId, owner);
    for (const c of [...tx.db.cellState.gridId.filter(grid.id)])
      tx.db.cellState.delete(c);
    for (const e of [...tx.db.gridEntity.gridId.filter(grid.id)])
      tx.db.gridEntity.delete(e);
    for (const p of [...tx.db.entityPath.gridId.filter(grid.id)])
      tx.db.entityPath.delete(p);
    tx.db.grid.delete(grid);
  });
}

// cost<=0 blocks, cost==defaultCost removes the sparse row.
export const setCellCostParams = {
  gridId: t.u64(),
  x: t.i32(),
  y: t.i32(),
  cost: t.i32(),
  terrain: t.option(t.string()),
};

export function setCellCostImpl(
  ctx: ProcedureModuleCtx,
  args: InferTypeOfParams<typeof setCellCostParams>,
  owner: string
): void {
  if ((args.terrain?.length ?? 0) > MAX_TERRAIN_LENGTH) {
    throw new SenderError('grid.invalid_terrain');
  }
  ctx.withTx(tx => {
    const grid = requireGridForMutation(tx, args.gridId, owner);
    assertInBounds(grid, { x: args.x, y: args.y });
    upsertCellState(tx, grid, args.x, args.y, args.cost, args.terrain);
  });
}

export const paintCellsParams = {
  gridId: t.u64(),
  cells: t.array(
    t.object('PaintCell', {
      x: t.i32(),
      y: t.i32(),
      cost: t.i32(),
      terrain: t.option(t.string()),
    })
  ),
};

export function paintCellsImpl(
  ctx: ProcedureModuleCtx,
  args: InferTypeOfParams<typeof paintCellsParams>,
  owner: string
): void {
  if (!Array.isArray(args.cells) || args.cells.length > MAX_PAINT_CELLS) {
    throw new SenderError('grid.too_many_cells');
  }
  ctx.withTx(tx => {
    const grid = requireGridForMutation(tx, args.gridId, owner);
    for (const c of args.cells) {
      if ((c.terrain?.length ?? 0) > MAX_TERRAIN_LENGTH) {
        throw new SenderError('grid.invalid_terrain');
      }
      assertInBounds(grid, { x: c.x, y: c.y });
      upsertCellState(tx, grid, c.x, c.y, c.cost, c.terrain);
    }
  });
}

export const placeEntityParams = {
  gridId: t.u64(),
  x: t.i32(),
  y: t.i32(),
  kind: t.string(),
  blocksMovement: t.bool(),
  label: t.option(t.string()),
};

export function placeEntityImpl(
  ctx: ProcedureModuleCtx,
  args: InferTypeOfParams<typeof placeEntityParams>,
  owner: string
): bigint {
  if (
    typeof args.kind !== 'string' ||
    args.kind.length === 0 ||
    args.kind.length > MAX_KIND_LENGTH
  ) {
    throw new SenderError('grid.invalid_entity_kind');
  }
  if ((args.label?.length ?? 0) > MAX_LABEL_LENGTH) {
    throw new SenderError('grid.invalid_entity_label');
  }
  return ctx.withTx(tx => {
    const grid = requireGridForMutation(tx, args.gridId, owner);
    assertInBounds(grid, { x: args.x, y: args.y });
    const row = tx.db.gridEntity.insert({
      id: 0n,
      gridId: grid.id,
      ownerUserId: owner,
      x: args.x,
      y: args.y,
      kind: args.kind,
      blocksMovement: args.blocksMovement,
      label: args.label,
      createdAt: ctx.timestamp,
      updatedAt: ctx.timestamp,
    });
    return row.id;
  });
}

export const moveEntityParams = {
  entityId: t.u64(),
  toX: t.i32(),
  toY: t.i32(),
};

export function moveEntityImpl(
  ctx: ProcedureModuleCtx,
  args: InferTypeOfParams<typeof moveEntityParams>,
  owner: string
): void {
  ctx.withTx(tx => {
    const ent = tx.db.gridEntity.id.find(args.entityId);
    if (!ent) throw new SenderError(`grid.entity_not_found:${args.entityId}`);
    if (ent.ownerUserId !== owner)
      throw new SenderError(`grid.entity_not_owner:${args.entityId}`);
    const grid = tx.db.grid.id.find(ent.gridId);
    if (!grid) throw new SenderError(`grid.not_found:${ent.gridId}`);
    assertInBounds(grid, { x: args.toX, y: args.toY });

    const neighborSet = neighbors(
      grid.kind as GridKind,
      { x: ent.x, y: ent.y },
      grid.connectivity as Connectivity
    );
    const reachable = neighborSet.some(
      n => n.x === args.toX && n.y === args.toY
    );
    if (!reachable) throw new SenderError(`grid.move_not_adjacent`);

    tx.db.gridEntity.id.update({
      ...ent,
      x: args.toX,
      y: args.toY,
      updatedAt: ctx.timestamp,
    });
  });
}

// A* over the current cost map; optionally writes entity_path.
export const computePathParams = {
  gridId: t.u64(),
  startX: t.i32(),
  startY: t.i32(),
  endX: t.i32(),
  endY: t.i32(),
  storeFor: t.option(t.u64()),
  maxExpansions: t.option(t.i32()),
};

export const computePathReturn = pathResult;

export function computePathImpl(
  ctx: ProcedureModuleCtx,
  args: InferTypeOfParams<typeof computePathParams>,
  owner: string
) {
  return ctx.withTx(tx => {
    const grid = requireGridForAccess(tx, args.gridId, owner);
    assertInBounds(grid, { x: args.startX, y: args.startY });
    assertInBounds(grid, { x: args.endX, y: args.endY });

    const costMap = buildCostMap(tx, grid);
    const requestedCap = Number(
      args.maxExpansions ?? PATH_DEFAULT_MAX_EXPANSIONS
    );
    if (
      !Number.isInteger(requestedCap) ||
      requestedCap < 1 ||
      requestedCap > PATH_MAX_EXPANSIONS
    ) {
      throw new SenderError('grid.invalid_max_expansions');
    }
    const cap = requestedCap;

    const neighborsFn = (c: Coord) =>
      filterInBounds(
        grid,
        neighbors(grid.kind as GridKind, c, grid.connectivity as Connectivity)
      );

    const costFn = (c: Coord) => {
      const v = costMap.get(coordKey(c));
      return v === undefined ? grid.defaultCost : v;
    };

    const result = findPathAstar({
      start: { x: args.startX, y: args.startY },
      goal: { x: args.endX, y: args.endY },
      cost: costFn,
      neighbors: neighborsFn,
      heuristic: (a, b) =>
        distance(
          grid.kind as GridKind,
          a,
          b,
          grid.connectivity as Connectivity
        ),
      maxExpansions: cap,
    });

    if (result.found && args.storeFor !== undefined && args.storeFor !== null) {
      const entity = tx.db.gridEntity.id.find(args.storeFor);
      if (!entity)
        throw new SenderError(`grid.entity_not_found:${args.storeFor}`);
      if (entity.gridId !== grid.id)
        throw new SenderError('grid.entity_grid_mismatch');
      if (entity.ownerUserId !== owner)
        throw new SenderError(`grid.entity_not_owner:${args.storeFor}`);
      if (result.cells.length > MAX_RESULT_CELLS)
        throw new SenderError('grid.path_too_long');
      writeEntityPath(
        tx,
        ctx.timestamp,
        args.storeFor,
        grid.id,
        result.cells,
        result.cost
      );
    }

    if (result.found) {
      if (result.cells.length > MAX_RESULT_CELLS)
        throw new SenderError('grid.path_too_long');
      return {
        found: true,
        cells: result.cells,
        cost: result.cost,
        expanded: result.expanded,
      };
    }
    return { found: false, cells: [], cost: 0, expanded: result.expanded };
  });
}

export const cellsInRangeParams = {
  gridId: t.u64(),
  originX: t.i32(),
  originY: t.i32(),
  maxCost: t.i32(),
};

export const cellsInRangeReturn = t.object('CellsInRangeResult', {
  cells: t.array(reachableCell),
});

export function cellsInRangeImpl(
  ctx: ProcedureModuleCtx,
  args: InferTypeOfParams<typeof cellsInRangeParams>,
  owner: string
) {
  return ctx.withTx(tx => {
    const grid = requireGridForAccess(tx, args.gridId, owner);
    assertInBounds(grid, { x: args.originX, y: args.originY });

    const costMap = buildCostMap(tx, grid);
    const neighborsFn = (c: Coord) =>
      filterInBounds(
        grid,
        neighbors(grid.kind as GridKind, c, grid.connectivity as Connectivity)
      );
    const costFn = (c: Coord) => {
      const v = costMap.get(coordKey(c));
      return v === undefined ? grid.defaultCost : v;
    };

    const reached = dijkstra({
      start: { x: args.originX, y: args.originY },
      cost: costFn,
      neighbors: neighborsFn,
      maxCost: args.maxCost,
      maxExpansions: PATH_MAX_EXPANSIONS,
    });

    if (reached.size > MAX_RESULT_CELLS)
      throw new SenderError('grid.range_too_large');

    const cells: Array<{ x: number; y: number; cost: number }> = [];
    for (const node of reached.values()) {
      cells.push({ x: node.cell.x, y: node.cell.y, cost: node.cost });
    }
    return { cells };
  });
}

function requireGridForMutation(
  tx: TransactionModuleCtx,
  gridId: bigint,
  owner: string
): GridRow {
  const grid = tx.db.grid.id.find(gridId);
  if (!grid) throw new SenderError(`grid.not_found:${gridId}`);
  if (grid.mode === GRID_MODE_OWNER && grid.ownerUserId !== owner) {
    throw new SenderError(`grid.not_owner:${gridId}`);
  }
  return grid;
}

function requireGridForAccess(
  tx: TransactionModuleCtx,
  gridId: bigint,
  owner: string
): GridRow {
  const grid = tx.db.grid.id.find(gridId);
  if (!grid) throw new SenderError(`grid.not_found:${gridId}`);
  if (grid.mode === GRID_MODE_OWNER && grid.ownerUserId !== owner) {
    throw new SenderError(`grid.not_owner:${gridId}`);
  }
  return grid;
}

function assertInBounds(grid: GridRow, c: Coord): void {
  if (c.x < 0 || c.y < 0 || c.x >= grid.width || c.y >= grid.height) {
    throw new SenderError(`grid.out_of_bounds:${c.x},${c.y}`);
  }
}

function filterInBounds(grid: GridRow, list: Coord[]): Coord[] {
  return list.filter(
    c => c.x >= 0 && c.y >= 0 && c.x < grid.width && c.y < grid.height
  );
}

function upsertCellState(
  tx: TransactionModuleCtx,
  grid: GridRow,
  x: number,
  y: number,
  cost: number,
  terrain: string | undefined
): void {
  let existing = null;
  for (const c of tx.db.cellState.gridId.filter(grid.id)) {
    if (c.x === x && c.y === y) {
      existing = c;
      break;
    }
  }
  if (
    cost === grid.defaultCost &&
    (terrain === undefined || terrain === null)
  ) {
    if (existing) tx.db.cellState.delete(existing);
    return;
  }
  if (existing) {
    tx.db.cellState.id.update({ ...existing, cost, terrain });
    return;
  }
  tx.db.cellState.insert({ id: 0n, gridId: grid.id, x, y, cost, terrain });
}

function buildCostMap(
  tx: TransactionModuleCtx,
  grid: GridRow
): Map<string, number> {
  const map = new Map<string, number>();
  for (const c of tx.db.cellState.gridId.filter(grid.id)) {
    map.set(coordKey({ x: c.x, y: c.y }), c.cost);
  }
  for (const e of tx.db.gridEntity.gridId.filter(grid.id)) {
    if (!e.blocksMovement) continue;
    const k = coordKey({ x: e.x, y: e.y });
    if (!map.has(k)) map.set(k, -1);
  }
  return map;
}

function writeEntityPath(
  tx: TransactionModuleCtx,
  timestamp: Timestamp,
  entityId: bigint,
  gridId: bigint,
  cells: Coord[],
  cost: number
): void {
  const existing = tx.db.entityPath.entityId.find(entityId);
  const row = {
    entityId,
    gridId,
    cells: cells.map(c => ({ x: c.x, y: c.y })),
    cost,
    computedAt: timestamp,
  };
  if (existing) {
    tx.db.entityPath.entityId.update(row);
  } else {
    tx.db.entityPath.insert(row);
  }
}
