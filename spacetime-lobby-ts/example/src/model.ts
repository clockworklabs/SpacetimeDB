export interface ServerConfig {
  spacetimeUri: string;
  databaseName: string;
}

export type TableEvents<T> = {
  iter(): Iterable<T>;
  onInsert(cb: (ctx: EventContext, row: T) => void): void;
  onUpdate(cb: (ctx: EventContext, old: T, row: T) => void): void;
  onDelete(cb: (ctx: EventContext, row: T) => void): void;
};

export type EnumTag<T extends string = string> = { tag: T };
export type ShipClass = 'Bulwark' | 'Interceptor' | 'Phantom' | 'Artillery';
export type ManeuverSlot = 'Primary' | 'Defensive' | 'Risky';

export type Pilot = {
  subject: string;
  displayName: string;
  shipClass: EnumTag<ShipClass>;
};

export type LobbyTicket = {
  ticketId: string;
  pool: string;
  status: EnumTag;
  roomId?: bigint;
  createdAt: { microsSinceUnixEpoch: bigint };
};

export type LobbyRoom = {
  roomId: bigint;
  pool: string;
  status: EnumTag;
  capacity: number;
  createdAt: { microsSinceUnixEpoch: bigint };
};

export type LobbySeat = {
  seatId: bigint;
  roomId: bigint;
  subject: string;
  seatIndex: number;
  status: EnumTag;
};

export type Duel = {
  roomId: bigint;
  status: EnumTag;
  round: number;
  winnerSubject?: string;
  updatedAt: { microsSinceUnixEpoch: bigint };
};

export type Combatant = {
  roomId: bigint;
  subject: string;
  displayName: string;
  shipClass: EnumTag<ShipClass>;
  hull: number;
  maxHull: number;
  shields: number;
  maxShields: number;
  attack: number;
  defense: number;
  speed: number;
  critBps: number;
  dodgeBps: number;
};

export type ShipCatalogRow = {
  shipId: string;
  shipClass: EnumTag<ShipClass>;
  role: string;
  description: string;
  hull: number;
  shields: number;
  attack: number;
  defense: number;
  speed: number;
  critBps: number;
  dodgeBps: number;
};

export type ManeuverCatalogRow = {
  maneuverId: string;
  shipClass: EnumTag<ShipClass>;
  slot: EnumTag<ManeuverSlot>;
  name: string;
  description: string;
  damageBps: number;
  defenseBps: number;
  shieldRestore: number;
  selfShieldCost: number;
  critBonusBps: number;
  dodgeBonusBps: number;
};

export type DuelManeuver = {
  choiceId: string;
  roomId: bigint;
  round: number;
  subject: string;
  slot: EnumTag<ManeuverSlot>;
  maneuverId: string;
};

export type RoundLog = {
  logId: bigint;
  roomId: bigint;
  round: number;
  message: string;
  createdAt: { microsSinceUnixEpoch: bigint };
};

export type QueueSummary = {
  pool: string;
  queuedTickets: number;
  readyRooms: number;
  activeRooms: number;
};

export type RatingRow = {
  pool: string;
  subject: string;
  rating: number;
  wins: number;
  losses: number;
  draws: number;
  matches: number;
};

export type LobbyScreen = 'setupScreen' | 'waitingScreen' | 'duelScreen';

export function selectLatestTicket(
  tickets: readonly LobbyTicket[]
): LobbyTicket | undefined {
  return [...tickets].sort((a, b) => {
    const aCreated = a.createdAt.microsSinceUnixEpoch;
    const bCreated = b.createdAt.microsSinceUnixEpoch;
    return aCreated < bCreated ? 1 : aCreated > bCreated ? -1 : 0;
  })[0];
}

export function selectLatestDuel(
  duels: readonly Duel[],
  ticket: LobbyTicket | undefined
): Duel | undefined {
  const newestFirst = [...duels].sort((a, b) => {
    const aUpdated = a.updatedAt.microsSinceUnixEpoch;
    const bUpdated = b.updatedAt.microsSinceUnixEpoch;
    return aUpdated < bUpdated ? 1 : aUpdated > bUpdated ? -1 : 0;
  });
  if (ticket?.roomId !== undefined) {
    const ticketDuel = newestFirst.find(duel => duel.roomId === ticket.roomId);
    if (ticketDuel) return ticketDuel;
  }
  return newestFirst.find(
    duel => duel.status.tag !== 'Complete' && duel.status.tag !== 'Abandoned'
  );
}

export function selectActiveRoom(
  rooms: readonly LobbyRoom[],
  duel: Duel | undefined,
  ticket: LobbyTicket | undefined
): LobbyRoom | undefined {
  if (
    duel &&
    duel.status.tag !== 'Complete' &&
    duel.status.tag !== 'Abandoned'
  ) {
    return rooms.find(room => room.roomId === duel.roomId);
  }
  if (ticket?.roomId === undefined) return undefined;
  return rooms.find(room => room.roomId === ticket.roomId);
}

