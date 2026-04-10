// Stress throughput scenario.
//
// Spawns N writer clients and has each fire send_message as fast as possible
// for `durationSec` seconds. A separate listener client (subscribed to the same
// room) measures fan-out latency by parsing a hrtime stamp embedded in the
// message text.
//
// Reports:
//   - sustained msgs/sec     (received-by-listener / duration)
//   - ack latency p50/p99    (PG: writer's own echo round-trip; STDB: reducer Promise resolve)
//   - fan-out latency p50/p99 (writer hrtime → listener observes row)

import { LatencyHistogram, parseStamp, stampMessage, nsToMs, type ScenarioResult } from '../metrics.ts';
import {
  type PgConfig,
  createPgRoom,
  createPgUser,
  joinPgRoom,
  connectPgClient,
  pgSend,
} from '../clients/postgres-client.ts';
import {
  type StdbConfig,
  connectStdb,
  stdbCreateRoom,
  stdbFindRoomIdByName,
  stdbJoinRoom,
  stdbSendMessage,
  stdbSetName,
} from '../clients/spacetime-client.ts';

export interface StressOpts {
  writers: number;
  durationSec: number;
}

export async function runStressPostgres(cfg: PgConfig, opts: StressOpts): Promise<ScenarioResult> {
  const tag = `ps${Date.now().toString(36)}`; // ~10 chars

  // Create N writers + 1 listener; one room they all join.
  const writerUsers = await Promise.all(
    Array.from({ length: opts.writers }, (_, i) => createPgUser(cfg, `${tag}_w${i}`)),
  );
  const listenerUser = await createPgUser(cfg, `${tag}_listener`);
  const room = await createPgRoom(cfg, tag, listenerUser.id);
  await Promise.all(writerUsers.map((u) => joinPgRoom(cfg, room.id, u.id)));

  const ack = new LatencyHistogram();
  const fanout = new LatencyHistogram();
  const inflight = new Map<number, bigint>();
  let received = 0;
  let sent = 0;
  let measuring = false;

  // Listener: counts received and computes fan-out latency
  const listener = await connectPgClient(cfg, listenerUser, room.id, (msg) => {
    if (!measuring) return;
    const stamp = parseStamp(msg.content);
    if (!stamp) return;
    received += 1;
    fanout.record(nsToMs(process.hrtime.bigint() - stamp.sentNs));
  });

  // Writers: each writer also subscribes (joined the room) and uses its own
  // echoes as the "ack" — the moment the server has inserted the message and
  // re-broadcast it back to me.
  const writers = await Promise.all(
    writerUsers.map((u) =>
      connectPgClient(cfg, u, room.id, (msg) => {
        if (!measuring) return;
        if (msg.userId !== u.id) return;
        const stamp = parseStamp(msg.content);
        if (!stamp) return;
        const start = inflight.get(stamp.seq);
        if (start !== undefined) {
          ack.record(nsToMs(process.hrtime.bigint() - start));
          inflight.delete(stamp.seq);
        }
      }),
    ),
  );

  // Brief warmup (not measured)
  for (let i = 0; i < writers.length; i++) {
    pgSend(writers[i]!, room.id, `${'__bench:'}${process.hrtime.bigint()}:0:warmup`);
  }
  await new Promise((r) => setTimeout(r, 1500));

  measuring = true;
  const startedAt = new Date().toISOString();
  const endTime = Date.now() + opts.durationSec * 1000;
  let seq = 1;

  // Each writer fires as fast as possible in its own async loop. The socket.io
  // client sends are fire-and-forget; the listener measures fan-out latency and
  // the writer's own echo gives ack latency.
  const writerLoop = async (w: typeof writers[number]): Promise<void> => {
    while (Date.now() < endTime) {
      const s = seq++;
      inflight.set(s, process.hrtime.bigint());
      pgSend(w, room.id, stampMessage(s));
      sent += 1;
      // Yield to the event loop so echoes can be processed and we don't
      // starve the socket.
      await new Promise((r) => setImmediate(r));
    }
  };
  await Promise.all(writers.map(writerLoop));

  // Drain in-flight echoes
  await new Promise((r) => setTimeout(r, 3000));
  measuring = false;

  for (const w of writers) w.close();
  listener.close();

  return {
    scenario: 'stress-throughput',
    backend: 'postgres',
    startedAt,
    durationSec: opts.durationSec,
    writers: opts.writers,
    sent,
    received,
    errors: 0,
    msgsPerSec: received / opts.durationSec,
    ackLatencyMs: ack.summary(),
    fanoutLatencyMs: fanout.summary(),
    notes: `${opts.writers} writers firing as fast as possible (rate limit disabled for benchmark)`,
  };
}

