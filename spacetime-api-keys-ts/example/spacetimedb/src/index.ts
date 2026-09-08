import {
  Router,
  Range,
  SenderError,
  t,
  type Infer,
  type Request,
  type SyncResponse,
} from 'spacetimedb/server';
import { ScheduleAt } from 'spacetimedb';
import * as apiKeys from '@spacetimedb/api-keys/submodule';
import * as gridSubmodule from '@spacetimedb/grid/submodule';
import {
  GRID_KIND_SQUARE,
  GRID_MODE_OWNER,
  GRID_ORIENTATION_FLAT,
} from '@spacetimedb/grid/submodule';
import {
  installPresenceConfig,
  removePresence,
  runPresenceSweep,
  upsertPresence,
} from '@spacetimedb/presence';
import {
  accessKeySummary,
  colonySweepTick,
  spacetimedb,
  type HttpCtx,
  type ReadCtx,
  type Tx,
} from './schema';
import {
  asI32,
  asObject,
  asOptionalString,
  asString,
  errorResponse,
  jsonResponse,
  readBearer,
  safeJson,
} from './http';

const COLONY_WIDTH = 12;
const COLONY_HEIGHT = 8;
const EVENT_RETAIN = 120;
const PRESENCE_TTL_SECONDS = 35;
const PRESENCE_SCOPE_MAX = 128;
const PRESENCE_NAME_MAX = 64;
const PRESENCE_ROLE_MAX = 32;
const PRESENCE_COLOR_PATTERN = /^#[0-9a-fA-F]{6}$/;
const PRESENCE_ROLES = new Set([
  'Owner',
  'Collaborator',
  'Terraformer',
  'Builder',
  'Planter',
  'Viewer',
  'Editor',
]);
const SWEEP_INTERVAL_MICROS = 10n * 1_000_000n;

// Scopes a share key can carry. view is read; the three edit scopes are the
// granular powers a share link can grant.
const SCOPE_VIEW = 'colony:view';
const SCOPE_TERRAFORM = 'colony:terraform';
const SCOPE_BUILD = 'colony:build';
const SCOPE_PLANT = 'colony:plant';

export default spacetimedb;

type ApiKeyCreateResult = Infer<typeof apiKeys.apiKeyCreateResult>;

// Surface terrain. regolith is the default (no row); the rest are stored.
const DEFAULT_TERRAIN = 'regolith';
const terrainCost: Record<string, number> = {
  rock: 0,
  grass: 1,
  water: 1,
  soil: 1,
};
const validTerrain = new Set([DEFAULT_TERRAIN, ...Object.keys(terrainCost)]);

// Placeable objects. Structures are gated by colony:build, nature by
// colony:plant, so a build-only and a plant-only key are visibly different.
const STRUCTURE_KINDS = new Set(['dome', 'pod', 'solar', 'road']);
const NATURE_KINDS = new Set(['tree', 'shrub', 'boulder']);

export const init = spacetimedb.init(ctx => {
  apiKeys.installApiKeys(ctx.as.apiKeys);
  gridSubmodule.installGrid(ctx.as.grid);
  installPresenceConfig(ctx, {
    defaultTtlSeconds: PRESENCE_TTL_SECONDS,
    sweepBatch: 500,
  });
  ctx.db.colonySweepTick.insert({
    scheduledId: 0n,
    scheduledAt: ScheduleAt.interval(SWEEP_INTERVAL_MICROS),
  });
});

function senderSubject(ctx: { sender: unknown }): string {
  const sender = ctx.sender as { toHexString?: () => string };
  return typeof sender?.toHexString === 'function'
    ? sender.toHexString()
    : String(ctx.sender);
}

function assertInBounds(x: number, y: number): void {
  if (x < 0 || y < 0 || x >= COLONY_WIDTH || y >= COLONY_HEIGHT) {
    throw new SenderError(`world.out_of_bounds:${x},${y}`);
  }
}