export function selectLobbyScreen(input: {
  screenOverride: 'setup' | null;
  ticket: LobbyTicket | undefined;
  duel: Duel | undefined;
  room: LobbyRoom | undefined;
  playedRoomId: string | null;
}): LobbyScreen {
  const { screenOverride, ticket, duel, room, playedRoomId } = input;
  if (screenOverride === 'setup') return 'setupScreen';
  if (ticket?.status.tag === 'Queued') return 'waitingScreen';
  if (
    duel &&
    (duel.status.tag === 'Complete' || duel.status.tag === 'Abandoned')
  ) {
    return playedRoomId === duel.roomId.toString()
      ? 'duelScreen'
      : 'setupScreen';
  }
  if (duel || room) return 'duelScreen';
  return 'setupScreen';
}

export function selectHighlightedManeuver(
  duel: Duel | undefined,
  combatant: Combatant,
  choices: readonly DuelManeuver[]
): DuelManeuver | undefined {
  if (!duel || duel.status.tag === 'Configuring') return undefined;
  const newestFirst = choices
    .filter(
      choice =>
        choice.roomId === combatant.roomId &&
        choice.subject === combatant.subject
    )
    .sort((a, b) => b.round - a.round);
  if (duel.status.tag === 'Active') {
    return (
      newestFirst.find(choice => choice.round === duel.round + 1) ??
      newestFirst.find(choice => choice.round === duel.round)
    );
  }
  return newestFirst[0];
}

export const TOKEN_KEY_PREFIX = 'lobby-duel:stdb-token';
export const MATCH_FALLBACK_MS = 4500;
export const shipClasses: ShipClass[] = [
  'Bulwark',
  'Interceptor',
  'Phantom',
  'Artillery',
];
export const maneuverSlots: ManeuverSlot[] = ['Primary', 'Defensive', 'Risky'];

export const shipColors: Record<ShipClass, string> = {
  Bulwark: 'var(--green)',
  Interceptor: 'var(--yellow)',
  Phantom: 'var(--violet)',
  Artillery: 'var(--red)',
};

// Each maneuver slot reads as a distinct tactical stance: strike / guard / gamble.
export const maneuverSlotMeta: Record<
  ManeuverSlot,
  { color: string; label: string; icon: string }
> = {
  Primary: {
    color: 'var(--red)',
    label: 'Primary',
    icon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linecap="round" aria-hidden="true"><circle cx="12" cy="12" r="7"/><path d="M12 2v3M12 19v3M2 12h3M19 12h3"/></svg>',
  },
  Defensive: {
    color: 'var(--cyan)',
    label: 'Defensive',
    icon: '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.9" stroke-linejoin="round" aria-hidden="true"><path d="M12 3l7 3v5.5c0 4-3 6.8-7 8.5-4-1.7-7-4.5-7-8.5V6z"/></svg>',
  },
  Risky: {
    color: 'var(--violet)',
    label: 'Special',
    icon: '<svg viewBox="0 0 24 24" fill="currentColor" aria-hidden="true"><path d="M12 2l1.8 6.4L20 10l-6.2 1.6L12 18l-1.8-6.4L4 10l6.2-1.6z"/></svg>',
  },
};

export function maneuverEffects(
  move: ManeuverCatalogRow
): Array<{ text: string; cost: boolean }> {
  const signPct = (bps: number) =>
    `${bps > 0 ? '+' : ''}${Math.round(bps / 100)}%`;
  const fx: Array<{ text: string; cost: boolean }> = [];
  if (move.damageBps)
    fx.push({
      text: `${signPct(move.damageBps)} dmg`,
      cost: move.damageBps < 0,
    });
  if (move.defenseBps)
    fx.push({
      text: `${signPct(move.defenseBps)} def`,
      cost: move.defenseBps < 0,
    });
  if (move.critBonusBps)
    fx.push({
      text: `${signPct(move.critBonusBps)} crit`,
      cost: move.critBonusBps < 0,
    });
  if (move.dodgeBonusBps)
    fx.push({
      text: `${signPct(move.dodgeBonusBps)} dodge`,
      cost: move.dodgeBonusBps < 0,
    });
  if (move.shieldRestore)
    fx.push({ text: `+${move.shieldRestore} shield`, cost: false });
  if (move.selfShieldCost)
    fx.push({ text: `-${move.selfShieldCost} shield`, cost: true });
  return fx;
}

export function maneuverFx(move: ManeuverCatalogRow): string {
  return maneuverEffects(move)
    .map(c => `<i class="fx ${c.cost ? 'fx-cost' : 'fx-buff'}">${c.text}</i>`)
    .join('');
}
import type { EventContext } from './module_bindings';
