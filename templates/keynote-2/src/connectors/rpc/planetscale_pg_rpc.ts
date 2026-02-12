import { RpcConnector } from '../../core/connectors.ts';
import { RpcRequest, RpcResponse } from './rpc_common.ts';

export default function planetscale_pg_rpc(
  url = process.env.PLANETSCALE_RPC_URL || 'http://127.0.0.1:4104',
): RpcConnector {
  let rpcUrl: URL | null = null;

  function ensureUrl() {
    if (!url) throw new Error('PLANETSCALE_RPC_URL not set');
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
        `planetscale_pg_rpc HTTP ${res.status} ${res.statusText}${
          text ? ` - ${text}` : ''
        }`,
      );
    }

    const json = (await res.json()) as RpcResponse;
    if (!json.ok) {
      throw new Error(json.error || 'Unknown planetscale_pg_rpc error');
    }
    return json.result;
  }

  const root: RpcConnector = {
    name: 'planetscale_pg_rpc',

    async open() {
      ensureUrl();
    },

    async close() {},

    async verify() {
      await httpCall('verify');
    },

    async call(name: string, args?: Record<string, unknown>) {
      return httpCall(name, args);
    },

    async createWorker() {
      const worker: any = planetscale_pg_rpc(url);
      await worker.open();
      worker.verify = async () => {
        throw new Error('verify() only on root connector');
      };
      delete worker.createWorker;
      return worker as RpcConnector;
    },

    async getAccount(id: number) {
      const result = (await httpCall('getAccount', { id })) as {
        id: number;
        balance: bigint;
      } | null;

      if (!result) return null;
      return {
        id: result.id,
        balance: result.balance,
      };
    },
  };

  return root;
}
