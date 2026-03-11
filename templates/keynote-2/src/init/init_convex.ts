const DEFAULT_CONVEX_URL = 'http://127.0.0.1:3210';

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

export async function initConvex() {
  if (process.env.SKIP_CONVEX === '1') {
    console.log('[convex] skipped (set SKIP_CONVEX=0 to enable)');
    return;
  }
  console.log('\n[convex] scaffold');

  const dir = process.env.CONVEX_DIR || './convex-app';

  const ACC = Number(process.env.SEED_ACCOUNTS ?? 100_000);
  const BAL = Number(process.env.SEED_INITIAL_BALANCE ?? 1_000_000_000);
  const url = process.env.CONVEX_URL || DEFAULT_CONVEX_URL;

  console.log(
    `[convex] expecting dev server at ${url} (start with: cd ${dir} && pnpm dev)`,
  );

  try {
    if (process.env.CLEAR_CONVEX_ON_PREP === '1') {
      console.log('[convex] clearing accounts…');
      for (;;) {
        const deleted: number = await callConvexMutation(
          url,
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
      `[convex] seeding ${ACC} accounts in chunks of ${CHUNK} (initial=${BAL})`,
    );

    for (let start = 0; start < ACC; start += CHUNK) {
      const count = Math.min(CHUNK, ACC - start);
      console.log(
        `[convex]   → seed:seed_range { start: ${start}, count: ${count} }`,
      );
      await callConvexMutation(url, 'seed:seed_range', {
        start,
        count,
        initial: BAL,
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