function ensureWorldTx(tx: Tx, ownerSubject: string) {
  const existing = tx.db.world.ownerSubject.find(ownerSubject);
  if (existing) return existing;

  const grid = tx.db.grid.grid.insert({
    id: 0n,
    ownerUserId: ownerSubject,
    name: 'Colony Grid',
    kind: GRID_KIND_SQUARE,
    orientation: GRID_ORIENTATION_FLAT,
    width: COLONY_WIDTH,
    height: COLONY_HEIGHT,
    defaultCost: 1,
    connectivity: 4,
    mode: GRID_MODE_OWNER,
    createdAt: tx.timestamp,
    updatedAt: tx.timestamp,
  });

  const seedCells = [
    [2, 1, 'grass'],
    [3, 1, 'grass'],
    [2, 2, 'grass'],
    [8, 2, 'water'],
    [9, 2, 'water'],
    [10, 5, 'rock'],
    [10, 6, 'rock'],
    [3, 6, 'rock'],
  ] as const;
  for (const [x, y, terrain] of seedCells) {
    tx.db.grid.cellState.insert({
      id: 0n,
      gridId: grid.id,
      x,
      y,
      cost: terrainCost[terrain],
      terrain,
    });
  }

  tx.db.grid.gridEntity.insert({
    id: 0n,
    gridId: grid.id,
    ownerUserId: ownerSubject,
    x: 5,
    y: 3,
    kind: 'dome',
    blocksMovement: false,
    label: 'Landing Dome',
    createdAt: tx.timestamp,
    updatedAt: tx.timestamp,
  });

  const row = tx.db.world.insert({
    ownerSubject,
    gridId: grid.id,
    name: 'Colony',
    createdAt: tx.timestamp,
    updatedAt: tx.timestamp,
  });
  insertEvent(
    tx,
    ownerSubject,
    '',
    'colony.created',
    true,
    'created',
    'Colony founded'
  );
  return row;
}

function deleteWorldTx(tx: Tx, ownerSubject: string): void {
  const existing = tx.db.world.ownerSubject.find(ownerSubject);
  if (!existing) return;
  for (const row of [...tx.db.grid.cellState.gridId.filter(existing.gridId)])
    tx.db.grid.cellState.delete(row);
  for (const row of [...tx.db.grid.gridEntity.gridId.filter(existing.gridId)])
    tx.db.grid.gridEntity.delete(row);
  for (const row of [...tx.db.grid.entityPath.gridId.filter(existing.gridId)])
    tx.db.grid.entityPath.delete(row);
  const grid = tx.db.grid.grid.id.find(existing.gridId);
  if (grid) tx.db.grid.grid.delete(grid);
  tx.db.world.delete(existing);
}

function insertEvent(
  tx: Tx,
  ownerSubject: string,
  keyPrefix: string,
  action: string,
  allowed: boolean,
  reason: string,
  message: string
): void {
  tx.db.worldEvent.insert({
    eventId: 0n,
    ownerSubject,
    keyPrefix,
    action,
    allowed,
    reason,
    message,
    createdAt: tx.timestamp,
  });

  const rows = [...tx.db.worldEvent.ownerSubject.filter(ownerSubject)];
  if (rows.length <= EVENT_RETAIN) return;
  rows.sort((a, b) => {
    const av = a.createdAt.microsSinceUnixEpoch as bigint;
    const bv = b.createdAt.microsSinceUnixEpoch as bigint;
    return av < bv ? -1 : av > bv ? 1 : 0;
  });
  for (const row of rows.slice(0, rows.length - EVENT_RETAIN))
    tx.db.worldEvent.delete(row);
}

function findCell(tx: Tx, gridId: bigint, x: number, y: number) {
  for (const row of tx.db.grid.cellState.gridId.filter(gridId)) {
    if (row.x === x && row.y === y) return row;
  }
  return undefined;
}

function findEntityAt(tx: Tx, gridId: bigint, x: number, y: number) {
  for (const row of tx.db.grid.gridEntity.gridId.filter(gridId)) {
    if (row.x === x && row.y === y) return row;
  }
  return undefined;
}

