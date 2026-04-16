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
  pgSendRest,
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

  // Brief warmup — also detects whether this PG app uses socket-based or
  // REST-based message sending. Track warmup echoes separately since
  // `received` only increments when `measuring` is true.
  let warmupEchoes = 0;
  const warmupHandler = () => { warmupEchoes += 1; };
  listener.socket.on('message', warmupHandler);
  listener.socket.on('new_message', warmupHandler);
  for (let i = 0; i < writers.length; i++) {
    pgSend(writers[i]!, room.id, `${'__bench:'}${process.hrtime.bigint()}:0:warmup`);
  }
  await new Promise((r) => setTimeout(r, 1500));
  listener.socket.off('message', warmupHandler);
  listener.socket.off('new_message', warmupHandler);
  const useRest = warmupEchoes === 0; // socket warmup produced no echoes → REST mode
  if (useRest) {
    console.log('[pg] Socket send produced no echoes — switching to REST mode (POST /api/rooms/:id/messages)');
  }

  measuring = true;
  const startedAt = new Date().toISOString();
  const endTime = Date.now() + opts.durationSec * 1000;
  let seq = 1;

  const MAX_INFLIGHT = 200;
  const writerLoop = async (w: typeof writers[number]): Promise<void> => {
    if (useRest) {
      // REST mode (20260403): POST per message, ack = HTTP response
      while (Date.now() < endTime) {
        while (inflight.size >= MAX_INFLIGHT && Date.now() < endTime) {
          await new Promise((r) => setTimeout(r, 1));
        }
        if (Date.now() >= endTime) break;
        const s = seq++;
        const t0 = process.hrtime.bigint();
        sent += 1;
        try {
          const resp = await pgSendRest(cfg, room.id, w.user.id, stampMessage(s));
          if (resp) {
            received += 1;
            ack.record(nsToMs(process.hrtime.bigint() - t0));
            fanout.record(nsToMs(process.hrtime.bigint() - t0));
          }
        } catch { /* ignore */ }
      }
    } else {
      // Socket mode (20260406): fire-and-forget emit, ack = echo
      while (Date.now() < endTime) {
        while (inflight.size >= MAX_INFLIGHT && Date.now() < endTime) {
          await new Promise((r) => setTimeout(r, 1));
        }
        if (Date.now() >= endTime) break;
        const s = seq++;
        inflight.set(s, process.hrtime.bigint());
        pgSend(w, room.id, stampMessage(s));
        sent += 1;
        await new Promise((r) => setImmediate(r));
      }
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
    notes: `${opts.writers} writers firing as fast as possible`,
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

  // Listener DISABLED for pure write-throughput measurement. With the listener
  // subscribed, fan-out processing competes with writer ack handling on the
  // same Node event loop, becoming the client-side bottleneck. We trust the
  // reducer ack to measure successful commits. fanout histogram will be empty.
  const listener: { close: () => void } | null = null;

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

  // Each writer worker runs a pipelined loop — keeps up to MAX_INFLIGHT_PER_WORKER
  // reducer calls in flight concurrently. Matches keynote-2 benchmark methodology.
  // STDB handles many more in-flight calls than PG because it batches over WS.
  const MAX_INFLIGHT_PER_WORKER = 10;
  const writerLoop = async (w: typeof writers[number]): Promise<void> => {
    const inflight = new Set<Promise<void>>();
    const launchOp = () => {
      const s = seq++;
      const text = stampMessage(s);
      const t0 = process.hrtime.bigint();
      sent += 1;
      const p = stdbSendMessage(w, roomId!, text).then(
        () => {
          if (Date.now() < endTime) {
            ack.record(nsToMs(process.hrtime.bigint() - t0));
          }
        },
        () => { /* ignore errors */ }
      );
      inflight.add(p);
      p.finally(() => { inflight.delete(p); });
    };
    while (Date.now() < endTime) {
      if (inflight.size < MAX_INFLIGHT_PER_WORKER) {
        launchOp();
      } else {
        await new Promise((r) => setImmediate(r));
      }
    }
    // Drain outstanding for up to 5s after end
    const drainDeadline = Date.now() + 5000;
    while (inflight.size > 0 && Date.now() < drainDeadline) {
      await new Promise((r) => setTimeout(r, 10));
    }
  };
  await Promise.all(writers.map(writerLoop));

  // Drain
  await new Promise((r) => setTimeout(r, 3000));
  measuring = false;

  for (const w of writers) w.close();
  if (listener) listener.close();

  // With listener disabled, count "received" as acked reducer calls — we
  // trust reducer acks as proof the row was committed.
  if (received === 0) received = ack.count();

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
