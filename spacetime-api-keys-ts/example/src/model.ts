export type TimestampLike = { microsSinceUnixEpoch: bigint };
export type EnumTag<T extends string = string> = { tag: T };

export type ServerConfig = { stdbUri: string; database: string };

export type World = {
  ownerSubject: string;
  gridId: bigint;
  name: string;
  updatedAt: TimestampLike;
};
export type Grid = { id: bigint; width: number; height: number };
export type CellState = {
  id: bigint;
  gridId: bigint;
  x: number;
  y: number;
  cost: number;
  terrain?: string;
};
export type GridEntity = {
  id: bigint;
  gridId: bigint;
  ownerUserId: string;
  x: number;
  y: number;
  kind: string;
  label?: string;
};
export type WorldEvent = {
  eventId: bigint;
  ownerSubject: string;
  keyPrefix: string;
  action: string;
  allowed: boolean;
  reason: string;
  message: string;
  createdAt: TimestampLike;
};
export type PresenceEntry = {
  key: string;
  scope: string;
  subject: string;
  status: string;
  payloadJson?: string;
  lastSeenAt: TimestampLike;
};
export type ApiKeySummary = {
  keyId: string;
  prefix: string;
  ownerSubject: string;
  name: string;
  scopesJson: string;
  metadataJson?: string;
  status: EnumTag<'Active' | 'Revoked'>;
  createdAt: TimestampLike;
  expiresAt?: TimestampLike;
  lastUsedAt?: TimestampLike;
  revokedAt?: TimestampLike;
};

export const WIDTH = 12;
export const HEIGHT = 8;
export const TOKEN_PREFIX = 'colony.stdb-token';
export const NAME_KEY = 'colony.name';
export const COLOR_KEY = 'colony.color';
export const TILE_SIZE = 78;
export const PAD = 26;
export const MIN_ZOOM = 0.5;
export const MAX_ZOOM = 2.6;
export const ZOOM_STEP = 1.18;
export const HEARTBEAT_MS = 80;
export const KEEPALIVE_MS = 9000;
export const PRESENCE_NAME_MAX = 64;
export const PRESENCE_ROLE_MAX = 32;

export function safePresenceColor(value: unknown, fallback = ''): string {
  return typeof value === 'string' && /^#[0-9a-fA-F]{6}$/.test(value)
    ? value.toLowerCase()
    : fallback;
}

export function safePresenceText(
  value: unknown,
  fallback: string,
  maxLength: number
): string {
  if (typeof value !== 'string') return fallback;
  const text = value.trim();
  return text ? text.slice(0, maxLength) : fallback;
}

export function safePresenceCoordinate(
  value: unknown,
  min: number,
  max: number
): number {
  return typeof value === 'number' && Number.isFinite(value)
    ? Math.min(max, Math.max(min, value))
    : 0;
}

export const SCOPE_VIEW = 'colony:view';
export const SCOPE_TERRAFORM = 'colony:terraform';
export const SCOPE_BUILD = 'colony:build';
export const SCOPE_PLANT = 'colony:plant';

export type AccessMode = 'owner' | 'holder';

export function parseScopes(json: string): string[] {
  try {
    const parsed: unknown = JSON.parse(json);
    return Array.isArray(parsed) ? parsed.map(String) : [];
  } catch {
    return [];
  }
}

export function hasScope(scopes: string[], scope: string): boolean {
  return (
    scopes.includes('*') ||
    scopes.includes('colony:*') ||
    scopes.includes(scope)
  );
}

export function roleLabel(scopes: string[]): string {
  const canTerraform = hasScope(scopes, SCOPE_TERRAFORM);
  const canBuild = hasScope(scopes, SCOPE_BUILD);
  const canPlant = hasScope(scopes, SCOPE_PLANT);
  if (canTerraform && canBuild && canPlant) return 'Collaborator';
  if (canTerraform && !canBuild && !canPlant) return 'Terraformer';
  if (canBuild && !canTerraform && !canPlant) return 'Builder';
  if (canPlant && !canTerraform && !canBuild) return 'Planter';
  if (!canTerraform && !canBuild && !canPlant) return 'Viewer';
  return 'Editor';
}

