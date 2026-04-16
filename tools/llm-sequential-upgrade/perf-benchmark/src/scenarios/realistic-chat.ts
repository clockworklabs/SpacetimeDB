// Realistic chat scenario.
//
// Spawns M concurrent users, each sending 1 message every 5-15 seconds (jittered)
// for `durationSec` seconds. Measures the same metrics as stress-throughput,
// but under load that resembles real usage rather than worst-case flooding.
//
// This is the headroom test: can the app sustain a comfortable chat load
// without latency tail blowing up?

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

export interface RealisticOpts {
  users: number;
  durationSec: number;
  minIntervalMs: number; // default 5000
  maxIntervalMs: number; // default 15000
}

function jitter(min: number, max: number): number {
  return min + Math.random() * (max - min);
}

export async function runRealisticPostgres(cfg: PgConfig, opts: RealisticOpts): Promise<ScenarioResult> {
  const tag = `pr${Date.now().toString(36)}`;
  const users = await Promise.all(
    Array.from({ length: opts.users }, (_, i) => createPgUser(cfg, `${tag}_u${i}`)),
  );
  const listenerUser = await createPgUser(cfg, `${tag}_listener`);
  const room = await createPgRoom(cfg, tag, listenerUser.id);
  await Promise.all(users.map((u) => joinPgRoom(cfg, room.id, u.id)));

  const fanout = new LatencyHistogram();
  let received = 0;
  let measuring = false;

  const listener = await connectPgClient(cfg, listenerUser, room.id, (msg) => {
    if (!measuring) return;
    const stamp = parseStamp(msg.content);
    if (!stamp) return;
    received += 1;
    fanout.record(nsToMs(process.hrtime.bigint() - stamp.sentNs));
  });

  const clients = await Promise.all(
    users.map((u) => connectPgClient(cfg, u, room.id, () => { /* discard own echoes */ })),
  );

  measuring = true;
  const startedAt = new Date().toISOString();
  const endTime = Date.now() + opts.durationSec * 1000;
  let seq = 1;
  let sent = 0;

  const userLoop = async (c: typeof clients[number]): Promise<void> => {
    while (Date.now() < endTime) {
      pgSend(c, room.id, stampMessage(seq++));
      sent += 1;
      await new Promise((r) => setTimeout(r, jitter(opts.minIntervalMs, opts.maxIntervalMs)));
    }
  };
  await Promise.all(clients.map(userLoop));

  await new Promise((r) => setTimeout(r, 2000));
  measuring = false;

  for (const c of clients) c.close();
  listener.close();

  return {
    scenario: 'realistic-chat',
    backend: 'postgres',
    startedAt,
    durationSec: opts.durationSec,
    writers: opts.users,
    sent,
    received,
    errors: 0,
    msgsPerSec: received / opts.durationSec,
    ackLatencyMs: new LatencyHistogram().summary(),
    fanoutLatencyMs: fanout.summary(),
    notes: `${opts.users} users, jitter ${opts.minIntervalMs}-${opts.maxIntervalMs}ms`,
  };
}

export async function runRealisticSpacetime(cfg: StdbConfig, opts: RealisticOpts): Promise<ScenarioResult> {
  const tag = `sr${Date.now().toString(36)}`;

  const fanout = new LatencyHistogram();
  let received = 0;
  let measuring = false;

  const listener = await connectStdb(cfg, {
    onMessage: (row) => {
      if (!measuring) return;
      const stamp = parseStamp(row.text);
      if (!stamp) return;
      received += 1;
      fanout.record(nsToMs(process.hrtime.bigint() - stamp.sentNs));
    },
  });
  await stdbSetName(listener, `${tag}_l`);
  await stdbCreateRoom(listener, tag);
  let roomId: bigint | null = null;
  for (let i = 0; i < 20 && roomId === null; i++) {
    roomId = stdbFindRoomIdByName(listener, tag);
    if (roomId === null) await new Promise((r) => setTimeout(r, 100));
  }
  if (roomId === null) throw new Error('failed to locate created room id');

  const clients: Awaited<ReturnType<typeof connectStdb>>[] = [];
  for (let i = 0; i < opts.users; i++) {
    const w = await connectStdb(cfg);
    await stdbSetName(w, `${tag}_u${i}`);
    await stdbJoinRoom(w, roomId);
    clients.push(w);
  }

  measuring = true;
  const startedAt = new Date().toISOString();
  const endTime = Date.now() + opts.durationSec * 1000;
  let seq = 1;
  let sent = 0;

  const userLoop = async (c: typeof clients[number]): Promise<void> => {
    while (Date.now() < endTime) {
      try {
        await stdbSendMessage(c, roomId!, stampMessage(seq++));
        sent += 1;
      } catch { /* ignore */ }
      await new Promise((r) => setTimeout(r, jitter(opts.minIntervalMs, opts.maxIntervalMs)));
    }
  };
  await Promise.all(clients.map(userLoop));

  await new Promise((r) => setTimeout(r, 2000));
  measuring = false;

  for (const c of clients) c.close();
  listener.close();

  return {
    scenario: 'realistic-chat',
    backend: 'spacetime',
    startedAt,
    durationSec: opts.durationSec,
    writers: opts.users,
    sent,
    received,
    errors: 0,
    msgsPerSec: received / opts.durationSec,
    ackLatencyMs: new LatencyHistogram().summary(),
    fanoutLatencyMs: fanout.summary(),
    notes: `${opts.users} users, jitter ${opts.minIntervalMs}-${opts.maxIntervalMs}ms`,
  };
}
