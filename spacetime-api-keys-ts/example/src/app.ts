import {
  DbConnection,
  tables,
  type ErrorContext,
} from './codegen/app/index.ts';
import { parseShareKey, shareKeyFromHash } from './share-key';

import {
  WIDTH,
  HEIGHT,
  TOKEN_PREFIX,
  NAME_KEY,
  COLOR_KEY,
  TILE_SIZE,
  PAD,
  MIN_ZOOM,
  MAX_ZOOM,
  ZOOM_STEP,
  HEARTBEAT_MS,
  KEEPALIVE_MS,
  SCOPE_VIEW,
  SCOPE_TERRAFORM,
  SCOPE_BUILD,
  SCOPE_PLANT,
  ROLES,
  STRUCT_GLYPH,
  TOOLS,
  REMOVE_GLYPH,
  CLIENT_NATURE,
  TREE_SVGS,
  BOULDER_SVGS,
  variantIndex,
  ENTITY_SVG,
  clamp,
  colorFor,
  paintLinePoints,
  parsePresencePayload,
  parseScopes,
  permissionsFor,
  roleLabel,
  safePresenceColor,
  stepDirection,
  toolAllowedFor,
  worldPixelSize,
  type AccessMode,
  type ServerConfig,
  type Grid,
  type CellState,
  type GridEntity,
  type WorldEvent,
  type PresenceEntry,
  type ApiKeySummary,
  type ToolGroup,
  type Tool,
} from './model';

let conn: DbConnection | null = null;
let config: ServerConfig | null = null;
let identityHex = '';

// mode + resolved colony
let mode: AccessMode = 'owner';
let holderKey = '';
let colonyId = '';
let gridId = 0n;
let myScopes: string[] = [];
let selectedTool = 'soil';
let subscribed = false;
// The last road placed in the current stroke, so roads connect along the
// direction you draw. This keeps connections intentional.
let lastRoad: { x: number; y: number } | null = null;

let reconnectTimer: number | null = null;
let controlsWired = false;
let viewScale = 1;
let viewX = 0;
let viewY = 0;
let viewInitialized = false;
let lastGridWidth = 0;
let lastGridHeight = 0;
let isPanning = false;
let panStartClientX = 0;
let panStartClientY = 0;
let panStartViewX = 0;
let panStartViewY = 0;
let panMoved = false;
// Left-drag paints the current tool across tiles (draw roads, brush terrain).
let isPainting = false;
let paintRemove = false;
let lastPaintTile: { x: number; y: number } | null = null;

// presence
let myName = '';
let myColor = '';
let lastBeatAt = 0;
let beatTimer: number | null = null;
let cursor = { cx: 0, cy: 0, onGrid: false };
let keepaliveTimer: number | null = null;

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el;
}

function escapeHtml(value: string): string {
  return value
    .replaceAll('&', '&amp;')
    .replaceAll('<', '&lt;')
    .replaceAll('>', '&gt;')
    .replaceAll('"', '&quot;');
}

function toast(message: string, kind: 'ok' | 'error' = 'ok'): void {
  const el = $('toast');
  el.textContent = message;
  el.className = `toast show ${kind === 'error' ? 'error' : ''}`;
  window.setTimeout(() => el.classList.remove('show'), 2800);
}

function tokenKey(): string {
  return `${TOKEN_PREFIX}.${config?.stdbUri ?? 'unknown'}.${config?.database ?? 'unknown'}`;
}

function requireConn(): DbConnection {
  if (!conn) throw new Error('stdb.disconnected');
  return conn;
}

function grids(): Grid[] {
  return [...requireConn().db.colonyGrid.iter()].filter(g => g.id === gridId);
}
function cells(): CellState[] {
  return [...requireConn().db.colonyCells.iter()].filter(
    c => c.gridId === gridId
  );
}
function entities(): GridEntity[] {
  return [...requireConn().db.colonyEntities.iter()].filter(
    e => e.gridId === gridId
  );
}
function events(): WorldEvent[] {
  return [...requireConn().db.worldEvent.iter()].filter(
    e => e.ownerSubject === colonyId
  );
}
function presenceRows(): PresenceEntry[] {
  return [...requireConn().db.presenceEntry.iter()].filter(
    p => p.scope === colonyId
  );
}
function apiKeyRows(): ApiKeySummary[] {
  return [...requireConn().db.myAccessKeys.iter()];
}

function myRole(): string {
  return mode === 'owner' ? 'Owner' : roleLabel(myScopes);
}

function canTerraform(): boolean {
  return permissionsFor(mode, myScopes).terraform;
}
function canBuild(): boolean {
  return permissionsFor(mode, myScopes).build;
}
function canPlant(): boolean {
  return permissionsFor(mode, myScopes).plant;
}
function toolAllowed(tool: Tool): boolean {
  return toolAllowedFor(mode, myScopes, tool);
}

// Colors and names
function loadName(): string {
  const stored = localStorage.getItem(NAME_KEY);
  if (stored) return stored;
  const suffix =
    identityHex.slice(-4) || Math.floor(Math.random() * 9000 + 1000).toString();
  return `Settler-${suffix}`;
}
function loadColor(): string {
  return safePresenceColor(
    localStorage.getItem(COLOR_KEY),
    colorFor(identityHex)
  );
}

async function loadConfig(): Promise<ServerConfig> {
  const res = await fetch('/api/config');
  if (!res.ok) throw new Error(`/api/config returned ${res.status}`);
  return (await res.json()) as ServerConfig;
}

