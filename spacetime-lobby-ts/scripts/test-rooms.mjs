// Room lifecycle checks against an in-memory table fake.
import assert from 'node:assert/strict';
import { Timestamp } from 'spacetimedb';
import { RoomStatus, SeatStatus } from '../src/submodule/schema.ts';
import {
  joinRoomForSubject,
  leaveRoomForSubject,
  reportMatchResult,
} from '../src/submodule/operations.ts';

const rooms = new Map();
const seats = new Map();
const ratings = new Map();
const results = [];
const tx = {
  timestamp: new Timestamp(1n),
  db: {
    lobbyRoom: {
      roomId: {
        find: id => rooms.get(id),
        update: row => rooms.set(row.roomId, row),
      },
    },
    lobbyRoomSeat: {
      byRoom: {
        filter: roomId => [...seats.values()].filter(s => s.roomId === roomId),
      },
      byRoomSubject: {
        filter: ([roomId, subject]) =>
          [...seats.values()].filter(
            s => s.roomId === roomId && s.subject === subject
          ),
      },
      seatId: { update: row => seats.set(row.seatId, row) },
    },
    lobbySubjectRating: {
      ratingId: {
        find: id => ratings.get(id),
        update: row => ratings.set(row.ratingId, row),
      },
      insert: row => (ratings.set(row.ratingId, row), row),
    },
    lobbyMatchResult: { insert: row => results.push(row) },
  },
};

rooms.set(1n, { roomId: 1n, pool: 'duel', status: RoomStatus.Ready });
for (const [seatId, subject] of [
  [1n, 'alice'],
  [2n, 'bob'],
]) {
  seats.set(seatId, {
    seatId,
    roomId: 1n,
    subject,
    seatIndex: Number(seatId) - 1,
    status: SeatStatus.Reserved,
  });
}

joinRoomForSubject(tx, { roomId: 1n, subject: 'alice' });
assert.equal(rooms.get(1n).status.tag, 'Ready');
joinRoomForSubject(tx, { roomId: 1n, subject: 'bob' });
assert.equal(rooms.get(1n).status.tag, 'Active');

// A losing player who leaves cannot stop the host from reporting the result.
leaveRoomForSubject(tx, { roomId: 1n, subject: 'bob' });
assert.equal(rooms.get(1n).status.tag, 'Active');
reportMatchResult(tx, { roomId: 1n, winnerSubject: 'alice' });
assert.equal(rooms.get(1n).status.tag, 'Closed');
assert.equal(results.length, 1);
assert.ok(ratings.get('4:duel3:bob').rating < 1000);
assert.throws(
  () => reportMatchResult(tx, { roomId: 1n, winnerSubject: 'bob' }),
  error => error.message === 'lobby.room_not_active'
);

process.stdout.write('lobby room tests passed\n');
