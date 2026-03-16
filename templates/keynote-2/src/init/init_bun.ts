import 'dotenv/config';

export async function initBun(url?: string) {
  const bunUrl = url ?? process.env.BUN_URL;

  if (!bunUrl || !bunUrl.trim()) {
    console.log('[bun] missing BUN_URL; skipping');
    return;
  }

  let base: URL;
  try {
    base = new URL(bunUrl);
  } catch {
    console.error(`[bun] invalid BUN_URL=${bunUrl}`);
    throw new Error('invalid BUN_URL');
  }

  const rpcUrl = new URL('/rpc', base);
  console.log(`[bun] init @ ${rpcUrl.href}`);

  // 1) health
  let res: Response;
  try {
    res = await fetch(rpcUrl, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ name: 'health', args: {} }),
    });
  } catch (err) {
    console.error('[bun] request failed:', err);
    throw err;
  }

  if (!res.ok) {
    const body = await res.text().catch(() => '');
    console.error(
      `[bun] HTTP ${res.status} ${res.statusText}: ${body.slice(0, 200)}`,
    );
    throw new Error('bun health check failed');
  }

  let json: any;
  try {
    json = await res.json();
  } catch {
    console.error('[bun] invalid JSON response from Bun RPC server');
    throw new Error('bun health check returned invalid JSON');
  }

  if (!json || typeof json !== 'object' || json.ok !== true) {
    console.error(
      '[bun] health check failed:',
      typeof json === 'object' ? JSON.stringify(json) : String(json),
    );
    throw new Error('bun health check failed');
  }

  console.log('[bun] health ok');

  // 2) seed via RPC
  const seedAccounts = Number(process.env.SEED_ACCOUNTS ?? '0');
  const seedInitial = process.env.SEED_INITIAL_BALANCE;

  if (!Number.isInteger(seedAccounts) || seedAccounts <= 0 || !seedInitial) {
    console.log(
      '[bun] missing SEED_ACCOUNTS or SEED_INITIAL_BALANCE; skipping seed',
    );
    return;
  }

  console.log(
    `[bun] seeding ${seedAccounts} accounts with initial_balance=${seedInitial}`,
  );

  const seedRes = await fetch(rpcUrl, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({
      name: 'seed',
      args: {
        accounts: seedAccounts,
        initialBalance: seedInitial,
      },
    }),
  });

  if (!seedRes.ok) {
    const body = await seedRes.text().catch(() => '');
    console.error(
      `[bun] seed HTTP ${seedRes.status} ${seedRes.statusText}: ${body.slice(
        0,
        200,
      )}`,
    );
    throw new Error('bun seed failed');
  }

  const seedJson: any = await seedRes.json().catch(() => null);

  if (!seedJson || typeof seedJson !== 'object' || seedJson.ok !== true) {
    console.error(
      '[bun] seed failed:',
      typeof seedJson === 'object'
        ? JSON.stringify(seedJson)
        : String(seedJson),
    );
    throw new Error('bun seed failed');
  }

  console.log('[bun] ready');
}