export function permissionsFor(mode: AccessMode, scopes: string[]) {
  const owner = mode === 'owner';
  return {
    terraform: owner || hasScope(scopes, SCOPE_TERRAFORM),
    build: owner || hasScope(scopes, SCOPE_BUILD),
    plant: owner || hasScope(scopes, SCOPE_PLANT),
  };
}

// Share roles: a small, honest set that maps straight onto scopes.
export const ROLES: Array<{
  id: string;
  name: string;
  scopes: string[];
  blurb: string;
}> = [
  {
    id: 'viewer',
    name: 'Viewer',
    scopes: [SCOPE_VIEW],
    blurb: 'Can look around, cannot change anything.',
  },
  {
    id: 'terraformer',
    name: 'Terraformer',
    scopes: [SCOPE_VIEW, SCOPE_TERRAFORM],
    blurb: 'Can reshape the surface.',
  },
  {
    id: 'builder',
    name: 'Builder',
    scopes: [SCOPE_VIEW, SCOPE_BUILD],
    blurb: 'Can place structures and roads.',
  },
  {
    id: 'planter',
    name: 'Planter',
    scopes: [SCOPE_VIEW, SCOPE_PLANT],
    blurb: 'Can plant and clear greenery.',
  },
  {
    id: 'collaborator',
    name: 'Collaborator',
    scopes: [SCOPE_VIEW, SCOPE_TERRAFORM, SCOPE_BUILD, SCOPE_PLANT],
    blurb: 'Can do everything.',
  },
];

export type ToolGroup = 'surface' | 'structure' | 'nature' | 'remove';
export type Tool = {
  id: string;
  group: ToolGroup;
  kind?: string;
  label: string;
};

export function toolAllowedFor(
  mode: AccessMode,
  scopes: string[],
  tool: Tool
): boolean {
  const permissions = permissionsFor(mode, scopes);
  if (tool.group === 'surface') return permissions.terraform;
  if (tool.group === 'structure') return permissions.build;
  if (tool.group === 'nature') return permissions.plant;
  return permissions.build || permissions.plant;
}

export const STRUCT_GLYPH: Record<string, string> = {
  dome: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><path d="M3 18a9 9 0 0 1 18 0z"/><path d="M2 18h20"/><path d="M12 9v9M6.5 12.5l11 0"/></svg>',
  pod: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M6 19v-7a6 6 0 0 1 12 0v7z"/><circle cx="12" cy="11" r="2"/><path d="M5 19h14"/></svg>',
  solar:
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><rect x="3" y="7" width="18" height="8" rx="1"/><path d="M7 7v8M11 7v8M15 7v8M12 15v4"/></svg>',
  road: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7"><path d="M6 3v18M18 3v18M12 5v3M12 12v3M12 19v1"/></svg>',
  tree: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="8" r="5"/><path d="M12 13v8"/></svg>',
  shrub:
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round" stroke-linejoin="round"><path d="M7 17a4 4 0 0 1 .5-7.5A4 4 0 0 1 15 8a3.5 3.5 0 0 1 2 6.5"/><path d="M12 21v-7"/></svg>',
  boulder:
    '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linejoin="round"><path d="M4 17l3-8 5-2 6 4 2 6z"/></svg>',
};
export const TOOLS: Tool[] = [
  { id: 'regolith', group: 'surface', kind: 'regolith', label: 'Bare' },
  { id: 'rock', group: 'surface', kind: 'rock', label: 'Rock' },
  { id: 'grass', group: 'surface', kind: 'grass', label: 'Grass' },
  { id: 'water', group: 'surface', kind: 'water', label: 'Water' },
  { id: 'soil', group: 'surface', kind: 'soil', label: 'Soil' },
  { id: 'dome', group: 'structure', kind: 'dome', label: 'Dome' },
  { id: 'pod', group: 'structure', kind: 'pod', label: 'Pod' },
  { id: 'solar', group: 'structure', kind: 'solar', label: 'Solar' },
  { id: 'road', group: 'structure', kind: 'road', label: 'Road' },
  { id: 'tree', group: 'nature', kind: 'tree', label: 'Tree' },
  { id: 'boulder', group: 'nature', kind: 'boulder', label: 'Boulder' },
  { id: 'remove', group: 'remove', label: 'Remove' },
];
export const REMOVE_GLYPH =
  '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.7" stroke-linecap="round"><path d="M4 7h16M9 7V5h6v2M7 7l1 13h8l1-13"/></svg>';
