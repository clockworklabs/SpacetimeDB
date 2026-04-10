// Connection soak scenario.
//
// Ramps connections at +rampRate per second. Each connection registers,
// joins the bench room, and idles. Counts how many concurrent connections
// the app holds before the first error or until the cap is reached.
//
// Reports the high-water mark of stable concurrent connections.

import type { ScenarioResult } from '../metrics.ts';
import { LatencyHistogram } from '../metrics.ts';
import {
  type PgConfig,
  createPgRoom,
  createPgUser,
  joinPgRoom,
  connectPgClient,
} from '../clients/postgres-client.ts';
import {
  type StdbConfig,
  connectStdb,
  stdbCreateRoom,
  stdbFindRoomIdByName,
  stdbJoinRoom,
  stdbSetName,
} from '../clients/spacetime-client.ts';

export interface SoakOpts {
  cap: number;
  rampPerSec: number;
}

export async function runSoakPostgres(cfg: PgConfig, opts: SoakOpts): Promise<ScenarioResult> {
  const tag = `pk${Date.now().toString(36)}`;
  const seedUser = await createPgUser(cfg, `${tag}_seed`);
  const room = await createPgRoom(cfg, tag, seedUser.id);

  const startedAt = new Date().toISOString();
  const startTime = Date.now();
  const opened: Array<{ close(): void }> = [];
  let errors = 0;

  while (opened.length < opts.cap) {
    const targetByNow = Math.floor(((Date.now() - startTime) / 1000) * opts.rampPerSec);
    while (opened.length < targetByNow && opened.length < opts.cap) {
      try {
        const u = await createPgUser(cfg, `${tag}_c${opened.length}`);
        await joinPgRoom(cfg, room.id, u.id);
        const c = await connectPgClient(cfg, u, room.id, () => { /* idle */ });
        opened.push(c);
      } catch {
        errors += 1;
        if (errors > 5) break;
      }
    }
    if (errors > 5) break;
    await new Promise((r) => setTimeout(r, 100));
  }

  const durationSec = (Date.now() - startTime) / 1000;
  const held = opened.length;

  for (const c of opened) c.close();

  return {
    scenario: 'connection-soak',
    backend: 'postgres',
    startedAt,
    durationSec,
    writers: held,
    sent: 0,
    received: 0,
    errors,
    msgsPerSec: 0,
    ackLatencyMs: new LatencyHistogram().summary(),
    fanoutLatencyMs: new LatencyHistogram().summary(),
    notes: `Held ${held} concurrent connections (cap=${opts.cap}, ramp=${opts.rampPerSec}/s)`,
  };
}

export async function runSoakSpacetime(cfg: StdbConfig, opts: SoakOpts): Promise<ScenarioResult> {
  const tag = `sk${Date.now().toString(36)}`;
  // Seed only needs the room table for room id lookup; skip message etc to avoid OOM
  const seed = await connectStdb(cfg, { subscriptions: ['SELECT * FROM room'] });
  await stdbSetName(seed, `${tag}_seed`);
  await stdbCreateRoom(seed, tag);
  let roomId: bigint | null = null;
  for (let i = 0; i < 20 && roomId === null; i++) {
    roomId = stdbFindRoomIdByName(seed, tag);
    if (roomId === null) await new Promise((r) => setTimeout(r, 100));
  }
  if (roomId === null) throw new Error('failed to locate created room id');

  const startedAt = new Date().toISOString();
  const startTime = Date.now();
  const opened: Array<{ close(): void }> = [];
  let errors = 0;

  // Async ws errors on already-opened connections surface as unhandledRejection
  // / uncaughtException in the node ws layer. Count them instead of crashing so
  // we can report the high water mark we actually reached.
  const onAsyncErr = (): void => { errors += 1; };
  process.on('unhandledRejection', onAsyncErr);
  process.on('uncaughtException', onAsyncErr);

  try {
    while (opened.length < opts.cap) {
      const targetByNow = Math.floor(((Date.now() - startTime) / 1000) * opts.rampPerSec);
      while (opened.length < targetByNow && opened.length < opts.cap) {
        try {
          // Soak workers don't need any subscriptions — they just hold the
          // connection open. Skipping table syncs avoids OOM at high cap.
          const c = await connectStdb(cfg, { subscriptions: [] });
          await stdbSetName(c, `${tag}_c${opened.length}`);
          opened.push(c);
        } catch {
          errors += 1;
          if (errors > 50) break;
        }
      }
      if (errors > 50) break;
      await new Promise((r) => setTimeout(r, 100));
    }
  } finally {
    process.off('unhandledRejection', onAsyncErr);
    process.off('uncaughtException', onAsyncErr);
  }

  const durationSec = (Date.now() - startTime) / 1000;
  const held = opened.length;

  for (const c of opened) c.close();
  seed.close();

  return {
    scenario: 'connection-soak',
    backend: 'spacetime',
    startedAt,
    durationSec,
    writers: held,
    sent: 0,
    received: 0,
    errors,
    msgsPerSec: 0,
    ackLatencyMs: new LatencyHistogram().summary(),
    fanoutLatencyMs: new LatencyHistogram().summary(),
    notes: `Held ${held} concurrent connections (cap=${opts.cap}, ramp=${opts.rampPerSec}/s)`,
  };
}
