import 'dotenv/config';
import { init_supabase } from './init_supabase.ts';
import { ACC, BAL, has, sh } from './utils.ts';
import { initSpacetime } from './init_spacetime.ts';
import { initConvex } from './init_convex.ts';
import { initPgLike } from './init_pglike.ts';
import { initSqlite } from './init_sqlite.ts';
import { initBun } from './init_bun.ts';
import { initRpcServers } from './init_rpc_servers.ts';

async function main() {
  // 1) Bring up docker services
  const useDocker = process.env.USE_DOCKER === '1';
  if (useDocker) {
    console.log('\n[docker] compose up -d --build');
    await sh('docker', ['compose', 'up', '-d', '--build', '--force-recreate', "--remove-orphans"]);

    // Seed the SQLite named volume inside Docker
    console.log('[sqlite] seeding named volume via sqlite-seed...');
    await sh('docker', ['compose', 'run', '--rm', 'sqlite-seed']);
  } else {
    console.log('\n[docker] skipped (set USE_DOCKER=1 to enable)');
  }

  // 2) Init PG/CRDB/PlanetScale if URLs set
  if (has(process.env.PG_URL) && process.env.SKIP_PG !== '1') {
    await initPgLike(process.env.PG_URL!, 'postgres');
  } else {
    console.log('[postgres] skipped (set SKIP_PG=0 to enable)');
  }

  if (has(process.env.CRDB_URL) && process.env.SKIP_CRDB !== '1') {
    await initPgLike(process.env.CRDB_URL!, 'cockroach');
  } else {
    console.log('[cockroach] skipped (set SKIP_CRDB=0 to enable)');
  }

  if (
    has(process.env.PLANETSCALE_PG_URL) &&
    process.env.SKIP_PLANETSCALE_PG !== '1'
  ) {
    await initPgLike(process.env.PLANETSCALE_PG_URL!, 'planetscale');
  } else {
    console.log(
      '[planetscale_pg] skipped (set PLANETSCALE_PG_URL and SKIP_PLANETSCALE_PG=0 to enable)',
    );
  }

  // 3) Init SQLite if path set
  if (has(process.env.SQLITE_FILE) && process.env.SKIP_SQLITE !== '1')
    initSqlite(process.env.SQLITE_FILE!);

  // 4) Supabase
  if (has(process.env.SUPABASE_URL) && process.env.SKIP_SUPABASE !== '1') {
    console.log(
      '[supabase] detected env; run your SQL in the Supabase SQL editor once (see README).',
    );
    await init_supabase({
      dbUrl: process.env.SUPABASE_DB_URL,
      supabaseUrl: process.env.SUPABASE_URL,
      supabaseAnonKey: process.env.SUPABASE_ANON_KEY,
      accountCount: ACC,
      initialBalance: BAL,
    });
  }

  // 5) Bun
  if (process.env.BUN_URL && process.env.SKIP_BUN !== '1') {
    await initBun(process.env.BUN_URL);
  } else {
    console.log('[bun] skipped (set BUN_URL and SKIP_BUN=0 to enable)');
  }

  // 6) Convex
  if (has(process.env.CONVEX_URL) && process.env.SKIP_CONVEX !== '1') {
    console.log(
      '[convex] detected env; ensure debit/credit mutations + seed exist (see README).',
    );
    await initConvex();
  }

  // 7) SpacetimeDB publish/generate/seed
  await initSpacetime();

  // 8) start rpc
  await initRpcServers();

  console.log('\n[prep] All set ✅');
}

main().catch((e) => {
  console.error(e);
  process.exit(1);
});
