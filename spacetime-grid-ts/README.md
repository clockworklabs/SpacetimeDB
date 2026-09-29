# @spacetimedb/grid

Square and hex grids for SpacetimeDB modules, with sparse cell costs, owned or
collaborative entities, A\* pathfinding, and Dijkstra movement ranges inside the
gameplay transaction.

---

## Install

```bash
npm install @spacetimedb/grid spacetimedb
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

A grid is a row plus its associated `cellState`, `gridEntity`, and `entityPath`
rows. Everything is regular SpacetimeDB state, so clients subscribe to grid
changes like any other table.

## Usage

### Integrate into an application

Register the Grid namespace and wrap its helpers with the application's ownership
rules. Every helper runs inside the caller's transaction: pass `ctx.as.grid`
from a reducer, or `tx.as.grid` inside a procedure's `withTx` when the host
needs a return value.

```ts
import { schema, t } from 'spacetimedb/server';
import * as grid from '@spacetimedb/grid/submodule';

const spacetimedb = schema({ grid });
export default spacetimedb;

export const createPlayerGrid = spacetimedb.procedure(
  grid.createGridParams,
  t.u64(),
  (ctx, args) =>
    ctx.withTx(tx =>
      grid.createGrid(tx.as.grid, args, ctx.sender.toHexString())
    )
);

