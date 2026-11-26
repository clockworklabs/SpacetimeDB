import { schema, table, t, SenderError } from 'spacetimedb/server';
const seatEvent = table(
  { name: 'seat_event', public: true },
  { id: t.u64().primaryKey(), capacity: t.u32() }
);
const booking = table(
  {
    name: 'booking',
    public: true,
    indexes: [
      {
        accessor: 'byOwnerEvent',
        algorithm: 'btree',
        columns: ['owner', 'eventId'],
      },
    ],
  },
  {
    id: t.u64().primaryKey().autoInc(),
    owner: t.identity(),
    eventId: t.u64().index('btree'),
    status: t.string(),
  }
);
const spacetimedb = schema({ seatEvent, booking });
export default spacetimedb;
export const create_event = spacetimedb.reducer(
  { id: t.u64(), capacity: t.u32() },
  (ctx, { id, capacity }) => {
    if (!capacity) throw new SenderError('invalid capacity');
    if (ctx.db.seatEvent.id.find(id)) throw new SenderError('event exists');
    ctx.db.seatEvent.insert({ id, capacity });
  }
);
export const join_event = spacetimedb.reducer(
  { eventId: t.u64() },
  (ctx, { eventId }) => {
    const event = ctx.db.seatEvent.id.find(eventId);
    if (!event) throw new SenderError('missing event');
    if (
      [...ctx.db.booking.byOwnerEvent.filter([ctx.sender, eventId])].some(
        b => b.status !== 'cancelled'
      )
    )
      return;
    const seated = [...ctx.db.booking.eventId.filter(eventId)].filter(
      b => b.status === 'seated'
    ).length;
    ctx.db.booking.insert({
      id: 0n,
      owner: ctx.sender,
      eventId,
      status: seated < event.capacity ? 'seated' : 'waiting',
    });
  }
);
export const cancel_booking = spacetimedb.reducer(
  { id: t.u64() },
  (ctx, { id }) => {
    const row = ctx.db.booking.id.find(id);
    if (!row) return;
    if (!row.owner.equals(ctx.sender)) throw new SenderError('owner only');
    if (row.status === 'cancelled') return;
    ctx.db.booking.id.update({ ...row, status: 'cancelled' });
    if (row.status === 'seated') {
      const next = [...ctx.db.booking.eventId.filter(row.eventId)]
        .filter(b => b.status === 'waiting')
        .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))[0];
      if (next) ctx.db.booking.id.update({ ...next, status: 'seated' });
    }
  }
);
