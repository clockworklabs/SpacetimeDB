// Owner is passed explicitly so the submodule is identity-scheme-agnostic.
// Each helper runs inside the caller's transaction: pass `ctx.as.grid` from a
// reducer or `tx.as.grid` inside a procedure's `withTx`.
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
} from './rows';
import { errors } from './errors';
import {
  type Coord,
  type GridKind,
  type Connectivity,
  coordKey,
  neighbors,
  distance,
  findPathAstar,
  dijkstra,
} from './math/index';
import type { ReducerModuleCtx } from './submodule/schema';

type GridRow = Infer<typeof gridRow>;

const VALID_KINDS = new Set([GRID_KIND_SQUARE, GRID_KIND_HEX]);
const VALID_ORIENTATIONS = new Set([
  GRID_ORIENTATION_FLAT,
  GRID_ORIENTATION_POINTY,
]);
const VALID_MODES = new Set([GRID_MODE_OWNER, GRID_MODE_COLLABORATIVE]);
const VALID_CONNECTIVITY = new Set([4, 8]);

const GRID_MAX_DIM = 1024;
// Path costs stay inside i32: at most PATH_MAX_EXPANSIONS steps of MAX_CELL_COST.
const MAX_CELL_COST = 10_000;
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

export function createGrid(
  tx: ReducerModuleCtx,
  args: InferTypeOfParams<typeof createGridParams>,
  owner: string
): bigint {
  if (args.name.length === 0 || args.name.length > MAX_NAME_LENGTH) {
    throw new SenderError(errors.invalidName);
  }
  if (!VALID_KINDS.has(args.kind)) throw new SenderError(errors.invalidKind);
  if (!VALID_ORIENTATIONS.has(args.orientation)) {
    throw new SenderError(errors.invalidOrientation);
  }
  if (!VALID_MODES.has(args.mode)) throw new SenderError(errors.invalidMode);
  if (
    args.width < 1 ||
    args.width > GRID_MAX_DIM ||
    args.height < 1 ||
    args.height > GRID_MAX_DIM
  ) {
    throw new SenderError(errors.invalidDimensions);
  }
  if (args.defaultCost < 1 || args.defaultCost > MAX_CELL_COST) {
    throw new SenderError(errors.invalidDefaultCost);
  }
  if (
    args.kind === GRID_KIND_SQUARE &&
    !VALID_CONNECTIVITY.has(args.connectivity)
  ) {
    throw new SenderError(errors.invalidConnectivity);
  }
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
    createdAt: tx.timestamp,
    updatedAt: tx.timestamp,
  });
  return row.id;
}

// Only the grid's owner deletes it, in either mode. Cascades cell_state,
// grid_entity, and entity_path.
export const deleteGridParams = {
  gridId: t.u64(),
};

export function deleteGrid(
  tx: ReducerModuleCtx,
  args: InferTypeOfParams<typeof deleteGridParams>,
  owner: string
): void {
  const grid = tx.db.grid.id.find(args.gridId);
  if (!grid) throw new SenderError(errors.notFound);
  if (grid.ownerUserId !== owner) throw new SenderError(errors.notOwner);
  for (const c of [...tx.db.cellState.gridId.filter(grid.id)])
    tx.db.cellState.delete(c);
  for (const e of [...tx.db.gridEntity.gridId.filter(grid.id)])
    tx.db.gridEntity.delete(e);
  for (const p of [...tx.db.entityPath.gridId.filter(grid.id)])
    tx.db.entityPath.delete(p);
  tx.db.grid.delete(grid);
}

// cost<=0 blocks, cost==defaultCost removes the sparse row.
export const setCellCostParams = {
  gridId: t.u64(),
  x: t.i32(),
  y: t.i32(),
  cost: t.i32(),
  terrain: t.option(t.string()),
};