function connect(cfg: ServerConfig): Promise<DbConnection> {
  return new Promise((resolve, reject) => {
    let retriedWithoutToken = false;
    const start = (token: string | undefined) => {
      let builder = DbConnection.builder()
        .withUri(cfg.stdbUri)
        .withDatabaseName(cfg.database);
      if (token) builder = builder.withToken(token);
      builder
        .onConnect((c, identity, nextToken) => {
          conn = c;
          identityHex =
            typeof identity.toHexString === 'function'
              ? identity.toHexString()
              : String(identity);
          if (nextToken) localStorage.setItem(tokenKey(), nextToken);
          resolve(c);
        })
        .onDisconnect((_ctx, err) => {
          conn = null;
          subscribed = false;
          setStatus(err?.message ?? 'Disconnected');
          scheduleReconnect();
        })
        .onConnectError((_ctx, err) => {
          if (token && !retriedWithoutToken) {
            retriedWithoutToken = true;
            localStorage.removeItem(tokenKey());
            start(undefined);
            return;
          }
          reject(err);
        })
        .build();
    };
    start(localStorage.getItem(tokenKey()) ?? undefined);
  });
}

function setStatus(message: string): void {
  const el = $('connChip');
  const connected = message === 'Connected';
  el.dataset.state = connected ? 'connected' : 'connecting';
  el.hidden = connected;
  el.innerHTML = `<span class="dot"></span>${escapeHtml(connected ? '' : message.toLowerCase())}`;
}

function scheduleReconnect(): void {
  if (reconnectTimer) return;
  reconnectTimer = window.setTimeout(() => {
    reconnectTimer = null;
    run().catch(err => {
      console.error(err);
      setStatus(err instanceof Error ? err.message : String(err));
      scheduleReconnect();
    });
  }, 2000);
}

// Subscriptions. Owner and holder subscribe to the same colony by id;
// only the id differs. Reads are public-by-colony; writes are gated.

function subscribeAll(): void {
  if (subscribed) return;
  subscribed = true;
  const c = requireConn();
  c.subscriptionBuilder()
    .onApplied(() => renderWorld())
    .onError((ctx: ErrorContext) =>
      console.error('subscription error', ctx.event)
    )
    .subscribe([
      tables.world.where(row => row.ownerSubject.eq(colonyId)),
      tables.worldEvent.where(row => row.ownerSubject.eq(colonyId)),
      tables.colonyGrid.where(row => row.id.eq(gridId)),
      tables.colonyCells.where(row => row.gridId.eq(gridId)),
      tables.colonyEntities.where(row => row.gridId.eq(gridId)),
      tables.presenceEntry.where(row => row.scope.eq(colonyId)),
      ...(mode === 'owner' ? [tables.myAccessKeys] : []),
    ]);

  c.db.world.onInsert(() => renderWorld());
  c.db.world.onUpdate(() => renderWorld());
  c.db.world.onDelete(() => renderWorld());
  c.db.colonyGrid.onInsert(() => renderWorld());
  c.db.colonyGrid.onUpdate(() => renderWorld());
  c.db.colonyGrid.onDelete(() => renderWorld());
  c.db.colonyCells.onInsert(() => renderWorld());
  c.db.colonyCells.onUpdate(() => renderWorld());
  c.db.colonyCells.onDelete(() => renderWorld());
  c.db.colonyEntities.onInsert(() => renderWorld());
  c.db.colonyEntities.onUpdate(() => renderWorld());
  c.db.colonyEntities.onDelete(() => renderWorld());
  c.db.worldEvent.onInsert(() => renderWorld());
  c.db.worldEvent.onUpdate(() => renderWorld());
  c.db.worldEvent.onDelete(() => renderWorld());
  c.db.myAccessKeys.onInsert(() => renderWorld());
  c.db.myAccessKeys.onUpdate(() => renderWorld());
  c.db.myAccessKeys.onDelete(() => renderWorld());
  c.db.presenceEntry.onInsert(() => renderPresence());
  c.db.presenceEntry.onUpdate(() => renderPresence());
  c.db.presenceEntry.onDelete(() => renderPresence());
}

// Viewport geometry, pan, and zoom

function terrainFor(x: number, y: number): string {
  return cells().find(c => c.x === x && c.y === y)?.terrain ?? 'regolith';
}
function entityAt(x: number, y: number): GridEntity | undefined {
  return entities().find(e => e.x === x && e.y === y);
}
function applyViewportTransform(): void {
  $('gridStage').style.transform =
    `translate(${viewX}px, ${viewY}px) scale(${viewScale})`;
  $('zoomLabel').textContent = `${Math.round(viewScale * 100)}%`;
}

function resetViewport(
  width = grids()[0]?.width ?? WIDTH,
  height = grids()[0]?.height ?? HEIGHT
): void {
  const bounds = $('gridViewport').getBoundingClientRect();
  const world = worldPixelSize(width, height);
  const topMargin = 76;
  const bottomMargin = 116;
  const sideMargin = 28;
  const fitScale = Math.min(
    (bounds.width - sideMargin * 2) / world.width,
    (bounds.height - topMargin - bottomMargin) / world.height
  );
  viewScale = clamp(Math.min(2.0, fitScale), MIN_ZOOM, MAX_ZOOM);
  viewX = (bounds.width - world.width * viewScale) / 2;
  viewY =
    topMargin +
    Math.max(
      0,
      (bounds.height - topMargin - bottomMargin - world.height * viewScale) / 2
    );
  viewInitialized = true;
  applyViewportTransform();
}