function upsertTerrain(
  tx: Tx,
  gridId: bigint,
  x: number,
  y: number,
  terrain: string
): void {
  assertInBounds(x, y);
  if (!validTerrain.has(terrain))
    throw new SenderError(`world.invalid_terrain:${terrain}`);
  const existing = findCell(tx, gridId, x, y);
  // Regolith is the bare surface, so painting it clears the cell row.
  if (terrain === DEFAULT_TERRAIN) {
    if (existing) tx.db.grid.cellState.delete(existing);
    return;
  }
  const cost = terrainCost[terrain];
  if (existing) tx.db.grid.cellState.id.update({ ...existing, cost, terrain });
  else tx.db.grid.cellState.insert({ id: 0n, gridId, x, y, cost, terrain });
}

// Core mutations shared by the owner reducers and the scoped
// HTTP routes. keyPrefix is '' for native owner edits, or the key prefix
// for share-key edits (for the activity feed).

function doTerraform(
  tx: Tx,
  ownerSubject: string,
  keyPrefix: string,
  x: number,
  y: number,
  terrain: string
) {
  const w = ensureWorldTx(tx, ownerSubject);
  upsertTerrain(tx, w.gridId, x, y, terrain);
  insertEvent(
    tx,
    ownerSubject,
    keyPrefix,
    'terraform',
    true,
    'allowed',
    `Terraformed ${terrain} at ${x},${y}`
  );
  return { x, y, terrain };
}

function mergeRoadMask(a: string, b: string): string {
  const out: string[] = [];
  for (const ch of a + b)
    if ('nesw'.includes(ch) && !out.includes(ch)) out.push(ch);
  return out.join('');
}

function doBuild(
  tx: Tx,
  ownerSubject: string,
  keyPrefix: string,
  x: number,
  y: number,
  kind: string,
  label?: string
) {
  assertInBounds(x, y);
  if (!STRUCTURE_KINDS.has(kind))
    throw new SenderError(`world.invalid_kind:${kind}`);
  const w = ensureWorldTx(tx, ownerSubject);
  const occupant = findEntityAt(tx, w.gridId, x, y);
  if (occupant) {
    // Dragging a road into an existing road merges the connection into it (so
    // adjacent existing roads link when you drag between them); anything else
    // on the cell blocks the build.
    if (kind === 'road' && occupant.kind === 'road') {
      const merged = mergeRoadMask(occupant.label ?? '', label ?? '');
      if (merged !== (occupant.label ?? '')) {
        tx.db.grid.gridEntity.id.update({
          ...occupant,
          label: merged,
          updatedAt: tx.timestamp,
        });
      }
      return { entityId: occupant.id, x, y, kind };
    }
    throw new SenderError(`world.cell_occupied:${x},${y}`);
  }
  // Roads carry a connection mask (which sides link) in label, computed as you
  // draw; other structures store their kind.
  const entity = tx.db.grid.gridEntity.insert({
    id: 0n,
    gridId: w.gridId,
    ownerUserId: ownerSubject,
    x,
    y,
    kind,
    blocksMovement: false,
    label: label ?? kind,
    createdAt: tx.timestamp,
    updatedAt: tx.timestamp,
  });
  insertEvent(
    tx,
    ownerSubject,
    keyPrefix,
    'build',
    true,
    'allowed',
    `Built ${kind} at ${x},${y}`
  );
  return { entityId: entity.id, x, y, kind };
}

function doUnbuild(
  tx: Tx,
  ownerSubject: string,
  keyPrefix: string,
  x: number,
  y: number
) {
  assertInBounds(x, y);
  const w = ensureWorldTx(tx, ownerSubject);
  const entity = findEntityAt(tx, w.gridId, x, y);
  if (!entity || !STRUCTURE_KINDS.has(entity.kind))
    throw new SenderError(`world.nothing_to_remove:${x},${y}`);
  tx.db.grid.gridEntity.delete(entity);
  insertEvent(
    tx,
    ownerSubject,
    keyPrefix,
    'unbuild',
    true,
    'allowed',
    `Removed ${entity.kind} at ${x},${y}`
  );
  return { x, y };
}

