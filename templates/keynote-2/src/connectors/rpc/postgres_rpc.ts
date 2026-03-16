import type { RpcConnector } from '../../core/connectors.ts';
import { RpcRequest, RpcResponse } from './rpc_common.ts';

export default function postgres_rpc(
  url = process.env.PG_RPC_URL || 'http://127.0.0.1:4101',
): RpcConnector {
  let rpcUrl: URL | null = null;

  function ensureUrl() {
    if (!url) throw new Error('PG_RPC_URL not set');
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

    const text = await res.text();
    let json: RpcResponse;
    try {
      json = JSON.parse(text) as RpcResponse;
    } catch {
      throw new Error(
        `[postgres_rpc] invalid JSON: HTTP ${res.status} ${res.statusText} body=${text.slice(
          0,
          200,
        )}`,
      );
    }

    if (!res.ok || !json.ok) {
      throw new Error(
        `[postgres_rpc] RPC ${name} failed: HTTP ${res.status} ${res.statusText} body=${text.slice(
          0,
          200,
        )}`,
      );
    }

    return json.result;
  }

  const root: RpcConnector = {
    name: 'postgres_rpc',

    async open() {
      await httpCall('health');
    },

    async close() {
      // no-op
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

    async verify() {
      await httpCall('verify');
    },

    async call(name: string, args?: Record<string, unknown>) {
      return httpCall(name, args);
    },

    async createWorker(): Promise<RpcConnector> {
      const worker = postgres_rpc(url);

      //no need to open

      worker.verify = async () => {
        throw new Error(
          'verify() not supported on postgres_rpc worker connector; call verify() on the root connector instead',
        );
      };
      delete worker.createWorker;
      return worker as RpcConnector;
    },
  };

  return root;
}
