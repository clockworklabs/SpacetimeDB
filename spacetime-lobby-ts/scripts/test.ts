import * as assert from 'node:assert/strict';
import {
  expectedScore,
  rankedBand,
  rankedSelection,
  updatedRating,
} from '../src/matchmaking.ts';
import { lobbyCompositeKey } from '../src/keys.ts';

assert.equal(expectedScore(1000, 1000), 0.5);
assert.ok(expectedScore(1200, 1000) > 0.75);
assert.equal(updatedRating(1000, 0.5, 1), 1016);
assert.equal(updatedRating(1000, 0.5, 0), 984);
assert.equal(updatedRating(100, 1, 0), 100);
assert.equal(updatedRating(5000, 0, 1), 5000);

const at = (microsSinceUnixEpoch: bigint) => ({ microsSinceUnixEpoch });
assert.equal(rankedBand({ createdAt: at(100_000_000n) }, 100_000_000n), 100);
assert.equal(rankedBand({ createdAt: at(0n) }, 20_000_000n), 200);
assert.equal(rankedBand({ createdAt: at(0n) }, 1_000_000_000n), 800);

const queue = [
  { ticketId: 'a', rating: 1000, ratingPool: 'ranked', createdAt: at(0n) },
  { ticketId: 'b', rating: 1080, ratingPool: 'ranked', createdAt: at(1n) },
  { ticketId: 'c', rating: 1400, ratingPool: 'ranked', createdAt: at(2n) },
];
assert.deepEqual(
  rankedSelection(queue, 2, 5_000_000n)?.map(ticket => ticket.ticketId),
  ['a', 'b']
);
assert.equal(rankedSelection(queue, 3, 5_000_000n), undefined);

// Closest ratings win; other rating pools are never mixed in.
const mixed = [
  { ticketId: 'anchor', rating: 1000, ratingPool: 'ranked', createdAt: at(0n) },
  { ticketId: 'far', rating: 1090, ratingPool: 'ranked', createdAt: at(1n) },
  { ticketId: 'other', rating: 1000, ratingPool: 'casual', createdAt: at(2n) },
  { ticketId: 'near', rating: 960, ratingPool: 'ranked', createdAt: at(3n) },
];
assert.deepEqual(
  rankedSelection(mixed, 2, 0n)?.map(ticket => ticket.ticketId),
  ['anchor', 'near']
);
assert.deepEqual(
  rankedSelection(mixed, 3, 0n)?.map(ticket => ticket.ticketId),
  ['anchor', 'near', 'far']
);

// Far-apart ratings stay unmatched; selection stays fast at the scan cap.
const spread = Array.from({ length: 5000 }, (_, index) => ({
  rating: 100 + index * 1000,
  ratingPool: 'ranked',
  createdAt: at(BigInt(index)),
}));
const started = Date.now();
assert.equal(rankedSelection(spread, 2, 0n), undefined);
assert.ok(Date.now() - started < 1000);

assert.notEqual(lobbyCompositeKey('a:b', 'c'), lobbyCompositeKey('a', 'b:c'));
assert.equal(lobbyCompositeKey('ranked', 'player-1'), '6:ranked8:player-1');

console.log('lobby tests passed');