function doPlant(
  tx: Tx,
  ownerSubject: string,
  keyPrefix: string,
  x: number,
  y: number,
  kind: string
) {
  assertInBounds(x, y);
  if (!NATURE_KINDS.has(kind))
    throw new SenderError(`world.invalid_kind:${kind}`);
  const w = ensureWorldTx(tx, ownerSubject);
  if (findEntityAt(tx, w.gridId, x, y))
    throw new SenderError(`world.cell_occupied:${x},${y}`);
  const entity = tx.db.grid.gridEntity.insert({
    id: 0n,
    gridId: w.gridId,
    ownerUserId: ownerSubject,
    x,
    y,
    kind,
    blocksMovement: false,
    label: kind,
    createdAt: tx.timestamp,
    updatedAt: tx.timestamp,
  });
  insertEvent(
    tx,
    ownerSubject,
    keyPrefix,
    'plant',
    true,
    'allowed',
    `Planted ${kind} at ${x},${y}`
  );
  return { entityId: entity.id, x, y, kind };
}

function doClear(
  tx: Tx,
  ownerSubject: string,
  keyPrefix: string,
  x: number,
  y: number
) {
  assertInBounds(x, y);
  const w = ensureWorldTx(tx, ownerSubject);
  const entity = findEntityAt(tx, w.gridId, x, y);
  if (!entity || !NATURE_KINDS.has(entity.kind))
    throw new SenderError(`world.nothing_to_clear:${x},${y}`);
  tx.db.grid.gridEntity.delete(entity);
  insertEvent(
    tx,
    ownerSubject,
    keyPrefix,
    'clear',
    true,
    'allowed',
    `Cleared ${entity.kind} at ${x},${y}`
  );
  return { x, y };
}

function readWorldSnapshot(tx: Tx, ownerSubject: string) {
  const w = ensureWorldTx(tx, ownerSubject);
  return {
    world: w,
    grid: tx.db.grid.grid.id.find(w.gridId),
    cells: [...tx.db.grid.cellState.gridId.filter(w.gridId)],
    entities: [...tx.db.grid.gridEntity.gridId.filter(w.gridId)],
  };
}

function mirrorAccessKey(tx: Tx, row: ApiKeyCreateResult): void {
  const summary = {
    keyId: row.keyId,
    prefix: row.prefix,
    ownerSubject: row.ownerSubject,
    name: row.name,
    scopesJson: row.scopesJson,
    metadataJson: row.metadataJson,
    status: row.status,
    createdAt: row.createdAt,
    expiresAt: row.expiresAt,
    lastUsedAt: undefined,
    revokedAt: undefined,
  };
  const existing = tx.db.accessKeySummary.keyId.find(summary.keyId);
  if (existing) tx.db.accessKeySummary.keyId.update(summary);
  else tx.db.accessKeySummary.insert(summary);
}

function verifyRequest(
  tx: Tx,
  req: Request,
  requiredScope: string,
  action: string
) {
  const key = readBearer(req);
  if (!key)
    return { allowed: false, reason: 'missing_bearer', status: 401 } as const;
  const result = apiKeys.verifyApiKey(tx.as.apiKeys, {
    key,
    requiredScope,
    action,
  });
  if (!result.allowed) {
    if (result.ownerSubject) {
      insertEvent(
        tx,
        result.ownerSubject,
        result.prefix ?? '',
        action,
        false,
        result.reason,
        `${action} denied: ${result.reason}`
      );
    }
    return {
      allowed: false,
      reason: result.reason,
      status: result.reason === 'scope_denied' ? 403 : 401,
    } as const;
  }
  if (!result.ownerSubject) {
    return { allowed: false, reason: 'missing_owner', status: 401 } as const;
  }
  return {
    allowed: true,
    keyPrefix: result.prefix ?? '',
    ownerSubject: result.ownerSubject,
    scopesJson: result.scopesJson ?? '[]',
  } as const;
}

function handleAuthedWorldAction(
  ctx: HttpCtx,
  req: Request,
  requiredScope: string,
  action: string,
  fn: (
    tx: Tx,
    ownerSubject: string,
    keyPrefix: string,
    scopesJson: string
  ) => unknown
): SyncResponse {
  try {
    const out = ctx.withTx((tx: Tx) => {
      const auth = verifyRequest(tx, req, requiredScope, action);
      if (!auth.allowed) return { error: auth.reason, status: auth.status };
      return {
        ok: true,
        value: fn(
          tx,
          auth.ownerSubject,
          auth.keyPrefix,
          auth.scopesJson ?? '[]'
        ),
      };
    });
    if (out?.error) return errorResponse(out.error, out.status);
    return jsonResponse({ ok: true, result: out?.value ?? null });
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return errorResponse(
      message,
      message.startsWith('world.invalid') ? 400 : 500
    );
  }
}

