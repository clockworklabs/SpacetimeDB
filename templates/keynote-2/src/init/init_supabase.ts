import pg, { Client as Pg } from 'pg';
import { setTimeout as sleep } from 'node:timers/promises';

type PrepOpts = {
  dbUrl?: string;
  supabaseUrl?: string;
  supabaseAnonKey?: string;
  accountCount?: number;
  initialBalance?: number;
  maxReloadWaitMs?: number;
};

export async function init_supabase(opts: PrepOpts = {}) {
  const dbUrl =
    opts.dbUrl ??
    process.env.SUPABASE_DB_URL ??
    'postgresql://postgres:postgres@127.0.0.1:54322/postgres';

  const supabaseUrl =
    opts.supabaseUrl ?? process.env.SUPABASE_URL ?? 'http://127.0.0.1:54321';

  const anon = opts.supabaseAnonKey ?? process.env.SUPABASE_ANON_KEY ?? '';

  const N = Number.isFinite(opts.accountCount)
    ? (opts.accountCount as number)
    : 2;
  const initial = opts.initialBalance ?? 1_000_000;
  const maxReloadWaitMs = opts.maxReloadWaitMs ?? 10_000;

  // Ensure REST is up before touching DB through PostgREST later.
  await waitForRest(`${supabaseUrl}/health`);

  const dbName = await ensureSchema(dbUrl);
  console.log(`[supabase] ensured schema in db=${dbName}`);

  const inserted = await ensureAccountsRange(dbUrl, N, initial);
  console.log(`[supabase] seeded [0..${N - 1}] inserted=${inserted}`);

  await notifyReload(dbUrl);
  await waitForRpcVisible({ supabaseUrl, anon }, maxReloadWaitMs);
}

/* -------------------- internals -------------------- */
async function ensureSchema(dbUrl: string): Promise<string> {
  const sql = `
    create table if not exists public.accounts(
                                                id bigint primary key,
                                                balance bigint not null
    );

    create or replace function public.seed_accounts(n int, initial bigint)
    returns bigint
    language plpgsql
    security definer
    as $$
    declare
    seeded bigint;
    begin
      delete from public.accounts;

      insert into public.accounts(id, balance)
      select i, initial
      from generate_series(0, n - 1) as g(i);

      get diagnostics seeded = row_count;
      return seeded;
    end;
    $$;

    create or replace function public.transfer(
      amount bigint,
      from_id bigint,
      to_id bigint
    )
    returns void
    language plpgsql
    security definer
    as $$
    declare
    a bigint;
      b bigint;
      from_bal bigint;
    begin
      if amount is null or from_id is null or to_id is null then
        raise exception 'arg_null amount=%, from_id=%, to_id=%', amount, from_id, to_id;
    end if;

      if amount <= 0 then
        raise exception 'non_positive_amount';
    end if;

      if from_id = to_id then
        raise exception 'same_account';
    end if;

      a := least(from_id, to_id);
      b := greatest(from_id, to_id);

      perform 1
      from public.accounts
      where id = a
      for update;

    if not found then
        raise exception 'account_not_found id=%', a;
    end if;

      perform 1
      from public.accounts
      where id = b
      for update;

    if not found then
        raise exception 'account_not_found id=%', b;
    end if;

      select balance
      into from_bal
      from public.accounts
      where id = from_id;

      if from_bal < amount then
        raise exception 'insufficient_funds balance=%, amount=%', from_bal, amount;
    end if;

      update public.accounts
      set balance = balance
      + case
          when id = from_id then -amount
          when id = to_id   then  amount
          else 0
                    end
    where id in (from_id, to_id);
    end;
    $$;

    grant usage on schema public to anon, authenticated, service_role;
    grant execute on function public.transfer(bigint,bigint,bigint), public.seed_accounts(int,bigint)
      to anon, authenticated, service_role;

    select pg_notify('pgrst','reload schema');
  `;

  const useSsl = /supabase\.co/.test(dbUrl);

  const client = new pg.Client({
    connectionString: dbUrl,
    ssl: useSsl ? { rejectUnauthorized: false } : false,
  });

  await client.connect();
  try {
    const { rows } = await client.query<{ db: string }>(
      'select current_database() as db;',
    );
    const dbName = rows[0].db;

    await client.query('begin');
    await client.query(sql);
    await client.query('commit');

    return dbName;
  } catch (e) {
    try {
      await client.query('rollback');
    } catch {
      // ignore rollback errors
    }
    throw e;
  } finally {
    await client.end();
  }
}

async function ensureAccountsRange(
  dbUrl: string,
  count: number,
  initial: number,
): Promise<number> {
  if (!(count > 0)) return 0;
  const c = new Pg({ connectionString: dbUrl, ssl: false });
  await c.connect();
  try {
    await c.query('SET synchronous_commit = on');
    await c.query("SET work_mem = '64MB'");

    await c.query('begin');
    const r = await c.query<{ seeded: string }>(
      `select public.seed_accounts($1::int, $2::bigint) as seeded;`,
      [count, initial],
    );
    await c.query('commit');
    return Number(r.rows[0].seeded);
  } catch (e) {
    await c.query('rollback');
    throw e;
  } finally {
    await c.end();
  }
}

async function notifyReload(dbUrl: string) {
  const c = new Pg({ connectionString: dbUrl, ssl: false });
  await c.connect();
  try {
    await c.query(`select pg_notify('pgrst','reload schema');`);
  } finally {
    await c.end();
  }
}

async function waitForRpcVisible(
  opts: { supabaseUrl: string; anon: string },
  timeoutMs: number,
) {
  const deadline = Date.now() + timeoutMs;
  for (;;) {
    const r = await fetch(`${opts.supabaseUrl}/rest/v1/rpc/transfer`, {
      method: 'POST',
      headers: {
        apikey: opts.anon,
        authorization: `Bearer ${opts.anon}`,
        'content-type': 'application/json',
      },
      body: JSON.stringify({ amount: 0, from_id: 0, to_id: 0 }),
    });
    if (r.status !== 404) return;
    if (Date.now() > deadline) {
      const body = await r.text();
      throw new Error(
        `RPC not visible after reload window. Last body: ${body}`,
      );
    }
    await sleep(250);
  }
}

async function waitForRest(url: string, timeoutMs = 20_000) {
  const start = Date.now();
  for (;;) {
    try {
      const r = await fetch(url, { method: 'GET' });
      if (r.status >= 200 && r.status < 500) return;
    } catch {}
    if (Date.now() - start > timeoutMs) {
      throw new Error(
        `Supabase REST not reachable at ${url} within ${timeoutMs}ms`,
      );
    }
    await sleep(300);
  }
}