function zoomViewport(
  nextScale: number,
  clientX?: number,
  clientY?: number
): void {
  const rect = $('gridViewport').getBoundingClientRect();
  const anchorX = (clientX ?? rect.left + rect.width / 2) - rect.left;
  const anchorY = (clientY ?? rect.top + rect.height / 2) - rect.top;
  const clamped = clamp(nextScale, MIN_ZOOM, MAX_ZOOM);
  const worldX = (anchorX - viewX) / viewScale;
  const worldY = (anchorY - viewY) / viewScale;
  viewScale = clamped;
  viewX = anchorX - worldX * viewScale;
  viewY = anchorY - worldY * viewScale;
  applyViewportTransform();
}

// Screen point to fractional tile coordinates, accounting for pan + zoom.
function pointerToTile(
  clientX: number,
  clientY: number
): { cx: number; cy: number; onGrid: boolean } {
  const rect = $('gridViewport').getBoundingClientRect();
  const worldX = (clientX - rect.left - viewX) / viewScale;
  const worldY = (clientY - rect.top - viewY) / viewScale;
  const cx = (worldX - PAD) / TILE_SIZE;
  const cy = (worldY - PAD) / TILE_SIZE;
  const width = grids()[0]?.width ?? WIDTH;
  const height = grids()[0]?.height ?? HEIGHT;
  const onGrid = cx >= 0 && cy >= 0 && cx < width && cy < height;
  return { cx, cy, onGrid };
}

function tileAt(x: number, y: number): HTMLElement | null {
  return $('worldGrid').querySelector<HTMLElement>(
    `.tile[data-x="${x}"][data-y="${y}"]`
  );
}

function flashTile(
  x: number,
  y: number,
  kind: 'allow' | 'deny',
  denyText = 'DENIED'
): void {
  const tile = tileAt(x, y);
  if (!tile) return;
  const cls = kind === 'allow' ? 'flash-allow' : 'flash-deny';
  tile.classList.remove('flash-allow', 'flash-deny');
  void tile.offsetWidth;
  tile.classList.add(cls);
  window.setTimeout(() => tile.classList.remove(cls), 620);
  if (kind === 'deny') {
    const pop = document.createElement('span');
    pop.className = 'deny-pop';
    pop.textContent = denyText;
    tile.appendChild(pop);
    window.setTimeout(() => pop.remove(), 1000);
  }
}

// World rendering. Presence is rendered separately.

// A road renders exactly the arms in its own stored mask. Connections are
// written to both roads when they are drawn/dragged together, so there is no
// mirror guessing: a road only links where you explicitly drew a link.
function entitySpan(entity: GridEntity): string {
  if (entity.kind === 'road') {
    const own = entity.label ?? '';
    const arms = ['n', 's', 'e', 'w'].filter(d => own.includes(d));
    return `<span class="entity road" title="road">${arms.map(a => `<i class="road-arm ${a}"></i>`).join('')}<i class="road-center"></i></span>`;
  }
  if (entity.kind === 'tree')
    return `<span class="entity tree" title="tree">${TREE_SVGS[variantIndex(entity.x, entity.y, TREE_SVGS.length)]}</span>`;
  if (entity.kind === 'boulder')
    return `<span class="entity boulder" title="boulder">${BOULDER_SVGS[variantIndex(entity.x, entity.y, BOULDER_SVGS.length)]}</span>`;
  const svg = ENTITY_SVG[entity.kind];
  if (svg)
    return `<span class="entity ${escapeHtml(entity.kind)}" title="${escapeHtml(entity.kind)}">${svg}</span>`;
  return `<span class="entity ${escapeHtml(entity.kind)}" title="${escapeHtml(entity.label ?? entity.kind)}"></span>`;
}

function renderGrid(): void {
  const grid = grids()[0];
  const width = grid?.width ?? WIDTH;
  const height = grid?.height ?? HEIGHT;
  const worldGrid = $('worldGrid');
  worldGrid.style.setProperty('--cols', String(width));
  worldGrid.style.setProperty('--tile', `${TILE_SIZE}px`);
  const entityRows = entities();
  let html = '';
  for (let y = 0; y < height; y++) {
    for (let x = 0; x < width; x++) {
      const terrain = terrainFor(x, y);
      const entity = entityRows.find(e => e.x === x && e.y === y);
      html += `<button class="tile ${terrain}" data-x="${x}" data-y="${y}" aria-label="${terrain} tile ${x},${y}" title="${terrain} ${x},${y}">${entity ? entitySpan(entity) : ''}</button>`;
    }
  }
  worldGrid.innerHTML = html;
  // Placement is handled on the viewport (pointer down + drag), not per-tile,
  // so a click-drag can paint/draw a path across tiles.
  if (
    !viewInitialized ||
    width !== lastGridWidth ||
    height !== lastGridHeight
  ) {
    lastGridWidth = width;
    lastGridHeight = height;
    resetViewport(width, height);
  } else {
    applyViewportTransform();
  }
}

