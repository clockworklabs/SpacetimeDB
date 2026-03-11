import type { RpcConnector } from '../../core/connectors.ts';
import { RpcRequest, RpcResponse } from './rpc_common.ts';

export default function sqlite_rpc(
  url = process.env.SQLITE_RPC_URL || 'http://127.0.0.1:4103',
): RpcConnector {
  let rpcUrl: URL | null = null;

  function ensureUrl() {
    if (!url) {
      throw new Error('SQLITE_RPC_URL not set');
    }
    if (!rpcUrl) {
      rpcUrl = new URL('/rpc', new URL(url));
    }
  }

  async function httpCall(name: string, args?: Record<string, unknown>) {
    ensureUrl();
    const body: RpcRequest = { name, args };

    let res: Response;
    try {
      res = await fetch(rpcUrl!, {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify(body),
      });
    } catch (err) {
      throw new Error(`[sqlite_rpc] RPC ${name} network error: ${String(err)}`);
    }

    const text = await res.text();
    let json: RpcResponse;
    try {
      json = JSON.parse(text) as RpcResponse;
    } catch {
      throw new Error(
        `[sqlite_rpc] RPC ${name} invalid JSON: HTTP ${res.status} ${
          res.statusText
        } body=${text.slice(0, 200)}`,
      );
    }

    if (!res.ok || !json.ok) {
      const msg =
        !json.ok && json.error
          ? json.error
          : `HTTP ${res.status} ${res.statusText}`;
      throw new Error(
        `[sqlite_rpc] RPC ${name} failed: ${msg} body=${text.slice(0, 200)}`,
      );
    }

    return json.result;
  }

  const connector: RpcConnector = {
    name: 'sqlite_rpc',

    async open() {
      // Root health check; runner only calls this on the root connector.
      await httpCall('health');
    },

    async close() {
      // no-op; RPC server lifetime is managed outside this process
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

    async createWorker() {
      return sqlite_rpc(url);
    },
  };

  return connector;
}
