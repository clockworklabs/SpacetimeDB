import assert from 'node:assert/strict';
import {
  selectActiveRoom,
  selectHighlightedManeuver,
  selectLatestDuel,
  selectLatestTicket,
  selectLobbyScreen,
  type Combatant,
  type Duel,
  type DuelManeuver,
  type LobbyRoom,
  type LobbyTicket,
} from '../src/model';

const timestamp = (value: bigint) => ({ microsSinceUnixEpoch: value });
const ticket = (
  ticketId: string,
  createdAt: bigint,
  status: string,
  roomId?: bigint
): LobbyTicket => ({
  ticketId,
  pool: 'spaceship_duel',
  status: { tag: status },
  roomId,
  createdAt: timestamp(createdAt),
});
const duel = (
  roomId: bigint,
  updatedAt: bigint,
  status: string,
  round = 0
): Duel => ({
  roomId,
  status: { tag: status },
  round,
  updatedAt: timestamp(updatedAt),
});
const room = (roomId: bigint): LobbyRoom => ({
  roomId,
  pool: 'spaceship_duel',
  status: { tag: 'Ready' },
  capacity: 2,
  createdAt: timestamp(roomId),
});

const olderTicket = ticket('older', 1n, 'Cancelled');
const currentTicket = ticket('current', 2n, 'Matched', 20n);
assert.equal(selectLatestTicket([currentTicket, olderTicket]), currentTicket);
assert.equal(selectLatestTicket([]), undefined);

const unrelatedActiveDuel = duel(10n, 3n, 'Active');
const ticketDuel = duel(20n, 1n, 'Complete');
assert.equal(
  selectLatestDuel([unrelatedActiveDuel, ticketDuel], currentTicket),
  ticketDuel
);
assert.equal(
  selectLatestDuel([ticketDuel, unrelatedActiveDuel], undefined),
  unrelatedActiveDuel
);
assert.equal(selectLatestDuel([ticketDuel], undefined), undefined);

const rooms = [room(10n), room(20n)];
assert.equal(
  selectActiveRoom(rooms, unrelatedActiveDuel, currentTicket),
  rooms[0]
);
assert.equal(selectActiveRoom(rooms, ticketDuel, currentTicket), rooms[1]);
assert.equal(selectActiveRoom(rooms, undefined, undefined), undefined);

assert.equal(
  selectLobbyScreen({
    screenOverride: 'setup',
    ticket: ticket('queued', 3n, 'Queued'),
    duel: unrelatedActiveDuel,
    room: rooms[0],
    playedRoomId: '10',
  }),
  'setupScreen'
);
assert.equal(
  selectLobbyScreen({
    screenOverride: null,
    ticket: ticket('queued', 3n, 'Queued'),
    duel: undefined,
    room: undefined,
    playedRoomId: null,
  }),
  'waitingScreen'
);
assert.equal(
  selectLobbyScreen({
    screenOverride: null,
    ticket: currentTicket,
    duel: ticketDuel,
    room: rooms[1],
    playedRoomId: '20',
  }),
  'duelScreen'
);
assert.equal(
  selectLobbyScreen({
    screenOverride: null,
    ticket: currentTicket,
    duel: ticketDuel,
    room: rooms[1],
    playedRoomId: null,
  }),
  'setupScreen'
);
assert.equal(
  selectLobbyScreen({
    screenOverride: null,
    ticket: undefined,
    duel: undefined,
    room: rooms[0],
    playedRoomId: null,
  }),
  'duelScreen'
);
assert.equal(
  selectLobbyScreen({
    screenOverride: null,
    ticket: undefined,
    duel: undefined,
    room: undefined,
    playedRoomId: null,
  }),
  'setupScreen'
);

const combatant: Combatant = {
  roomId: 10n,
  subject: 'pilot',
  displayName: 'Pilot',
  shipClass: { tag: 'Interceptor' },
  hull: 100,
  maxHull: 100,
  shields: 50,
  maxShields: 50,
  attack: 20,
  defense: 10,
  speed: 8,
  critBps: 500,
  dodgeBps: 1000,
};
const choice = (
  round: number,
  slot: 'Primary' | 'Defensive'
): DuelManeuver => ({
  choiceId: `${round}:${slot}`,
  roomId: 10n,
  round,
  subject: 'pilot',
  slot: { tag: slot },
  maneuverId: slot.toLowerCase(),
});
const choices = [choice(2, 'Defensive'), choice(1, 'Primary')];
const unrelatedChoice = {
  ...choice(99, 'Primary'),
  choiceId: 'unrelated',
  roomId: 99n,
};
assert.equal(
  selectHighlightedManeuver(duel(10n, 5n, 'Active', 1), combatant, [
    unrelatedChoice,
    ...choices,
  ]),
  choices[0]
);
assert.equal(
  selectHighlightedManeuver(duel(10n, 5n, 'Complete', 2), combatant, choices),
  choices[0]
);
assert.equal(
  selectHighlightedManeuver(duel(10n, 5n, 'Configuring'), combatant, choices),
  undefined
);

console.log('lobby model tests passed');
