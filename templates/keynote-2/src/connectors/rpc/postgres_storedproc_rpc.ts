import type { RpcConnector } from '../../core/connectors.ts';
import { RpcRequest, RpcResponse } from './rpc_common.ts';

/**
 * Connector for the stored-procedure Postgres RPC server.
 * Identical interface to postgres_rpc, just points at a different port.
 */
export default function postgres_storedproc_rpc(
  url = process.env.PG_STOREDPROC_RPC_URL || 'http://127.0.0.1:4105',
): RpcConnector {
  let rpcUrl: URL | null = null;

  function ensureUrl() {
    if (!url) throw new Error('PG_STOREDPROC_RPC_URL not set');
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
        `[postgres_storedproc_rpc] invalid JSON: HTTP ${res.status} ${res.statusText} body=${text.slice(
          0,
          200,
        )}`,
      );
    }

    if (!res.ok || !json.ok) {
      throw new Error(
        `[postgres_storedproc_rpc] RPC ${name} failed: HTTP ${res.status} ${res.statusText} body=${text.slice(
          0,
          200,
        )}`,
      );
    }

    return json.result;
  }

  const root: RpcConnector = {
    name: 'postgres_storedproc_rpc',

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
      const worker = postgres_storedproc_rpc(url);
      worker.verify = async () => {
        throw new Error(
          'verify() not supported on postgres_storedproc_rpc worker; call verify() on root',
        );
      };
      delete worker.createWorker;
      return worker as RpcConnector;
    },
  };

  return root;
}