export const ensure_world = spacetimedb.procedure(
  {},
  t.object('EnsureWorldResult', { ownerSubject: t.string(), gridId: t.u64() }),
  ctx =>
    ctx.withTx(tx => {
      const w = ensureWorldTx(tx, senderSubject(ctx));
      return { ownerSubject: w.ownerSubject, gridId: w.gridId };
    })
);

export const reset_world = spacetimedb.reducer({}, ctx => {
  const ownerSubject = senderSubject(ctx);
  deleteWorldTx(ctx, ownerSubject);
  ensureWorldTx(ctx, ownerSubject);
  insertEvent(
    ctx,
    ownerSubject,
    '',
    'colony.reset',
    true,
    'reset',
    'Colony reset'
  );
});

export const clear_world_events = spacetimedb.reducer({}, ctx => {
  const ownerSubject = senderSubject(ctx);
  for (const row of [...ctx.db.worldEvent.ownerSubject.filter(ownerSubject)]) {
    ctx.db.worldEvent.delete(row);
  }
});

export const terraform = spacetimedb.reducer(
  { x: t.i32(), y: t.i32(), terrain: t.string() },
  (ctx, args) => {
    doTerraform(ctx, senderSubject(ctx), '', args.x, args.y, args.terrain);
  }
);

export const build = spacetimedb.reducer(
  { x: t.i32(), y: t.i32(), kind: t.string(), label: t.option(t.string()) },
  (ctx, args) => {
    doBuild(ctx, senderSubject(ctx), '', args.x, args.y, args.kind, args.label);
  }
);

export const unbuild = spacetimedb.reducer(
  { x: t.i32(), y: t.i32() },
  (ctx, args) => {
    doUnbuild(ctx, senderSubject(ctx), '', args.x, args.y);
  }
);

export const plant = spacetimedb.reducer(
  { x: t.i32(), y: t.i32(), kind: t.option(t.string()) },
  (ctx, args) => {
    doPlant(ctx, senderSubject(ctx), '', args.x, args.y, args.kind ?? 'tree');
  }
);

export const clear = spacetimedb.reducer(
  { x: t.i32(), y: t.i32() },
  (ctx, args) => {
    doClear(ctx, senderSubject(ctx), '', args.x, args.y);
  }
);

// Presence includes the live roster and mouse cursors. Subject is derived from
// the caller identity to prevent spoofing. Scope is the colony id, so presence is
// per-colony. cx/cy are fractional tile coordinates.

export const presence_heartbeat = spacetimedb.reducer(
  {
    scope: t.string(),
    name: t.string(),
    role: t.string(),
    color: t.string(),
    cx: t.f64(),
    cy: t.f64(),
    onGrid: t.bool(),
  },
  (ctx, args) => {
    const scope = args.scope.trim();
    const name = args.name.trim();
    const role = args.role.trim();
    if (!scope || scope.length > PRESENCE_SCOPE_MAX) {
      throw new SenderError('presence.invalid_scope');
    }
    if (!name || name.length > PRESENCE_NAME_MAX) {
      throw new SenderError('presence.invalid_name');
    }
    if (!role || role.length > PRESENCE_ROLE_MAX || !PRESENCE_ROLES.has(role)) {
      throw new SenderError('presence.invalid_role');
    }
    if (!PRESENCE_COLOR_PATTERN.test(args.color)) {
      throw new SenderError('presence.invalid_color');
    }
    if (
      !Number.isFinite(args.cx) ||
      !Number.isFinite(args.cy) ||
      args.cx < -1 ||
      args.cx > COLONY_WIDTH + 1 ||
      args.cy < -1 ||
      args.cy > COLONY_HEIGHT + 1
    ) {
      throw new SenderError('presence.invalid_cursor');
    }
    upsertPresence(ctx, {
      scope,
      subject: senderSubject(ctx),
      status: 'online',
      activity: role,
      payloadJson: JSON.stringify({
        name,
        role,
        color: args.color.toLowerCase(),
        cx: args.cx,
        cy: args.cy,
        onGrid: args.onGrid,
      }),
      ttlSeconds: PRESENCE_TTL_SECONDS,
    });
  }
);

