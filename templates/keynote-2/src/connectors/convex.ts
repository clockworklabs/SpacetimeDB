import type { RpcConnector } from '../core/connectors.ts';

export default function convex(url: string): RpcConnector {
  if (!url) throw new Error('CONVEX_URL not set');

  async function queryConvex(path: string, args: any) {
    const res = await fetch(`${url}/api/query?format=json`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ path, args }),
    });

    let json: any = {};
    try {
      json = await res.json();
    } catch {}

    if (res.ok && json.status === 'success') {
      return json.value;
    }

    const msg =
      json?.errorMessage ??
      json?.message ??
      `HTTP ${res.status} ${res.statusText}`;

    throw new Error(`convex query ${path} failed: ${msg}`);
  }

  async function mutationConvex(path: string, args: any) {
    const res = await fetch(`${url}/api/mutation?format=json`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ path, args }),
    });

    let json: any = {};
    try {
      json = await res.json();
    } catch {}

    if (res.ok && json.status === 'success') {
      return json.value;
    }

    const msg =
      json?.errorMessage ??
      json?.message ??
      `HTTP ${res.status} ${res.statusText}`;

    throw new Error(`convex mutation ${path} failed: ${msg}`);
  }

  const root: RpcConnector = {
    name: 'convex',

    async open() {},
    async close() {},

    async call(method: string, args?: Record<string, unknown>) {
      return mutationConvex(method, args ?? {});
    },

    async getAccount(id: number) {
      const row = await queryConvex('accounts:get_account', { id });
      if (!row) return null;
      return {
        id: Number(row.id),
        balance: BigInt(row.balance),
      };
    },

    async verify() {
      const rawInitial = process.env.SEED_INITIAL_BALANCE;
      if (!rawInitial) {
        console.warn(
          '[convex] SEED_INITIAL_BALANCE not set; skipping verification',
        );
        return;
      }

      let initial: bigint;
      try {
        initial = BigInt(rawInitial);
      } catch {
        console.error(
          `[convex] invalid SEED_INITIAL_BALANCE=${rawInitial}; expected integer`,
        );
        return;
      }

      const BATCH = 64;

      let count = 0n;
      let total = 0n;
      let changed = 0n;
      let nextId = 0;

      for (;;) {
        const ids = Array.from({ length: BATCH }, (_, i) => nextId + i);

        const rows = await Promise.all(
          ids.map((id) => queryConvex('accounts:get_account', { id })),
        );

        let hitHole = false;
        let sawAny = false;

        for (const acc of rows) {
          if (!acc) {
            hitHole = true;
            break;
          }

          sawAny = true;
          count++;

          const bal = BigInt(acc.balance);
          total += bal;
          if (bal !== initial) changed++;
        }

        if (!sawAny || hitHole) break;

        nextId += BATCH;
      }

      if (count === 0n) {
        console.error('[convex] verify failed: no accounts found');
        throw new Error('convex verification failed: no accounts found');
      }

      const expected = initial * count;

      // 1) total must be conserved
      if (total !== expected) {
        console.error(
          `[convex] verify failed: accounts=${count} total_balance=${total} expected=${expected}`,
        );
        throw new Error('convex verification failed: total_balance mismatch');
      }

      // 2) at least one row must have changed
      if (changed === 0n) {
        console.error(
          '[convex] verify failed: total preserved but no balances changed',
        );
        throw new Error(
          'convex verification failed: no account balances changed',
        );
      }

      console.log(
        `[convex] verify ok: accounts=${count} total_balance=${total} changed=${changed}`,
      );
    },

    async createWorker(): Promise<RpcConnector> {
      const worker: any = convex(url);
      await worker.open();
      worker.verify = async () => {
        throw new Error(
          'verify() not supported on convex worker connector; call verify() on the root connector instead',
        );
      };
      delete worker.createWorker;
      return worker as RpcConnector;
    },
  };

  return root;
}