export const moveMyEntity = spacetimedb.reducer(
  grid.moveEntityParams,
  (ctx, args) => {
    grid.moveEntity(ctx.as.grid, args, ctx.sender.toHexString());
  }
);
```

The submodule treats owner values as opaque strings. Host operations must map
the authenticated caller to that string before calling helpers such as
`createGrid` or `moveEntity`. Grid tables are private, so the host also
exposes the rows each caller may see through its own views. See the
[Grid Tactics host module](./example/spacetimedb/)
for an authenticated boundary and scoped views.

The generated client calls the host operations:

```ts
const gridId = await conn.procedures.createPlayerGrid({
  name: 'Arena',
  kind: 'square',
  orientation: 'flat',
  width: 32,
  height: 32,
  defaultCost: 1,
  connectivity: 4,
  mode: 'owner',
});
```

### Standalone table builders

```ts
import {
  gridRow,
  cellStateRow,
  gridEntityRow,
  entityPathRow,
} from '@spacetimedb/grid/rows';
```

### `grid`

| Field              | Type              | Notes                                                                                   |
| ------------------ | ----------------- | --------------------------------------------------------------------------------------- |
| `id`               | `u64` PK auto-inc |                                                                                         |
| `ownerUserId`      | `string` indexed  | Opaque identity, application user ID, or host-defined actor ID                          |
| `name`             | `string`          |                                                                                         |
| `kind`             | `string`          | `GRID_KIND_SQUARE` or `GRID_KIND_HEX`                                                   |
| `orientation`      | `string`          | `GRID_ORIENTATION_FLAT` or `GRID_ORIENTATION_POINTY` (ignored when `kind === 'square'`) |
| `width` / `height` | `i32`             | Up to 1024 each                                                                         |
| `defaultCost`      | `i32`             | Per-cell traversal cost when no sparse row exists, 1 to 10,000                          |
| `connectivity`     | `i32`             | Square: `4` or `8`. Hex: always `6`.                                                    |
| `mode`             | `string`          | `GRID_MODE_OWNER` (creator-only mutation) or `GRID_MODE_COLLABORATIVE`                  |

### `cellState`

Sparse cell state stores rows for non-default cells. `cost <= 0` blocks the
cell; costs are at most 10,000. Rows are indexed by `gridId` and by
`(gridId, x, y)`.

### `gridEntity`

Entities placed on the grid. Each has `ownerUserId`, `kind` (user-defined
string), and `blocksMovement` for pathfinding. Movement uses `ownerUserId` for
authorization.

### `entityPath`

The last path written for an entity, with one row per `entityId`. Consumers call
`computePath` after cost-map changes to refresh the snapshot.

Helper types `pathCell`, `pathResult`, and `reachableCell` are exported for use in your own reducer and procedure signatures.

## Constants

| Constant                  | Value             |
| ------------------------- | ----------------- |
| `GRID_KIND_SQUARE`        | `'square'`        |
| `GRID_KIND_HEX`           | `'hex'`           |
| `GRID_ORIENTATION_FLAT`   | `'flat'`          |
| `GRID_ORIENTATION_POINTY` | `'pointy'`        |
| `GRID_MODE_OWNER`         | `'owner'`         |
| `GRID_MODE_COLLABORATIVE` | `'collaborative'` |

## API

Each helper takes `(tx, args, owner)`, where `tx` is `ctx.as.grid` or
`tx.as.grid`, and throws a code from the exported `errors` object on failure.
Parameter objects such as `createGridParams` can be passed straight to
`spacetimedb.reducer` or `spacetimedb.procedure`.

Package entrypoints:

- `@spacetimedb/grid/submodule` supplies the submodule tables and helpers.
- `@spacetimedb/grid` exports the lower-level rows, procedures, and math
  helpers.
- `@spacetimedb/grid/procedures` exports operation parameters,
  implementations, and result types.
- `@spacetimedb/grid/rows` exports lower-level row builders.
- `@spacetimedb/grid/math` exports standalone pathfinding primitives.

### `createGrid`

- Args: `name`, `kind`, `orientation`, `width`, `height`, `defaultCost`, `connectivity`, `mode`.
- Returns: `bigint` (the grid `id`).
- Validates kind / orientation / mode / dimensions (1-1024) / `defaultCost >= 1` / square connectivity in {4, 8}. Hex `connectivity` is forced to 6 regardless of input.

### `deleteGrid`

- Args: `gridId`.
- Cascades: deletes all `cellState`, `gridEntity`, and `entityPath` rows for the grid.
- Only the grid's owner may delete it, in either mode.

### `setCellCost`

- Args: `gridId`, `x`, `y`, `cost`, `terrain`.
- Upserts the sparse row. Setting `cost === grid.defaultCost` with empty terrain
  removes the sparse override. `cost` may be at most 10,000.
- `cost <= 0` blocks the cell for pathfinding.

### `paintCells`

- Args: `gridId`, `cells: PaintCell[]` (`{ x, y, cost, terrain}`).
- Batched `setCellCost` for editor brushes / map import.

### `placeEntity`

- Args: `gridId`, `x`, `y`, `kind`, `blocksMovement`, `label`.
- Returns: `bigint` (the entity `id`).
- Entity `ownerUserId` is set to the host-supplied `owner` value.

### `moveEntity`

- Args: `entityId`, `toX`, `toY`.
- Entity-owner-gated (independent of grid mode).
- Rejects with `grid.move_not_adjacent` unless `(toX, toY)` is in the entity's current neighbor set for the grid's `kind` and `connectivity`.
- Rejects with `grid.move_blocked` when the destination cell costs `<= 0` or holds another entity with `blocksMovement`.

For multi-step movement, drive sequential `moveEntity` calls from `computePath` results, or compute a path with `storeFor` and replay cells client-side.

### `computePath`

- Args: `gridId`, `startX`, `startY`, `endX`, `endY`, `storeFor` (entity id), `maxExpansions` (default 50,000).
- Returns: `PathResult { found, cells: PathCell[], cost, expanded }`.
- A\* over the live cost map (sparse `cellState`, with cells holding an entity with `blocksMovement` treated as blocked), using a kind-aware distance heuristic.
- When `storeFor` is set and a path is found, writes / overwrites the `entityPath` row for that entity.

### `cellsInRange`

- Args: `gridId`, `originX`, `originY`, `maxCost`.
- Returns: `{ cells: ReachableCell[] }`.
- Dijkstra flood-fill from origin out to `maxCost`. Useful for movement-range overlays, area-of-effect previews, line-of-sight gates.

## Errors

Helpers throw `SenderError` with the stable codes in `errors`, for example
`grid.not_found`, `grid.not_owner`, `grid.entity_not_owner`,
`grid.out_of_bounds`, `grid.move_not_adjacent`, and `grid.move_blocked`.

## Math helpers

`@spacetimedb/grid/math` exports the pathfinding primitives directly so you can run them off the live tables (e.g. for client-side preview or precomputed analysis):

- `neighbors(kind, coord, connectivity)`
- `distance(kind, a, b, connectivity)` - kind-aware heuristic
- `findPathAstar({ start, goal, cost, neighbors, heuristic, maxExpansions })`
- `dijkstra({ start, cost, neighbors, maxCost })`
- `coordKey(coord)` - stable string key for `Map<string, ...>`

Types: `Coord`, `GridKind`, `Connectivity`.

## Testing

```bash
pnpm test
pnpm run typecheck
```

Build the
[example host module](./example/spacetimedb/)
to verify the
submodule schema, helpers, and generated bindings.

## License

[Apache-2.0](./LICENSE.txt).