export const presence_leave = spacetimedb.reducer(
  { scope: t.string() },
  (ctx, args) => {
    removePresence(ctx, args.scope.trim(), senderSubject(ctx));
  }
);

export const colony_sweep = spacetimedb.reducer(
  { onSchedule: colonySweepTick },
  { arg: colonySweepTick.rowType },
  ctx => {
    runPresenceSweep(
      ctx,
      ctx.db.presenceEntry.expiresAt.filter(
        new Range(undefined, { tag: 'included', value: ctx.timestamp })
      )
    );
  }
);

// Reads. world, world_event, and presence_entry are public tables the
// client subscribes to with a WHERE on the colony id. The grid submodule's
// tables are reached through these public projection views, filtered by
// grid_id. A holder learns the colony id (owner subject) from verifyApiKey,
// then the grid id from the world row.

function allGridIds(ctx: ReadCtx): Set<bigint> {
  const ids = new Set<bigint>();
  for (const w of ctx.db.world.iter()) ids.add(w.gridId);
  return ids;
}

export const colonyGrid = spacetimedb.view(
  { name: 'colony_grid', public: true },
  t.array(gridSubmodule.grid.rowType),
  ctx => {
    const out = [];
    for (const id of allGridIds(ctx)) {
      const g = ctx.db.grid.grid.id.find(id);
      if (g) out.push(g);
    }
    return out;
  }
);

export const colonyCells = spacetimedb.view(
  { name: 'colony_cells', public: true },
  t.array(gridSubmodule.cellState.rowType),
  ctx => {
    const out = [];
    for (const id of allGridIds(ctx)) {
      for (const c of ctx.db.grid.cellState.gridId.filter(id)) out.push(c);
    }
    return out;
  }
);

export const colonyEntities = spacetimedb.view(
  { name: 'colony_entities', public: true },
  t.array(gridSubmodule.gridEntity.rowType),
  ctx => {
    const out = [];
    for (const id of allGridIds(ctx)) {
      for (const e of ctx.db.grid.gridEntity.gridId.filter(id)) out.push(e);
    }
    return out;
  }
);

export const myAccessKeys = spacetimedb.view(
  { name: 'my_access_keys', public: true },
  t.array(accessKeySummary.rowType),
  ctx => {
    const subject = senderSubject(ctx);
    return [...ctx.db.accessKeySummary.ownerSubject.filter(subject)];
  }
);

export const create_access_key = spacetimedb.procedure(
  {
    name: t.string(),
    scopesJson: t.string(),
    metadataJson: t.option(t.string()),
    expiresInSeconds: t.option(t.u32()),
    keyPrefix: t.option(t.string()),
  },
  apiKeys.apiKeyCreateResult,
  (ctx, args) =>
    ctx.withTx(tx => {
      const result = apiKeys.createApiKey(tx.as.apiKeys, {
        ownerSubject: senderSubject(ctx),
        name: args.name,
        scopesJson: args.scopesJson,
        metadataJson: args.metadataJson,
        expiresInSeconds: args.expiresInSeconds,
        keyPrefix: args.keyPrefix,
      });
      mirrorAccessKey(tx, result);
      return result;
    })
);

export const rotate_access_key = spacetimedb.procedure(
  {
    keyId: t.string(),
    expiresInSeconds: t.option(t.u32()),
    keyPrefix: t.option(t.string()),
  },
  apiKeys.apiKeyCreateResult,
  (ctx, args) =>
    ctx.withTx(tx => {
      const result = apiKeys.rotateApiKey(tx.as.apiKeys, {
        keyId: args.keyId,
        ownerSubject: senderSubject(ctx),
        expiresInSeconds: args.expiresInSeconds,
        keyPrefix: args.keyPrefix,
      });
      mirrorAccessKey(tx, result);
      return result;
    })
);

