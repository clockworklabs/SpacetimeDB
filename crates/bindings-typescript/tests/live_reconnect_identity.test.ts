import { describe, expect, test } from 'vitest';
import { execFileSync, spawn } from 'node:child_process';
import { ConnectionManager } from '../src/sdk/connection_manager.ts';
import { DbConnection } from '../test-app/src/module_bindings/index.ts';

/**
 * End-to-end proof that an automatic reconnect keeps the identity it was issued.
 *
 * The unit test in connection_manager_token_reuse.test.ts drives mocks. This
 * one drives a real host: connect anonymously, let the host restart underneath
 * the client, and check the identity that comes back is the same one.
 *
 * Requires a DISPOSABLE host — it restarts it. Never point this at the
 * benchmark's host on 3210 while a run is in flight.
 */
const URI = process.env.LIVE_STDB_URI ?? 'http://127.0.0.1:3211';
const PORT = new URL(URI).port;
const MODULE = process.env.LIVE_STDB_MODULE ?? 'tokentest';
const DATA_DIR = process.env.LIVE_STDB_DATA_DIR ?? '';
const CLI = process.env.LIVE_STDB_CLI ?? '';

const sleep = (ms: number) => new Promise(r => setTimeout(r, ms));

async function hostAnswers(): Promise<boolean> {
  try {
    const r = await fetch(`${URI}/v1/ping`, {
      signal: AbortSignal.timeout(2000),
    });
    return r.ok;
  } catch {
    return false;
  }
}

async function waitFor(
  predicate: () => boolean | Promise<boolean>,
  ms: number,
  label: string
): Promise<void> {
  const deadline = Date.now() + ms;
  while (Date.now() < deadline) {
    if (await predicate()) return;
    await sleep(500);
  }
  throw new Error(`timed out waiting for ${label}`);
}

describe('identity survives an automatic reconnect (live host)', () => {
  test('a host restart does not mint a new identity', async ctx => {
    // Opt-in: this needs a disposable host it is allowed to restart, so it
    // skips rather than fails when one is not configured. CI has none.
    //
    //   spacetimedb-cli start --listen-addr 127.0.0.1:3211 --data-dir <dir>
    //   spacetimedb-cli publish tokentest --module-path test-app/server -s http://127.0.0.1:3211
    //   LIVE_STDB_CLI=<cli> LIVE_STDB_DATA_DIR=<dir> vitest run tests/live_reconnect_identity.test.ts
    if (!CLI || !DATA_DIR || !(await hostAnswers())) {
      ctx.skip();
      return;
    }

    const key = ConnectionManager.getKey(URI, MODULE);

    // A first-ever visitor: no stored token, so the builder carries none. This
    // is the exact shape the client skill doc prescribes via useMemo(..., []).
    const builder = DbConnection.builder()
      .withUri(URI)
      .withDatabaseName(MODULE)
      .withToken(undefined);

    ConnectionManager.retain(key, builder as any);
    await waitFor(
      () => ConnectionManager.getSnapshot(key)?.isActive === true,
      30_000,
      'the first connection'
    );

    const before = ConnectionManager.getSnapshot(key)!;
    const identityBefore = before.identity?.toHexString();
    expect(identityBefore, 'host issued no identity').toBeTruthy();
    console.log('  identity before restart:', identityBefore);

    // Restart the host underneath the client. This is the reported scenario:
    // the SDK reconnects on its own, and the application never gets a say.
    // Kill by PORT OWNER, not by process name. `spacetime start` spawns a
    // child `spacetimedb-standalone.exe` and that child is what listens, so
    // matching the CLI's name kills nothing. Targeting the owner of this port
    // also guarantees the benchmark's host on another port is never touched.
    execFileSync('powershell.exe', [
      '-NoProfile',
      '-Command',
      `Get-NetTCPConnection -LocalPort ${PORT} -State Listen ` +
        `| Select-Object -ExpandProperty OwningProcess -Unique ` +
        `| ForEach-Object { Stop-Process -Id $_ -Force }`,
    ]);
    await waitFor(async () => !(await hostAnswers()), 20_000, 'the host to go down');
    console.log('  host stopped');

    spawn(CLI, ['start', '--listen-addr', `127.0.0.1:${PORT}`, '--data-dir', DATA_DIR], {
      detached: true,
      stdio: 'ignore',
    }).unref();
    await waitFor(hostAnswers, 60_000, 'the host to come back');
    console.log('  host back up');

    // The manager reconnects on its own backoff.
    await waitFor(
      () => ConnectionManager.getSnapshot(key)?.isActive === true,
      90_000,
      'the automatic reconnect'
    );

    const after = ConnectionManager.getSnapshot(key)!;
    const identityAfter = after.identity?.toHexString();
    console.log('  identity after reconnect:', identityAfter);

    // Before the fix the reconnect presented no credentials and the host issued
    // a second, unrelated identity — silently.
    expect(identityAfter).toBe(identityBefore);

    ConnectionManager.release(key);
  }, 240_000);
});