export function setCellCost(
  tx: ReducerModuleCtx,
  args: InferTypeOfParams<typeof setCellCostParams>,
  owner: string
): void {
  const grid = requireGrid(tx, args.gridId, owner);
  upsertCellState(tx, grid, args);
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

export function paintCells(
  tx: ReducerModuleCtx,
  args: InferTypeOfParams<typeof paintCellsParams>,
  owner: string
): void {
  if (args.cells.length > MAX_PAINT_CELLS) {
    throw new SenderError(errors.tooManyCells);
  }
  const grid = requireGrid(tx, args.gridId, owner);
  for (const cell of args.cells) upsertCellState(tx, grid, cell);
}

export const placeEntityParams = {
  gridId: t.u64(),
  x: t.i32(),
  y: t.i32(),
  kind: t.string(),
  blocksMovement: t.bool(),
  label: t.option(t.string()),
};

export function placeEntity(
  tx: ReducerModuleCtx,
  args: InferTypeOfParams<typeof placeEntityParams>,
  owner: string
): bigint {
  if (args.kind.length === 0 || args.kind.length > MAX_KIND_LENGTH) {
    throw new SenderError(errors.invalidEntityKind);
  }
  if ((args.label?.length ?? 0) > MAX_LABEL_LENGTH) {
    throw new SenderError(errors.invalidEntityLabel);
  }
  const grid = requireGrid(tx, args.gridId, owner);
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
    createdAt: tx.timestamp,
    updatedAt: tx.timestamp,
  });
  return row.id;
}

export const moveEntityParams = {
  entityId: t.u64(),
  toX: t.i32(),
  toY: t.i32(),
};