function renderToolbar(): void {
  if (mode === 'holder' && !canTerraform() && !canBuild() && !canPlant()) {
    $('toolbar').innerHTML =
      '<span class="toolbar-empty">View only. This key cannot change the colony.</span>';
    return;
  }
  const groups: Array<{ key: ToolGroup; tools: Tool[] }> = [
    { key: 'surface', tools: TOOLS.filter(t => t.group === 'surface') },
    { key: 'structure', tools: TOOLS.filter(t => t.group === 'structure') },
    { key: 'nature', tools: TOOLS.filter(t => t.group === 'nature') },
    { key: 'remove', tools: TOOLS.filter(t => t.group === 'remove') },
  ];
  let html = '';
  for (const g of groups) {
    const inner = g.tools
      .map(tool => {
        const allowed = toolAllowed(tool);
        const face =
          tool.group === 'surface'
            ? `<span class="swatch tile ${tool.kind}" style="--tile:22px"></span>`
            : tool.group === 'remove'
              ? `<span class="glyph">${REMOVE_GLYPH}</span>`
              : `<span class="glyph">${STRUCT_GLYPH[tool.kind ?? ''] ?? tool.label}</span>`;
        return `<button class="tool ${selectedTool === tool.id ? 'on' : ''}" data-tool="${tool.id}" ${allowed ? '' : 'disabled'} title="${tool.label}">${face}<span class="name">${tool.label}</span></button>`;
      })
      .join('');
    html += `<div class="tool-group">${inner}</div>`;
  }
  const bar = $('toolbar');
  bar.innerHTML = html;
  bar.querySelectorAll<HTMLButtonElement>('[data-tool]').forEach(btn => {
    btn.addEventListener('click', () => {
      selectedTool = btn.dataset.tool ?? selectedTool;
      lastRoad = null;
      renderToolbar();
    });
  });
}

function renderRoleBanner(): void {
  const banner = $('roleBanner');
  if (mode === 'owner') {
    banner.hidden = true;
    return;
  }
  banner.hidden = false;
  const role = myRole();
  const viewOnly = role === 'Viewer';
  banner.className = `role-banner glass ${viewOnly ? 'view-only' : ''}`;
  banner.innerHTML = `<span class="swatch" style="color:${myColor}"></span>Joined as <b>${escapeHtml(role)}</b>${viewOnly ? ' (view only)' : ''}`;
}

function renderFeed(): void {
  const ordered = events().sort((a, b) => {
    const av = a.createdAt.microsSinceUnixEpoch,
      bv = b.createdAt.microsSinceUnixEpoch;
    return av > bv ? -1 : av < bv ? 1 : 0;
  });
  $('eventFeed').innerHTML = ordered.length
    ? ordered
        .map(
          row => `
    <article class="event ${row.allowed ? '' : 'denied'}">
      <b>${escapeHtml(row.message)}</b>
      <small>${escapeHtml(row.action)} | ${row.allowed ? 'allowed' : escapeHtml(row.reason)}${row.keyPrefix ? ` | ${escapeHtml(row.keyPrefix)}` : ''}</small>
    </article>`
        )
        .join('')
    : '<p class="feed-empty">Nothing yet. Start building.</p>';
}

function renderKeys(): void {
  if (mode !== 'owner') return;
  const keys = apiKeyRows()
    .filter(k => k.ownerSubject === colonyId)
    .sort((a, b) => a.name.localeCompare(b.name));
  const active = keys.filter(k => k.status.tag === 'Active');
  if (!active.length) {
    $('keyList').innerHTML =
      '<p class="key-empty">No share links yet. Pick a role above and create one.</p>';
    return;
  }
  $('keyList').innerHTML = active
    .map(key => {
      const scopes = parseScopes(key.scopesJson);
      return `
      <article class="key-card" data-key="${escapeHtml(key.keyId)}">
        <div class="key-top">
          <b>${escapeHtml(key.name)}</b>
          <span class="key-role">${escapeHtml(roleLabel(scopes))}</span>
        </div>
        <div class="key-meta">${escapeHtml(key.prefix)}</div>
        <div class="key-actions">
          <button data-rotate="${escapeHtml(key.keyId)}">New link</button>
          <button data-revoke="${escapeHtml(key.keyId)}">Revoke</button>
        </div>
      </article>`;
    })
    .join('');
  $('keyList')
    .querySelectorAll<HTMLButtonElement>('[data-rotate]')
    .forEach(b =>
      b.addEventListener('click', () => void rotateKey(b.dataset.rotate ?? ''))
    );
  $('keyList')
    .querySelectorAll<HTMLButtonElement>('[data-revoke]')
    .forEach(b =>
      b.addEventListener('click', () => void revokeKey(b.dataset.revoke ?? ''))
    );
}

function renderWorld(): void {
  renderGrid();
  renderToolbar();
  renderRoleBanner();
  renderFeed();
  renderKeys();
  renderPresence();
}

