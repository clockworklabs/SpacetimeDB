# @spacetimedb/grid

Square and hex grids for SpacetimeDB modules, with sparse cell costs, owned or
collaborative entities, A\* pathfinding, and Dijkstra movement ranges inside the
gameplay transaction.

---

## Install

```bash
npm install @spacetimedb/grid spacetimedb@^2.8.3
```

Requires SpacetimeDB 2.8.3 or later for submodule mounting.

For the install-to-publish workflow, see
[Getting started](https://spacetimedb.com/docs/).

A grid is a row plus its associated `cellState`, `gridEntity`, and `entityPath`
rows. Everything is regular SpacetimeDB state, so clients subscribe to grid
changes like any other table.

## Usage

### Integrate into an application

Mount the grid namespace, initialize it from the host lifecycle hook, and wrap
its helpers with the application's ownership rules:

```ts
import { schema, t } from 'spacetimedb/server';
import * as grid from '@spacetimedb/grid/submodule';

const spacetimedb = schema({ grid });
export default spacetimedb;

export const init = spacetimedb.init(ctx => {
  grid.installGrid(ctx.as.grid);
});

export const create_player_grid = spacetimedb.procedure(
  grid.createGridParams,
  t.u64(),
  (ctx, args) =>
    grid.createGridImpl(ctx.as.grid, args, ctx.sender.toHexString())
);
```

The submodule treats owner values as opaque strings. Host operations must map
the authenticated caller to that string before calling helpers such as
`createGrid` or `moveEntity`. See the
[Grid Tactics host module](./example/spacetimedb/)
for an authenticated boundary and scoped views.

The generated client calls the host procedure, then subscribes to the host's
caller-scoped grid views:

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

conn.subscriptionBuilder().subscribe(['SELECT * FROM my_grids']);
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
| `defaultCost`      | `i32`             | Per-cell traversal cost when no sparse row exists                                       |
| `connectivity`     | `i32`             | Square: `4` or `8`. Hex: always `6`.                                                    |
| `mode`             | `string`          | `GRID_MODE_OWNER` (creator-only mutation) or `GRID_MODE_COLLABORATIVE`                  |

### `cellState`

Sparse cell state stores rows for non-default cells. `cost <= 0` blocks the
cell. Rows are indexed by `gridId`.

### `gridEntity`

Entities placed on the grid. Each has `ownerUserId`, `kind` (user-defined
string), and `blocksMovement` for pathfinding. Movement uses `ownerUserId` for
authorization.

### `entityPath`

The last path written for an entity, with one row per `entityId`. Consumers call
`computePath` after cost-map changes to refresh the snapshot.

Helper types `pathCell`, `pathResult`, and `reachableCell` are exported for use in your own procedure signatures.

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

Each `*Impl` takes `(ctx, args, owner)`. Wrap them with thin reducers in your module that supply `owner` however your auth scheme works.

Package entrypoints:

- `@spacetimedb/grid/submodule` supplies the mounted tables and helpers.
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
- Owner-mode-gated (collaborative mode allows any caller).

### `setCellCost`

- Args: `gridId`, `x`, `y`, `cost`, `terrain`.
- Upserts the sparse row. Setting `cost === grid.defaultCost` with empty terrain
  removes the sparse override.
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

For multi-step movement, drive sequential `moveEntity` calls from `computePath` results, or compute a path with `storeFor` and replay cells client-side.

### `computePath`

- Args: `gridId`, `startX`, `startY`, `endX`, `endY`, `storeFor` (entity id), `maxExpansions` (default 50,000).
- Returns: `PathResult { found, cells: PathCell[], cost, expanded }`.
- A\* over the live cost map (sparse `cellState` + entities with `blocksMovement`), using a kind-aware distance heuristic.
- When `storeFor` is set and a path is found, writes / overwrites the `entityPath` row for that entity.

### `cellsInRange`

- Args: `gridId`, `originX`, `originY`, `maxCost`.
- Returns: `{ cells: ReachableCell[] }`.
- Dijkstra flood-fill from origin out to `maxCost`. Useful for movement-range overlays, area-of-effect previews, line-of-sight gates.

## Errors

All `SenderError` with stable codes:

- `grid.invalid_kind` / `grid.invalid_orientation` / `grid.invalid_mode` / `grid.invalid_connectivity`
- `grid.invalid_dimensions:<w>x<h>` - outside 1-1024
- `grid.invalid_default_cost:<n>` - `defaultCost < 1`
- `grid.not_found:<id>` - missing grid
- `grid.not_owner:<id>` - caller lacks grid ownership when `mode === 'owner'`
- `grid.entity_not_found:<id>` / `grid.entity_not_owner:<id>`
- `grid.out_of_bounds:<x>,<y>`
- `grid.move_not_adjacent` - destination not in current neighbor set

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
mounted schema, procedures, and generated bindings.

## License

[BUSL-1.1](./LICENSE.txt) - same as SpacetimeDB.
