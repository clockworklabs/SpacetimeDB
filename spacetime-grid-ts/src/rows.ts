// ownerUserId is opaque and may contain an identity, application user ID, or
// host-defined actor ID. Owner mode restricts access to that value.
import { t } from 'spacetimedb/server';

export const GRID_KIND_SQUARE = 'square';
export const GRID_KIND_HEX = 'hex';

export const GRID_ORIENTATION_FLAT = 'flat';
export const GRID_ORIENTATION_POINTY = 'pointy';

export const GRID_MODE_OWNER = 'owner';
export const GRID_MODE_COLLABORATIVE = 'collaborative';

export const gridRow = {
  id: t.u64().primaryKey().autoInc(),
  ownerUserId: t.string().index(),
  name: t.string(),
  kind: t.string(), // 'square' | 'hex'
  orientation: t.string(), // 'flat' | 'pointy' (ignored for square)
  width: t.i32(),
  height: t.i32(),
  defaultCost: t.i32(),
  connectivity: t.i32(), // square only: 4 | 8. Hex is always 6.
  mode: t.string(), // 'owner' | 'collaborative'
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

// Sparse: only non-default cells have a row. cost<=0 means blocked.
export const cellStateRow = {
  id: t.u64().primaryKey().autoInc(),
  gridId: t.u64().index(),
  x: t.i32(),
  y: t.i32(),
  cost: t.i32(),
  terrain: t.option(t.string()),
};

export const gridEntityRow = {
  id: t.u64().primaryKey().autoInc(),
  gridId: t.u64().index(),
  ownerUserId: t.string().index(),
  x: t.i32(),
  y: t.i32(),
  kind: t.string(), // user-defined: 'player', 'npc', 'item', ...
  blocksMovement: t.bool(),
  label: t.option(t.string()),
  createdAt: t.timestamp(),
  updatedAt: t.timestamp(),
};

// Not auto-invalidated; consumers re-call compute_path for fresh routes.
export const entityPathRow = {
  entityId: t.u64().primaryKey(),
  gridId: t.u64().index(),
  cells: t.array(t.object('PathCell', { x: t.i32(), y: t.i32() })),
  cost: t.i32(),
  computedAt: t.timestamp(),
};

export const pathCell = t.object('PathCell', { x: t.i32(), y: t.i32() });

export const pathResult = t.object('PathResult', {
  found: t.bool(),
  cells: t.array(pathCell),
  cost: t.i32(),
  expanded: t.i32(),
});

export const reachableCell = t.object('ReachableCell', {
  x: t.i32(),
  y: t.i32(),
  cost: t.i32(),
});