export async function runStressSpacetime(cfg: StdbConfig, opts: StressOpts): Promise<ScenarioResult> {
  const tag = `ss${Date.now().toString(36)}`;

  const ack = new LatencyHistogram();
  const fanout = new LatencyHistogram();
  let received = 0;
  let measuring = false;

  // Seed connection: only subscribes to the room table, enough to create the
  // bench room and look up its id. Avoids syncing the (potentially large)
  // message table on every new connection.
  const seed = await connectStdb(cfg, { subscriptions: ['SELECT * FROM room'] });
  await stdbSetName(seed, `${tag}_s`);
  await stdbCreateRoom(seed, tag);
  let roomId: bigint | null = null;
  for (let i = 0; i < 20 && roomId === null; i++) {
    roomId = stdbFindRoomIdByName(seed, tag);
    if (roomId === null) await new Promise((r) => setTimeout(r, 100));
  }
  if (roomId === null) throw new Error('failed to locate created room id');

  // Listener: filter subscription to ONLY messages in this room, so the
  // initial sync is empty and we only receive new messages fired during the
  // test.
  const listener = await connectStdb(cfg, {
    subscriptions: [`SELECT * FROM message WHERE room_id = ${roomId}`],
    onMessage: (row) => {
      if (!measuring) return;
      const stamp = parseStamp(row.text);
      if (!stamp) return;
      received += 1;
      fanout.record(nsToMs(process.hrtime.bigint() - stamp.sentNs));
    },
  });
  await stdbSetName(listener, `${tag}_l`);

  // Spawn writers. Writers don't need table subscriptions — ack latency comes
  // from the reducer promise, not from observing echoes. Skipping the default
  // subscription set avoids syncing ~70k historical message rows per writer.
  const writers: Awaited<ReturnType<typeof connectStdb>>[] = [];
  for (let i = 0; i < opts.writers; i++) {
    const w = await connectStdb(cfg, { subscriptions: [] });
    await stdbSetName(w, `${tag}_w${i}`);
    await stdbJoinRoom(w, roomId);
    writers.push(w);
  }

  // Warmup: each writer fires 5 messages
  for (let i = 0; i < 5; i++) {
    await Promise.all(writers.map((w) => stdbSendMessage(w, roomId!, `${'__bench:'}${process.hrtime.bigint()}:0:warmup`)));
  }
  // Tiny pause to let warmup drain
  await new Promise((r) => setTimeout(r, 500));

  measuring = true;
  const startedAt = new Date().toISOString();
  const endTime = Date.now() + opts.durationSec * 1000;
  let seq = 1;
  let sent = 0;

  // Each writer fires concurrently in its own async loop, awaiting reducer
  // promise resolution for ack latency. Each send is raced against the
  // remaining duration so a hung reducer call can't block us past endTime.
  const writerLoop = async (w: typeof writers[number]): Promise<void> => {
    while (Date.now() < endTime) {
      const s = seq++;
      const text = stampMessage(s);
      const t0 = process.hrtime.bigint();
      const remainingMs = endTime - Date.now();
      if (remainingMs <= 0) break;
      try {
        const sendP = stdbSendMessage(w, roomId!, text);
        const timedOut = Symbol('timeout');
        const result = await Promise.race([
          sendP.then(() => 'ok'),
          new Promise((r) => setTimeout(() => r(timedOut), remainingMs)),
        ]);
        if (result === timedOut) break;
        ack.record(nsToMs(process.hrtime.bigint() - t0));
        sent += 1;
      } catch {
        /* ignore */
      }
    }
  };
  await Promise.all(writers.map(writerLoop));

  // Drain
  await new Promise((r) => setTimeout(r, 3000));
  measuring = false;

  for (const w of writers) w.close();
  listener.close();

  return {
    scenario: 'stress-throughput',
    backend: 'spacetime',
    startedAt,
    durationSec: opts.durationSec,
    writers: opts.writers,
    sent,
    received,
    errors: 0,
    msgsPerSec: received / opts.durationSec,
    ackLatencyMs: ack.summary(),
    fanoutLatencyMs: fanout.summary(),
  };
}