export const CLIENT_NATURE = new Set(['tree', 'shrub', 'boulder']);

// A biodome drawn in 3/4 isometric perspective: a gridded glass shell (latitude
// rings + meridians + top cap) on a short cylindrical base.
export const DOME_SVG = `<svg viewBox="0 0 100 96" xmlns="http://www.w3.org/2000/svg" aria-hidden="true">
  <path d="M6 71 A44 13 0 0 0 94 71 L94 79 A44 13 0 0 1 6 79 Z" fill="#aeb6bd" stroke="#20242a" stroke-width="1.6" stroke-linejoin="round"/>
  <ellipse cx="50" cy="71" rx="44" ry="13" fill="#ccd3d9" stroke="#20242a" stroke-width="1.6"/>
  <path d="M12 69 A38 38 0 0 1 88 69 Z" fill="rgba(190,238,250,0.5)" stroke="#3a63c0" stroke-width="2" stroke-linejoin="round"/>
  <path d="M12 69 A38 11 0 0 0 88 69" fill="none" stroke="#3a63c0" stroke-width="1.2"/>
  <ellipse cx="50" cy="58" rx="33" ry="8.5" fill="none" stroke="#3a63c0" stroke-width="1.1"/>
  <ellipse cx="50" cy="47" rx="25" ry="6.5" fill="none" stroke="#3a63c0" stroke-width="1.1"/>
  <ellipse cx="50" cy="36" rx="14" ry="5" fill="rgba(190,238,250,0.7)" stroke="#3a63c0" stroke-width="1.4"/>
  <path d="M50 69 L50 36" fill="none" stroke="#3a63c0" stroke-width="1.1"/>
  <path d="M18 64 Q30 47 37 38" fill="none" stroke="#3a63c0" stroke-width="1.1"/>
  <path d="M82 64 Q70 47 63 38" fill="none" stroke="#3a63c0" stroke-width="1.1"/>
  <path d="M32 68 Q41 49 44.5 36.5" fill="none" stroke="#3a63c0" stroke-width="1.1"/>
  <path d="M68 68 Q59 49 55.5 36.5" fill="none" stroke="#3a63c0" stroke-width="1.1"/>
</svg>`;

// Solid isometric objects, each sitting on a ground shadow.
export const POD_SVG = `<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><defs><linearGradient id="podg" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stop-color="#e6ecf1"/><stop offset="1" stop-color="#8a97a2"/></linearGradient></defs><ellipse cx="50" cy="86" rx="30" ry="7" fill="rgba(0,0,0,0.28)"/><path d="M28 84 V52 a22 22 0 0 1 44 0 V84 Z" fill="url(#podg)" stroke="#454f58" stroke-width="2.5" stroke-linejoin="round"/><path d="M28 70 H72" stroke="#454f58" stroke-width="1.6"/><circle cx="50" cy="47" r="9" fill="#bdf0ff" stroke="#2c8aab" stroke-width="2.2"/><circle cx="47" cy="44" r="3" fill="#ffffff" opacity="0.85"/><path d="M43 84 V75 a7 7 0 0 1 14 0 V84 Z" fill="#59646f"/></svg>`;