function renderPresence(): void {
  const people = presenceRows();
  // roster
  const rosterHtml = people.length
    ? people
        .map(p => {
          const payload = parsePresencePayload(p.payloadJson);
          const isMe = p.subject === identityHex;
          const color = safePresenceColor(payload.color, colorFor(p.subject));
          return `<div class="roster-row"><span class="swatch" style="color:${color}"></span><b>${escapeHtml(payload.name || 'Someone')}${isMe ? ' (you)' : ''}</b><small>${escapeHtml(payload.role || '')}</small></div>`;
        })
        .join('')
    : '<p class="roster-empty">Nobody here yet.</p>';
  $('roster').innerHTML = rosterHtml;
  $('rosterCount').textContent = String(people.length || 1);

  // live cursors (world-space, so they pan/zoom with the map)
  const layer = $('cursorLayer');
  let html = '';
  for (const p of people) {
    if (p.subject === identityHex) continue;
    const payload = parsePresencePayload(p.payloadJson);
    if (!payload.onGrid) continue;
    const color = safePresenceColor(payload.color, colorFor(p.subject));
    const left = PAD + payload.cx * TILE_SIZE;
    const top = PAD + payload.cy * TILE_SIZE;
    html += `<div class="cursor" style="left:${left}px;top:${top}px">
      <svg viewBox="0 0 24 24" fill="${color}" stroke="rgba(0,0,0,.5)" stroke-width="1"><path d="M5 3l14 8-6 1.5L10 20z"/></svg>
      <span class="tag" style="background:${color}">${escapeHtml(payload.name || 'Someone')}</span>
    </div>`;
  }
  layer.innerHTML = html;
}

// Tool application

async function applyTool(x: number, y: number): Promise<void> {
  const tool = TOOLS.find(t => t.id === selectedTool);
  if (!tool) return;
  if (tool.group === 'remove') {
    await removeAt(x, y);
    return;
  }
  if (!toolAllowed(tool)) {
    flashTile(x, y, 'deny', 'NO ACCESS');
    toast(
      `This key cannot ${tool.group === 'surface' ? 'terraform' : tool.group === 'structure' ? 'build' : 'plant'} here.`,
      'error'
    );
    return;
  }

  try {
    if (tool.group === 'surface') {
      await mutate('terraform', { x, y, terrain: tool.kind });
      lastRoad = null;
    } else if (tool.group === 'structure') {
      if (tool.kind === 'road') {
        // Link the new tile and the preceding tile in this stroke, on both
        // sides. Nothing else (perpendicular neighbours) is touched.
        const prev = lastRoad;
        const toPrev = prev ? stepDirection(x, y, prev.x, prev.y) : '';
        lastRoad = { x, y };
        await mutate('build', { x, y, kind: 'road', label: toPrev });
        if (prev && toPrev) {
          await mutate('build', {
            x: prev.x,
            y: prev.y,
            kind: 'road',
            label: stepDirection(prev.x, prev.y, x, y),
          });
        }
      } else {
        await mutate('build', { x, y, kind: tool.kind });
        lastRoad = null;
      }
    } else {
      await mutate('plant', { x, y, kind: tool.kind });
      lastRoad = null;
    }
    flashTile(x, y, 'allow');
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const denied = /scope|denied|forbidden|401|403/i.test(message);
    const occupied = /occupied|nothing/i.test(message);
    flashTile(
      x,
      y,
      'deny',
      denied ? 'NO ACCESS' : occupied ? 'BLOCKED' : 'FAILED'
    );
    if (denied) toast('This key cannot do that here.', 'error');
    else if (!occupied) toast(message, 'error');
  }
}

// Remove whatever is on a tile: an object (structure or nature), or if empty,
// reset the surface to bare. Used by the Remove tool and by ctrl/cmd-click.
async function removeAt(x: number, y: number): Promise<void> {
  lastRoad = null;
  const ent = entityAt(x, y);
  try {
    if (ent && CLIENT_NATURE.has(ent.kind)) {
      if (!canPlant()) throw new Error('scope_denied');
      await mutate('clear', { x, y });
    } else if (ent) {
      if (!canBuild()) throw new Error('scope_denied');
      await mutate('unbuild', { x, y });
    } else {
      if (!canTerraform()) throw new Error('scope_denied');
      await mutate('terraform', { x, y, terrain: 'regolith' });
    }
    flashTile(x, y, 'allow');
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    const denied = /scope|denied|forbidden|401|403/i.test(message);
    flashTile(x, y, 'deny', denied ? 'NO ACCESS' : 'FAILED');
    if (denied) toast('This key cannot remove that here.', 'error');
  }
}

// Owner mutates via native reducers; holder via scoped HTTP routes.
function requiredNumber(body: Record<string, unknown>, field: string): number {
  const value = body[field];
  if (typeof value !== 'number' || !Number.isFinite(value))
    throw new Error(`invalid_${field}`);
  return value;
}

function requiredString(body: Record<string, unknown>, field: string): string {
  const value = body[field];
  if (typeof value !== 'string') throw new Error(`invalid_${field}`);
  return value;
}

async function mutate(
  action: string,
  body: Record<string, unknown>
): Promise<void> {
  if (mode === 'owner') {
    const r = requireConn().reducers;
    const x = requiredNumber(body, 'x');
    const y = requiredNumber(body, 'y');
    if (action === 'terraform')
      r.terraform({ x, y, terrain: requiredString(body, 'terrain') });
    else if (action === 'build') {
      const label = body.label;
      if (label !== undefined && typeof label !== 'string')
        throw new Error('invalid_label');
      r.build({ x, y, kind: requiredString(body, 'kind'), label });
    } else if (action === 'plant')
      r.plant({ x, y, kind: requiredString(body, 'kind') });
    else if (action === 'unbuild') r.unbuild({ x, y });
    else if (action === 'clear') r.clear({ x, y });
    else throw new Error(`unknown_action:${action}`);
    return;
  }
  await colonyRequest(`/api/colony/${action}`, body);
}

