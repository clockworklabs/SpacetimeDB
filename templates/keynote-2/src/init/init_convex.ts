import type { ConvexInitConfig } from '../config.ts';

async function callConvexMutation(url: string, pathName: string, args: any) {
  const res = await fetch(`${url}/api/mutation?format=json`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ path: pathName, args }),
  });

  const json = await res.json().catch(() => ({}) as any);
  if (!res.ok || json.status !== 'success') {
    const msg =
      json?.errorMessage ??
      json?.message ??
      `HTTP ${res.status} ${res.statusText}`;
    throw new Error(`convex mutation ${pathName} failed: ${msg}`);
  }
  return json.value;
}

export async function initConvex(config: ConvexInitConfig) {
  if (process.env.SKIP_CONVEX === '1') {
    console.log('[convex] skipped (set SKIP_CONVEX=0 to enable)');
    return;
  }
  console.log('\n[convex] scaffold');

  const { accounts, convexDir, convexUrl, initialBalance } = config;

  console.log(
    `[convex] expecting dev server at ${convexUrl} (start with: cd ${convexDir} && pnpm dev)`,
  );

  try {
    if (process.env.CLEAR_CONVEX_ON_PREP === '1') {
      console.log('[convex] clearing accounts…');
      for (;;) {
        const deleted: number = await callConvexMutation(
          convexUrl,
          'seed:clear_accounts',
          {},
        );
        console.log(`[convex]   → deleted batch of ${deleted}`);
        if (!deleted) break;
      }
    } else {
      console.log(
        '[convex] skipping account clearing (set CLEAR_CONVEX_ON_PREP=1 to enable)',
      );
    }

    // Max ~16k writes per function; keep a safety margin
    const CHUNK = 10_000;
    console.log(
      `[convex] seeding ${accounts} accounts in chunks of ${CHUNK} (initial=${initialBalance})`,
    );

    for (let start = 0; start < accounts; start += CHUNK) {
      const count = Math.min(CHUNK, accounts - start);
      console.log(
        `[convex]   → seed:seed_range { start: ${start}, count: ${count} }`,
      );
      await callConvexMutation(convexUrl, 'seed:seed_range', {
        start,
        count,
        initial: initialBalance,
      });
    }

    console.log('[convex] seed complete.');
  } catch (err) {
    console.warn(
      `[convex] seed failed: ${(err as Error).message}\n` +
        `Make sure "convex dev" is running and that "seed:clear_accounts" / "seed:seed_range" exist.`,
    );
  }
}