export const SOLAR_SVG = `<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><ellipse cx="52" cy="88" rx="24" ry="6" fill="rgba(0,0,0,0.25)"/><path d="M50 60 L50 85" stroke="#59636d" stroke-width="5"/><path d="M20 40 L66 30 L80 54 L34 64 Z" fill="#22437f" stroke="#5b86cc" stroke-width="2.4" stroke-linejoin="round"/><path d="M35.5 37 L49.5 58 M50 34.5 L64 55 M27 47 L73 42" stroke="#4066ad" stroke-width="1.5" fill="none"/></svg>`;

// A few tree and boulder variants for natural variety; the variant is chosen
// from the tile position so it is stable across renders and identical for
// everyone viewing the colony.
export const TREE_SVGS = [
  `<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><ellipse cx="50" cy="88" rx="20" ry="6" fill="rgba(0,0,0,0.25)"/><rect x="45" y="58" width="10" height="30" rx="3" fill="#6f4a29"/><rect x="45" y="58" width="4" height="30" rx="2" fill="#8a5f38"/><ellipse cx="50" cy="42" rx="27" ry="25" fill="#469a3f" stroke="#2f7a30" stroke-width="2"/><ellipse cx="42" cy="35" rx="11" ry="9" fill="#83d472" opacity="0.9"/></svg>`,
  `<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><ellipse cx="50" cy="90" rx="18" ry="5" fill="rgba(0,0,0,0.25)"/><rect x="45" y="74" width="10" height="16" rx="2" fill="#6f4a29"/><path d="M50 18 L67 48 L33 48 Z" fill="#57ac4d" stroke="#2f7a30" stroke-width="1.5" stroke-linejoin="round"/><path d="M50 36 L72 66 L28 66 Z" fill="#4b9f43" stroke="#2f7a30" stroke-width="1.5" stroke-linejoin="round"/><path d="M50 52 L77 80 L23 80 Z" fill="#3f8f3a" stroke="#2f7a30" stroke-width="1.5" stroke-linejoin="round"/></svg>`,
  `<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><ellipse cx="50" cy="88" rx="20" ry="6" fill="rgba(0,0,0,0.25)"/><rect x="46" y="60" width="8" height="28" rx="3" fill="#6f4a29"/><circle cx="40" cy="46" r="15" fill="#3f8f3a"/><circle cx="61" cy="44" r="16" fill="#4fa447"/><circle cx="50" cy="34" r="15" fill="#57ac4d"/><circle cx="45" cy="33" r="6" fill="#8bd67a" opacity="0.85"/></svg>`,
];

export const BOULDER_SVGS = [
  `<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><ellipse cx="50" cy="82" rx="27" ry="7" fill="rgba(0,0,0,0.25)"/><path d="M22 72 Q17 50 34 43 Q47 31 65 40 Q85 47 81 65 Q79 79 57 80 Q33 81 22 72 Z" fill="#8b9199" stroke="#474d53" stroke-width="2.4" stroke-linejoin="round"/><path d="M34 43 Q47 31 65 40 Q71 44 70 53 Q52 57 40 52 Q31 49 34 43 Z" fill="#aab0b7" opacity="0.75"/><path d="M57 80 Q77 77 81 65 Q83 73 76 79 Q68 82 57 80 Z" fill="#585e65" opacity="0.6"/></svg>`,
  `<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><ellipse cx="52" cy="84" rx="28" ry="7" fill="rgba(0,0,0,0.25)"/><path d="M46 74 Q40 56 54 50 Q66 44 78 54 Q86 62 80 72 Q70 80 58 79 Q48 79 46 74 Z" fill="#8b9199" stroke="#474d53" stroke-width="2.2" stroke-linejoin="round"/><path d="M20 78 Q16 66 27 62 Q37 58 44 66 Q48 72 42 78 Q32 82 24 80 Q20 80 20 78 Z" fill="#7d838a" stroke="#474d53" stroke-width="2.2" stroke-linejoin="round"/><path d="M54 50 Q66 44 78 54 Q82 58 80 63 Q66 66 56 61 Q50 56 54 50 Z" fill="#aab0b7" opacity="0.7"/></svg>`,
  `<svg viewBox="0 0 100 100" xmlns="http://www.w3.org/2000/svg" aria-hidden="true"><ellipse cx="50" cy="80" rx="30" ry="7" fill="rgba(0,0,0,0.25)"/><path d="M16 70 Q14 58 30 55 Q50 50 70 55 Q86 58 84 68 Q82 76 60 77 Q34 78 22 74 Q16 73 16 70 Z" fill="#8b9199" stroke="#474d53" stroke-width="2.3" stroke-linejoin="round"/><path d="M30 55 Q50 50 70 55 Q78 57 76 62 Q52 65 36 62 Q28 59 30 55 Z" fill="#aab0b7" opacity="0.7"/></svg>`,
];

