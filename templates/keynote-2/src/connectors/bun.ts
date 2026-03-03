import type { RpcConnector } from '../core/connectors.ts';

export default function bun(
  url = process.env.BUN_URL || 'http://127.0.0.1:4000',
): RpcConnector {
  if (!url) throw new Error('BUN_URL not set');

  const baseUrl = url.replace(/\/+$/, '');

  async function httpCall(
    name: string,
    args?: Record<string, unknown>,
  ): Promise<unknown> {
    const res = await fetch(`${baseUrl}/rpc`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ name, args: args ?? {} }),
    });

    if (!res.ok) {
      const text = await res.text().catch(() => '');
      throw new Error(
        `[bun] RPC ${name} HTTP ${res.status} ${res.statusText}: ${text.slice(
          0,
          200,
        )}`,
      );
    }

    const json = (await res.json()) as any;

    if (json && typeof json === 'object' && typeof json.ok === 'boolean') {
      if (!json.ok) {
        throw new Error(
          `[bun] RPC ${name} failed: ${String(json.error ?? 'unknown error')}`,
        );
      }
      return json.result;
    }

    return json;
  }

  const root: RpcConnector = {
    name: 'bun',

    async open() {
      try {
        await httpCall('health').catch(() => {});
      } catch (err) {
        console.warn('[bun] health check error (continuing anyway):', err);
      }
    },

    async close() {},

    async call(name: string, args?: Record<string, unknown>): Promise<unknown> {
      return httpCall(name, args);
    },

    async getAccount(id: number) {
      const r = (await root.call('getAccount', { id })) as {
        id: number;
        balance: bigint;
      } | null;

      if (!r) return null;

      return {
        id: r.id,
        balance: r.balance,
      };
    },

    async verify() {
      await root.call('verify');
    },

    async createWorker() {
      const workerConnector: RpcConnector = {
        name: 'bun',

        async open() {},

        async close() {},

        async call(
          name: string,
          args?: Record<string, unknown>,
        ): Promise<unknown> {
          return httpCall(name, args);
        },

        async getAccount(id: number) {
          const r = (await workerConnector.call('getAccount', { id })) as {
            id: number;
            balance: bigint;
          } | null;

          if (!r) return null;

          return {
            id: r.id,
            balance: r.balance,
          };
        },

        async verify() {
          throw new Error(
            'verify() not supported on bun worker connector; call verify() on the root connector instead',
          );
        },
      };

      return workerConnector;
    },
  };

  return root;
}
