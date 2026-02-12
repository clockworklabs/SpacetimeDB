import type { RpcConnector } from '../../core/connectors.ts';
import { RpcRequest, RpcResponse } from './rpc_common.ts';

export default function cockroach_rpc(
  url = process.env.CRDB_RPC_URL || 'http://127.0.0.1:4102',
): RpcConnector {
  let rpcUrl: URL | null = null;

  function ensureUrl() {
    if (!url) throw new Error('CRDB_RPC_URL not set');
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
        `[cockroach_rpc] invalid JSON: HTTP ${res.status} ${res.statusText} body=${text.slice(
          0,
          200,
        )}`,
      );
    }

    if (!res.ok || !json.ok) {
      throw new Error(
        `[cockroach_rpc] RPC ${name} failed: HTTP ${res.status} ${res.statusText} body=${text.slice(
          0,
          200,
        )}`,
      );
    }

    return json.result;
  }

  async function callWithRetry(
    name: string,
    args?: Record<string, unknown>,
    maxRetries: number = 5,
  ) {
    let attempts = 0;
    while (attempts < maxRetries) {
      try {
        return await httpCall(name, args);
      } catch (err: unknown) {
        let errMsg = 'Unknown error';
        if (err instanceof Error) {
          errMsg = err.message;
        } else if (typeof err === 'string') {
          errMsg = err;
        }
        if (
          errMsg.includes('serialization') ||
          errMsg.includes('restart transaction') ||
          errMsg.includes('40001')
        ) {
          attempts++;
          if (attempts >= maxRetries) throw err;
          continue;
        }
        throw err;
      }
    }
    throw new Error('Max retries exceeded');
  }

  const root: RpcConnector = {
    name: 'cockroach_rpc',

    // Only a single health check on the root.
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
      if (name === 'transfer') {
        return callWithRetry(name, args);
      }
      return httpCall(name, args);
    },

    async createWorker(): Promise<RpcConnector> {
      const worker = cockroach_rpc(url);

      worker.open = async () => {};

      worker.verify = async () => {
        throw new Error(
          'verify() not supported on cockroach_rpc worker connector; call verify() on the root connector instead',
        );
      };

      delete worker.createWorker;
      return worker as RpcConnector;
    },
  };

  return root;
}