export function variantIndex(x: number, y: number, n: number): number {
  const h = ((x * 73856093) ^ (y * 19349663)) >>> 0;
  return h % n;
}

const PRESENCE_PALETTE = [
  '#e0714a',
  '#59c6d6',
  '#74c56a',
  '#f0c05a',
  '#c58fe0',
  '#f07676',
  '#8fb7d6',
  '#e08fc0',
  '#7ad6b0',
  '#d6b84a',
  '#9b8cff',
  '#5ad1a0',
];

export function colorFor(subject: string): string {
  let hash = 0;
  for (let index = 0; index < subject.length; index++) {
    hash = (hash * 31 + subject.charCodeAt(index)) >>> 0;
  }
  return PRESENCE_PALETTE[hash % PRESENCE_PALETTE.length];
}

export type PresencePayload = {
  name: string;
  role: string;
  color: string;
  cx: number;
  cy: number;
  onGrid: boolean;
};

export function parsePresencePayload(json?: string): PresencePayload {
  const fallback: PresencePayload = {
    name: 'Someone',
    role: '',
    color: '',
    cx: 0,
    cy: 0,
    onGrid: false,
  };
  if (!json) return fallback;
  try {
    const parsed: unknown = JSON.parse(json);
    if (typeof parsed !== 'object' || parsed === null) return fallback;
    const payload = parsed as Record<string, unknown>;
    return {
      name: safePresenceText(payload.name, 'Someone', PRESENCE_NAME_MAX),
      role: safePresenceText(payload.role, '', PRESENCE_ROLE_MAX),
      color: safePresenceColor(payload.color),
      cx: safePresenceCoordinate(payload.cx, -1, WIDTH + 1),
      cy: safePresenceCoordinate(payload.cy, -1, HEIGHT + 1),
      onGrid: payload.onGrid === true,
    };
  } catch {
    return fallback;
  }
}

export function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}

export function worldPixelSize(width: number, height: number) {
  return {
    width: width * TILE_SIZE + 2 * PAD,
    height: height * TILE_SIZE + 2 * PAD,
  };
}

export function stepDirection(
  fromX: number,
  fromY: number,
  toX: number,
  toY: number
): string {
  if (toX === fromX && toY === fromY - 1) return 'n';
  if (toX === fromX + 1 && toY === fromY) return 'e';
  if (toX === fromX && toY === fromY + 1) return 's';
  if (toX === fromX - 1 && toY === fromY) return 'w';
  return '';
}

export function paintLinePoints(
  from: { x: number; y: number },
  to: { x: number; y: number }
): Array<{ x: number; y: number }> {
  const points: Array<{ x: number; y: number }> = [];
  let { x, y } = from;
  let guard = 0;
  while ((x !== to.x || y !== to.y) && guard++ < 256) {
    if (x !== to.x) x += Math.sign(to.x - x);
    else y += Math.sign(to.y - y);
    points.push({ x, y });
  }
  return points;
}

export const ENTITY_SVG: Record<string, string> = {
  dome: DOME_SVG,
  pod: POD_SVG,
  solar: SOLAR_SVG,
};