export const revoke_access_key = spacetimedb.reducer(
  { keyId: t.string() },
  (ctx, args) => {
    const ownerSubject = senderSubject(ctx);
    apiKeys.revokeApiKey(ctx.as.apiKeys, { keyId: args.keyId, ownerSubject });
    const row = ctx.db.accessKeySummary.keyId.find(args.keyId);
    if (!row || row.ownerSubject !== ownerSubject) return;
    ctx.db.accessKeySummary.keyId.update({
      ...row,
      status: apiKeys.ApiKeyStatus.Revoked,
      revokedAt: ctx.timestamp,
    });
  }
);

// Scoped HTTP routes. A share-key holder calls these with the key as a
// bearer token; verifyApiKey checks the scope and resolves the colony owner.

export const colonySnapshot = spacetimedb.httpHandler((ctx, req) =>
  handleAuthedWorldAction(
    ctx,
    req,
    SCOPE_VIEW,
    'snapshot',
    (tx, ownerSubject, keyPrefix, scopesJson) => {
      const snapshot = readWorldSnapshot(tx, ownerSubject);
      insertEvent(
        tx,
        ownerSubject,
        keyPrefix,
        'snapshot',
        true,
        'allowed',
        'Snapshot read'
      );
      // The holder learns which world it is (owner subject + grid id) and what
      // this key can do (scopes) in one call, so it can subscribe and enable
      // only the allowed tools.
      return { ...snapshot, ownerSubject, scopesJson };
    }
  )
);

export const colonyTerraform = spacetimedb.httpHandler((ctx, req) =>
  handleAuthedWorldAction(
    ctx,
    req,
    SCOPE_TERRAFORM,
    'terraform',
    (tx, ownerSubject, keyPrefix) => {
      const body = asObject(safeJson(req));
      return doTerraform(
        tx,
        ownerSubject,
        keyPrefix,
        asI32(body.x, 'x'),
        asI32(body.y, 'y'),
        asString(body.terrain, 'terrain')
      );
    }
  )
);

export const colonyBuild = spacetimedb.httpHandler((ctx, req) =>
  handleAuthedWorldAction(
    ctx,
    req,
    SCOPE_BUILD,
    'build',
    (tx, ownerSubject, keyPrefix) => {
      const body = asObject(safeJson(req));
      return doBuild(
        tx,
        ownerSubject,
        keyPrefix,
        asI32(body.x, 'x'),
        asI32(body.y, 'y'),
        asString(body.kind, 'kind'),
        asOptionalString(body.label)
      );
    }
  )
);

export const colonyUnbuild = spacetimedb.httpHandler((ctx, req) =>
  handleAuthedWorldAction(
    ctx,
    req,
    SCOPE_BUILD,
    'unbuild',
    (tx, ownerSubject, keyPrefix) => {
      const body = asObject(safeJson(req));
      return doUnbuild(
        tx,
        ownerSubject,
        keyPrefix,
        asI32(body.x, 'x'),
        asI32(body.y, 'y')
      );
    }
  )
);

export const colonyPlant = spacetimedb.httpHandler((ctx, req) =>
  handleAuthedWorldAction(
    ctx,
    req,
    SCOPE_PLANT,
    'plant',
    (tx, ownerSubject, keyPrefix) => {
      const body = asObject(safeJson(req));
      return doPlant(
        tx,
        ownerSubject,
        keyPrefix,
        asI32(body.x, 'x'),
        asI32(body.y, 'y'),
        asOptionalString(body.kind) ?? 'tree'
      );
    }
  )
);

export const colonyClear = spacetimedb.httpHandler((ctx, req) =>
  handleAuthedWorldAction(
    ctx,
    req,
    SCOPE_PLANT,
    'clear',
    (tx, ownerSubject, keyPrefix) => {
      const body = asObject(safeJson(req));
      return doClear(
        tx,
        ownerSubject,
        keyPrefix,
        asI32(body.x, 'x'),
        asI32(body.y, 'y')
      );
    }
  )
);

export const router = spacetimedb.httpRouter(
  new Router()
    .get('/api/colony/snapshot', colonySnapshot)
    .post('/api/colony/terraform', colonyTerraform)
    .post('/api/colony/build', colonyBuild)
    .post('/api/colony/unbuild', colonyUnbuild)
    .post('/api/colony/plant', colonyPlant)
    .post('/api/colony/clear', colonyClear)
);