async function colonyRequest(path: string, body?: unknown): Promise<unknown> {
  const res = await fetch(path, {
    method: body === undefined ? 'GET' : 'POST',
    headers: {
      authorization: `Bearer ${holderKey}`,
      ...(body === undefined ? {} : { 'content-type': 'application/json' }),
    },
    body: body === undefined ? undefined : JSON.stringify(body),
  });
  const data = await res.json().catch(() => ({}));
  if (!res.ok) {
    const error =
      data && typeof data === 'object' && 'error' in data
        ? String((data as { error: unknown }).error)
        : `http_${res.status}`;
    if (/revoked|expired|unknown_key|invalid_key/i.test(error))
      showAccessRemoved(error);
    throw new Error(error);
  }
  return data;
}

// Owner share keys

let selectedRole = 'collaborator';

function renderRoleGrid(): void {
  $('roleGrid').innerHTML = ROLES.map(
    role => `
    <button class="role-opt ${selectedRole === role.id ? 'on' : ''}" data-role="${role.id}">
      <b>${role.name}</b><small>${role.blurb}</small>
    </button>`
  ).join('');
  $('roleGrid')
    .querySelectorAll<HTMLButtonElement>('[data-role]')
    .forEach(b => {
      b.addEventListener('click', () => {
        selectedRole = b.dataset.role ?? selectedRole;
        renderRoleGrid();
      });
    });
}

function shareLink(secret: string): string {
  return `${location.origin}${location.pathname}#key=${encodeURIComponent(secret)}`;
}

async function createKey(): Promise<void> {
  const role = ROLES.find(r => r.id === selectedRole) ?? ROLES[0];
  const input = document.getElementById(
    'keyNameInput'
  ) as HTMLInputElement | null;
  const name = input?.value.trim() || role.name;
  try {
    const result = await requireConn().procedures.createAccessKey({
      name,
      scopesJson: JSON.stringify(role.scopes),
      metadataJson: JSON.stringify({ role: role.id }),
      expiresInSeconds: undefined,
      keyPrefix: undefined,
    });
    if (input) input.value = '';
    showLink(name, result.key);
    toast(`${name} link created`);
    renderKeys();
  } catch (err) {
    toast(err instanceof Error ? err.message : String(err), 'error');
  }
}

function showLink(label: string, secret: string): void {
  const box = $('linkBox');
  const link = shareLink(secret);
  box.hidden = false;
  box.innerHTML = `
    <div class="lh">${escapeHtml(label)} link. Anyone with it gets this access. Copy it now.</div>
    <div class="link-row">
      <code class="link-code">${escapeHtml(link)}</code>
      <button class="primary" id="copyFreshLink" type="button" style="height:auto;padding:0 13px">Copy</button>
    </div>`;
  box
    .querySelector<HTMLButtonElement>('#copyFreshLink')
    ?.addEventListener('click', async () => {
      try {
        await navigator.clipboard.writeText(link);
        toast('Link copied');
      } catch {
        toast('Copy failed. Select the text manually.', 'error');
      }
    });
}

async function rotateKey(keyId: string): Promise<void> {
  if (!keyId) return;
  try {
    const result = await requireConn().procedures.rotateAccessKey({
      keyId,
      expiresInSeconds: undefined,
      keyPrefix: undefined,
    });
    showLink(result.name ?? 'New', result.key);
    toast('Replacement link issued. Previous link revoked.');
    renderKeys();
  } catch (err) {
    toast(err instanceof Error ? err.message : String(err), 'error');
  }
}

async function revokeKey(keyId: string): Promise<void> {
  if (!keyId) return;
  requireConn().reducers.revokeAccessKey({ keyId });
  toast('Access revoked');
}

// Holder access removal state

function showAccessRemoved(reason: string): void {
  const overlay = $('accessOverlay');
  const expired = /expired/i.test(reason);
  $('overlayTitle').textContent = expired ? 'Link expired' : 'Access removed';
  $('overlayText').textContent = expired
    ? 'This share link has expired. Ask the owner for a new one.'
    : 'The owner revoked this share link. Colony access is unavailable.';
  overlay.classList.add('show');
  if (keepaliveTimer) {
    window.clearInterval(keepaliveTimer);
    keepaliveTimer = null;
  }
}

// Presence heartbeats

function sendBeat(): void {
  if (!conn || !colonyId) return;
  lastBeatAt = Date.now();
  try {
    requireConn().reducers.presenceHeartbeat({
      scope: colonyId,
      name: myName,
      role: myRole(),
      color: myColor,
      cx: cursor.cx,
      cy: cursor.cy,
      onGrid: cursor.onGrid,
    });
  } catch {
    /* connection churn, keepalive will retry */
  }
}

function queueBeat(): void {
  const now = Date.now();
  const wait = HEARTBEAT_MS - (now - lastBeatAt);
  if (wait <= 0) {
    sendBeat();
    return;
  }
  if (beatTimer) return;
  beatTimer = window.setTimeout(() => {
    beatTimer = null;
    sendBeat();
  }, wait);
}

function startPresence(): void {
  sendBeat();
  if (keepaliveTimer) window.clearInterval(keepaliveTimer);
  keepaliveTimer = window.setInterval(() => {
    if (mode === 'holder') void reverify();
    sendBeat();
  }, KEEPALIVE_MS);
  window.addEventListener('beforeunload', () => {
    try {
      requireConn().reducers.presenceLeave({ scope: colonyId });
    } catch {
      /* best-effort disconnect cleanup */
    }
  });
}