// Moves one step to an adjacent cell that is not blocked by terrain or by
// another blocking entity.
export function moveEntity(
  tx: ReducerModuleCtx,
  args: InferTypeOfParams<typeof moveEntityParams>,
  owner: string
): void {
  const ent = tx.db.gridEntity.id.find(args.entityId);
  if (!ent) throw new SenderError(errors.entityNotFound);
  if (ent.ownerUserId !== owner) throw new SenderError(errors.entityNotOwner);
  const grid = tx.db.grid.id.find(ent.gridId);
  if (!grid) throw new SenderError(errors.notFound);
  const to = { x: args.toX, y: args.toY };
  assertInBounds(grid, to);

  const adjacent = neighbors(
    grid.kind as GridKind,
    { x: ent.x, y: ent.y },
    grid.connectivity as Connectivity
  ).some(n => n.x === to.x && n.y === to.y);
  if (!adjacent) throw new SenderError(errors.moveNotAdjacent);
  if (
    cellCost(tx, grid, to) <= 0 ||
    [...tx.db.gridEntity.byGridCell.filter([grid.id, to.x, to.y])].some(
      other => other.blocksMovement && other.id !== ent.id
    )
  ) {
    throw new SenderError(errors.moveBlocked);
  }

  tx.db.gridEntity.id.update({
    ...ent,
    x: to.x,
    y: to.y,
    updatedAt: tx.timestamp,
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

export function computePath(
  tx: ReducerModuleCtx,
  args: InferTypeOfParams<typeof computePathParams>,
  owner: string
): Infer<typeof pathResult> {
  const grid = requireGrid(tx, args.gridId, owner);
  assertInBounds(grid, { x: args.startX, y: args.startY });
  assertInBounds(grid, { x: args.endX, y: args.endY });
  const maxExpansions = args.maxExpansions ?? PATH_MAX_EXPANSIONS;
  if (maxExpansions < 1 || maxExpansions > PATH_MAX_EXPANSIONS) {
    throw new SenderError(errors.invalidMaxExpansions);
  }

  const costMap = buildCostMap(tx, grid);
  const result = findPathAstar({
    start: { x: args.startX, y: args.startY },
    goal: { x: args.endX, y: args.endY },
    cost: c => costMap.get(coordKey(c)) ?? grid.defaultCost,
    neighbors: c => neighborsInBounds(grid, c),
    heuristic: (a, b) =>
      distance(grid.kind as GridKind, a, b, grid.connectivity as Connectivity),
    maxExpansions,
  });
  if (!result.found) {
    return { found: false, cells: [], cost: 0, expanded: result.expanded };
  }
  if (result.cells.length > MAX_RESULT_CELLS) {
    throw new SenderError(errors.pathTooLong);
  }

  if (args.storeFor !== undefined) {
    const entity = tx.db.gridEntity.id.find(args.storeFor);
    if (!entity) throw new SenderError(errors.entityNotFound);
    if (entity.gridId !== grid.id) {
      throw new SenderError(errors.entityGridMismatch);
    }
    if (entity.ownerUserId !== owner) {
      throw new SenderError(errors.entityNotOwner);
    }
    writeEntityPath(tx, args.storeFor, grid.id, result.cells, result.cost);
  }

  return {
    found: true,
    cells: result.cells,
    cost: result.cost,
    expanded: result.expanded,
  };
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

export function cellsInRange(
  tx: ReducerModuleCtx,
  args: InferTypeOfParams<typeof cellsInRangeParams>,
  owner: string
): Infer<typeof cellsInRangeReturn> {
  const grid = requireGrid(tx, args.gridId, owner);
  assertInBounds(grid, { x: args.originX, y: args.originY });

  const costMap = buildCostMap(tx, grid);
  const reached = dijkstra({
    start: { x: args.originX, y: args.originY },
    cost: c => costMap.get(coordKey(c)) ?? grid.defaultCost,
    neighbors: c => neighborsInBounds(grid, c),
    maxCost: args.maxCost,
    maxExpansions: PATH_MAX_EXPANSIONS,
  });
  if (reached.size > MAX_RESULT_CELLS) {
    throw new SenderError(errors.rangeTooLarge);
  }
  return {
    cells: [...reached.values()].map(node => ({
      x: node.cell.x,
      y: node.cell.y,
      cost: node.cost,
    })),
  };
}

// Owner mode restricts every operation to the grid owner.
function requireGrid(
  tx: ReducerModuleCtx,
  gridId: bigint,
  owner: string
): GridRow {
  const grid = tx.db.grid.id.find(gridId);
  if (!grid) throw new SenderError(errors.notFound);
  if (grid.mode === GRID_MODE_OWNER && grid.ownerUserId !== owner) {
    throw new SenderError(errors.notOwner);
  }
  return grid;
}

function assertInBounds(grid: GridRow, c: Coord): void {
  if (c.x < 0 || c.y < 0 || c.x >= grid.width || c.y >= grid.height) {
    throw new SenderError(errors.outOfBounds);
  }
}

function neighborsInBounds(grid: GridRow, c: Coord): Coord[] {
  return neighbors(
    grid.kind as GridKind,
    c,
    grid.connectivity as Connectivity
  ).filter(n => n.x >= 0 && n.y >= 0 && n.x < grid.width && n.y < grid.height);
}

function findCellState(tx: ReducerModuleCtx, gridId: bigint, c: Coord) {
  for (const row of tx.db.cellState.byGridCell.filter([gridId, c.x, c.y])) {
    return row;
  }
  return undefined;
}

function cellCost(tx: ReducerModuleCtx, grid: GridRow, c: Coord): number {
  return findCellState(tx, grid.id, c)?.cost ?? grid.defaultCost;
}

function upsertCellState(
  tx: ReducerModuleCtx,
  grid: GridRow,
  cell: { x: number; y: number; cost: number; terrain?: string | undefined }
): void {
  if (cell.cost > MAX_CELL_COST) throw new SenderError(errors.invalidCost);
  if ((cell.terrain?.length ?? 0) > MAX_TERRAIN_LENGTH) {
    throw new SenderError(errors.invalidTerrain);
  }
  assertInBounds(grid, cell);
  const existing = findCellState(tx, grid.id, cell);
  if (cell.cost === grid.defaultCost && cell.terrain === undefined) {
    if (existing) tx.db.cellState.delete(existing);
    return;
  }
  if (existing) {
    tx.db.cellState.id.update({
      ...existing,
      cost: cell.cost,
      terrain: cell.terrain,
    });
    return;
  }
  tx.db.cellState.insert({
    id: 0n,
    gridId: grid.id,
    x: cell.x,
    y: cell.y,
    cost: cell.cost,
    terrain: cell.terrain,
  });
}

// Blocking entities override the cell cost, so painted cells cannot hide them.
function buildCostMap(
  tx: ReducerModuleCtx,
  grid: GridRow
): Map<string, number> {
  const map = new Map<string, number>();
  for (const c of tx.db.cellState.gridId.filter(grid.id)) {
    map.set(coordKey(c), c.cost);
  }
  for (const e of tx.db.gridEntity.gridId.filter(grid.id)) {
    if (e.blocksMovement) map.set(coordKey(e), -1);
  }
  return map;
}

function writeEntityPath(
  tx: ReducerModuleCtx,
  entityId: bigint,
  gridId: bigint,
  cells: Coord[],
  cost: number
): void {
  const row = {
    entityId,
    gridId,
    cells: cells.map(c => ({ x: c.x, y: c.y })),
    cost,
    computedAt: tx.timestamp,
  };
  if (tx.db.entityPath.entityId.find(entityId)) {
    tx.db.entityPath.entityId.update(row);
  } else {
    tx.db.entityPath.insert(row);
  }
}
