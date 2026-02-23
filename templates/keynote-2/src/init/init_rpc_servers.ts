import { spawnServer } from './utils.ts';

type RpcServerConfig = {
  name: string;
  urlEnv: string;
  defaultUrl: string;
  command: string;
};

async function rpcHealthCheck(baseUrl: string): Promise<boolean> {
  try {
    const url = new URL('/rpc', new URL(baseUrl));
    const res = await fetch(url, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ name: 'health', args: {} }),
    });

    if (!res.ok) return false;

    const json = await res.json().catch(() => null);
    return !!json && typeof json === 'object' && json.ok === true;
  } catch {
    return false;
  }
}

async function ensureRpcServer(cfg: RpcServerConfig) {
  const url = process.env[cfg.urlEnv] || cfg.defaultUrl;
  const healthy = await rpcHealthCheck(url);

  if (healthy) {
    console.log(`[rpc] ${cfg.name} already running @ ${url}, skipping spawn`);
    return;
  }

  console.log(`[rpc] ${cfg.name} not responding @ ${url}, starting...`);
  spawnServer(cfg.command, cfg.name);
}

export async function initRpcServers() {
  const enabled = process.env.ENABLE_RPC_SERVERS !== '0';
  if (!enabled) {
    console.log('\n[rpc] skipped (ENABLE_RPC_SERVERS=0)');
    return;
  }

  console.log('\n[rpc] starting Node RPC servers (if not already running)');

  await Promise.all([
    ensureRpcServer({
      name: 'postgres_rpc',
      urlEnv: 'PG_RPC_URL',
      defaultUrl: 'http://127.0.0.1:4101',
      command: 'pnpm tsx src/rpc-servers/postgres-rpc-server.ts',
    }),
    ensureRpcServer({
      name: 'cockroach_rpc',
      urlEnv: 'CRDB_RPC_URL',
      defaultUrl: 'http://127.0.0.1:4102',
      command: 'pnpm tsx src/rpc-servers/cockroach-rpc-server.ts',
    }),
    ensureRpcServer({
      name: 'sqlite_rpc',
      urlEnv: 'SQLITE_RPC_URL',
      defaultUrl: 'http://127.0.0.1:4103',
      command: 'pnpm tsx src/rpc-servers/sqlite-rpc-server.ts',
    }),
    ensureRpcServer({
      name: 'supabase_rpc',
      urlEnv: 'SUPABASE_RPC_URL',
      defaultUrl: 'http://127.0.0.1:4106',
      command: 'pnpm tsx src/rpc-servers/supabase-rpc-server.ts',
    }),
    ensureRpcServer({
      name: 'planetscale_pg_rpc',
      urlEnv: 'PLANETSCALE_RPC_URL',
      defaultUrl: 'http://127.0.0.1:4104',
      command: 'pnpm tsx src/rpc-servers/postgres-rpc-server.ts',
    }),
  ]);
}