// Confirm holder access and show the overlay after revocation.
async function reverify(): Promise<void> {
  if (mode !== 'holder') return;
  try {
    await colonyRequest('/api/colony/snapshot');
  } catch {
    /* colonyRequest already shows the overlay on revoke/expire */
  }
}

// Pointer-driven placement. Dragging paints across tiles.

function tileFromEvent(
  clientX: number,
  clientY: number
): { x: number; y: number } | null {
  const t = pointerToTile(clientX, clientY);
  if (!t.onGrid) return null;
  return { x: Math.floor(t.cx), y: Math.floor(t.cy) };
}
function applyAtTile(x: number, y: number, remove: boolean): void {
  if (remove) void removeAt(x, y);
  else void applyTool(x, y);
}

// Fill in every tile along a drag so a fast drag never leaves gaps (and roads
// chain tile-by-tile). Walks orthogonally so each step is adjacent to the last.
// HUD controls

function toggle(id: string, others: string[]): void {
  const panel = $(id);
  const open = panel.hidden;
  for (const o of others) $(o).hidden = true;
  panel.hidden = !open;
}

// Use the URL fragment so the bearer key is not sent in the initial HTTP request.
function joinColony(): void {
  const raw = ($('joinInput') as HTMLInputElement).value.trim();
  if (!raw) {
    toast('Paste a share link or key first', 'error');
    return;
  }
  const key = parseShareKey(raw);
  if (!key) {
    toast('Use a raw key or a share link with #key=...', 'error');
    return;
  }
  location.href = shareLink(key);
}

function wireControls(): void {
  if (controlsWired) return;
  controlsWired = true;

  const viewport = $('gridViewport');
  $('zoomOut').addEventListener('click', () =>
    zoomViewport(viewScale / ZOOM_STEP)
  );
  $('zoomIn').addEventListener('click', () =>
    zoomViewport(viewScale * ZOOM_STEP)
  );
  $('resetView').addEventListener('click', () => resetViewport());
  viewport.addEventListener(
    'wheel',
    event => {
      event.preventDefault();
      zoomViewport(
        viewScale * (event.deltaY < 0 ? ZOOM_STEP : 1 / ZOOM_STEP),
        event.clientX,
        event.clientY
      );
    },
    { passive: false }
  );
  viewport.addEventListener('contextmenu', event => event.preventDefault());
  viewport.addEventListener('mousedown', event => {
    if (event.button === 1) event.preventDefault();
  });
  viewport.addEventListener('pointerdown', event => {
    const touch = event.pointerType === 'touch';
    // Middle / right / touch drag pans. Left mouse paints the current tool.
    const wantsPan = touch || event.button === 1 || event.button === 2;
    if (wantsPan) {
      if (!touch) event.preventDefault();
      isPanning = true;
      panMoved = false;
      panStartClientX = event.clientX;
      panStartClientY = event.clientY;
      panStartViewX = viewX;
      panStartViewY = viewY;
      viewport.classList.add('panning');
      viewport.setPointerCapture(event.pointerId);
      return;
    }
    if (event.button !== 0) return;
    const tile = tileFromEvent(event.clientX, event.clientY);
    if (!tile) return;
    isPainting = true;
    paintRemove = event.ctrlKey || event.metaKey;
    lastPaintTile = tile;
    applyAtTile(tile.x, tile.y, paintRemove);
    viewport.setPointerCapture(event.pointerId);
  });
  viewport.addEventListener('pointermove', event => {
    if (isPanning) {
      const dx = event.clientX - panStartClientX,
        dy = event.clientY - panStartClientY;
      if (Math.abs(dx) > 3 || Math.abs(dy) > 3) panMoved = true;
      viewX = panStartViewX + dx;
      viewY = panStartViewY + dy;
      applyViewportTransform();
    } else if (isPainting) {
      const tile = tileFromEvent(event.clientX, event.clientY);
      if (
        tile &&
        (!lastPaintTile ||
          tile.x !== lastPaintTile.x ||
          tile.y !== lastPaintTile.y)
      ) {
        if (lastPaintTile) {
          for (const point of paintLinePoints(lastPaintTile, tile)) {
            applyAtTile(point.x, point.y, paintRemove);
          }
        } else applyAtTile(tile.x, tile.y, paintRemove);
        lastPaintTile = tile;
      }
    }
    // cursor presence
    const t = pointerToTile(event.clientX, event.clientY);
    cursor = { cx: t.cx, cy: t.cy, onGrid: t.onGrid };
    queueBeat();
  });
  viewport.addEventListener('pointerleave', () => {
    cursor = { ...cursor, onGrid: false };
    sendBeat();
  });
  const endStroke = (event: PointerEvent): void => {
    if (isPanning) {
      isPanning = false;
      viewport.classList.remove('panning');
      // A touch tap that did not pan places a single tile.
      if (event.pointerType === 'touch' && !panMoved) {
        const tile = tileFromEvent(event.clientX, event.clientY);
        if (tile) applyAtTile(tile.x, tile.y, false);
      }
    }
    // End the stroke: roads only chain within a single continuous drag, so a
    // A separate click beside the road starts a distinct stroke.
    if (isPainting) {
      isPainting = false;
      lastPaintTile = null;
      lastRoad = null;
    }
    if (viewport.hasPointerCapture(event.pointerId))
      viewport.releasePointerCapture(event.pointerId);
  };
  viewport.addEventListener('pointerup', endStroke);
  viewport.addEventListener('pointercancel', endStroke);

  $('rosterBtn').addEventListener('click', () =>
    toggle('rosterPanel', ['sharePanel', 'joinPanel', 'logDrawer'])
  );
  $('joinBtn').addEventListener('click', () =>
    toggle('joinPanel', ['sharePanel', 'rosterPanel', 'logDrawer'])
  );
  $('shareBtn').addEventListener('click', () =>
    toggle('sharePanel', ['rosterPanel', 'joinPanel', 'logDrawer'])
  );
  $('logBtn').addEventListener('click', () =>
    toggle('logDrawer', ['rosterPanel', 'sharePanel', 'joinPanel'])
  );
  const colorInput = $('myColorInput') as HTMLInputElement;
  const nameInput = $('myNameInput') as HTMLInputElement;
  colorInput.value = /^#[0-9a-fA-F]{6}$/.test(myColor) ? myColor : '#59c6d6';
  nameInput.value = myName;
  colorInput.addEventListener('input', () => {
    myColor = colorInput.value;
    localStorage.setItem(COLOR_KEY, myColor);
    sendBeat();
    renderPresence();
    renderRoleBanner();
  });
  nameInput.addEventListener('input', () => {
    myName = nameInput.value.trim() || `Settler-${identityHex.slice(-4)}`;
    localStorage.setItem(NAME_KEY, myName);
    sendBeat();
    renderPresence();
  });
  $('createKeyBtn').addEventListener('click', () => void createKey());
  $('joinSubmit').addEventListener('click', () => joinColony());
  ($('joinInput') as HTMLInputElement).addEventListener('keydown', e => {
    if (e.key === 'Enter') joinColony();
  });
  $('resetWorld').addEventListener('click', () => {
    requireConn().reducers.resetWorld({});
    toast('Colony reset');
  });
  $('clearEvents').addEventListener('click', () => {
    requireConn().reducers.clearWorldEvents({});
    toast('Log cleared');
  });

  document.addEventListener('pointerdown', event => {
    const t = event.target as HTMLElement;
    if (!t.closest('#sharePanel, #shareBtn')) $('sharePanel').hidden = true;
    if (!t.closest('#rosterPanel, #rosterBtn')) $('rosterPanel').hidden = true;
    if (!t.closest('#joinPanel, #joinBtn')) $('joinPanel').hidden = true;
    if (!t.closest('#logDrawer, #logBtn')) $('logDrawer').hidden = true;
  });
  document.addEventListener('keydown', event => {
    if (event.key === 'Escape') {
      $('sharePanel').hidden = true;
      $('rosterPanel').hidden = true;
      $('joinPanel').hidden = true;
      $('logDrawer').hidden = true;
    }
  });
}

