import { RpcConnector } from '../../core/connectors.ts';
import { RpcRequest, RpcResponse } from './rpc_common.ts';

export default function supabase_rpc(
  url = process.env.SUPABASE_RPC_URL || 'http://127.0.0.1:4106',
): RpcConnector {
  let rpcUrl: URL | null = null;

  function ensureUrl() {
    if (!url) throw new Error('SUPABASE_RPC_URL not set');
    if (!rpcUrl) rpcUrl = new URL('/rpc', new URL(url));
  }

  async function httpCall(name: string, args?: Record<string, unknown>) {
    ensureUrl();

    const body: RpcRequest = { name, args };

    const res = await fetch(rpcUrl!, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(body),
    });

    if (!res.ok) {
      const text = await res.text().catch(() => '');
      throw new Error(
        `supabase_rpc HTTP ${res.status} ${res.statusText}${
          text ? ` - ${text}` : ''
        }`,
      );
    }

    let json: RpcResponse;
    try {
      json = (await res.json()) as RpcResponse;
    } catch (e: any) {
      throw new Error(
        `supabase_rpc returned invalid JSON: ${e?.message ?? String(e)}`,
      );
    }

    if (!json.ok) {
      throw new Error(json.error || 'Unknown supabase_rpc error');
    }

    return json.result;
  }

  const root: RpcConnector = {
    name: 'supabase_rpc',

    async open() {
      // Just validate URL; real connections happen on the server.
      ensureUrl();
    },

    async close() {
      // No persistent client state on the bench side.
    },

    async verify() {
      await httpCall('verify');
    },

    async call(name: string, args?: Record<string, unknown>): Promise<unknown> {
      return httpCall(name, args);
    },

    async getAccount(
      id: number,
    ): Promise<{ id: number; balance: bigint } | null> {
      const result = await httpCall('getAccount', { id });
      if (!result) return null;

      const r = result as { id: number; balance: bigint };

      return { id: r.id, balance: r.balance };
    },

    async createWorker() {
      const worker: any = supabase_rpc(url);
      await worker.open();

      worker.verify = async () => {
        throw new Error(
          'verify() not supported on supabase_rpc worker; call verify() on the root connector instead',
        );
      };

      delete worker.createWorker;
      return worker as RpcConnector;
    },
  };

  return root;
}