function applyModeChrome(): void {
  // Show sharing and colony administration controls to the owner.
  $('shareBtn').hidden = mode !== 'owner';
  $('drawerFoot').style.display = mode === 'owner' ? '' : 'none';
  if (mode === 'holder') {
    $('sharePanel').hidden = true;
  }
}

async function run(): Promise<void> {
  setStatus('Connecting');
  config = await loadConfig();
  const c = await connect(config);
  conn = c;
  myName = loadName();
  myColor = loadColor();

  const urlKey = shareKeyFromHash(location.hash);
  if (urlKey) {
    mode = 'holder';
    holderKey = urlKey;
    setStatus('Opening colony');
    // Snapshot resolves which colony, its grid, and this key's scopes at once.
    const data = (await colonyRequest('/api/colony/snapshot')) as {
      result?: {
        ownerSubject?: unknown;
        world?: { ownerSubject?: unknown; gridId?: string | number | bigint };
        grid?: { id?: string | number | bigint };
        scopesJson?: unknown;
      };
    };
    const result = data.result ?? {};
    colonyId = String(result.ownerSubject ?? result.world?.ownerSubject ?? '');
    gridId = BigInt(result.world?.gridId ?? result.grid?.id ?? 0);
    myScopes = parseScopes(
      typeof result.scopesJson === 'string' ? result.scopesJson : '[]'
    );
    if (!colonyId) throw new Error('could not resolve colony');
  } else {
    mode = 'owner';
    setStatus('Preparing colony');
    const r = await requireConn().procedures.ensureWorld({});
    colonyId = String(r.ownerSubject);
    gridId = BigInt(r.gridId);
    myScopes = [SCOPE_VIEW, SCOPE_TERRAFORM, SCOPE_BUILD, SCOPE_PLANT];
  }

  if (mode === 'holder' && !toolAllowed(TOOLS[0]) && !canBuild() && !canPlant())
    selectedTool = 'regolith';
  else if (mode === 'holder') {
    // Default to a tool allowed by this key.
    if (canTerraform()) selectedTool = 'soil';
    else if (canBuild()) selectedTool = 'dome';
    else if (canPlant()) selectedTool = 'tree';
  }

  setStatus('Connected');
  applyModeChrome();
  subscribeAll();
  wireControls();
  renderRoleGrid();
  renderWorld();
  startPresence();
}

run().catch(err => {
  console.error(err);
  setStatus(err instanceof Error ? err.message : String(err));
  toast(err instanceof Error ? err.message : String(err), 'error');
});
