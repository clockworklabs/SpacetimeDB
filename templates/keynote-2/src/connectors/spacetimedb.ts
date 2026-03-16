import type { ReducerConnector } from '../core/connectors';
import * as mod from '../../module_bindings';

export function spacetimedb(
  url = process.env.STDB_URL!,
  moduleName = process.env.STDB_MODULE!,
): ReducerConnector {
  let ready!: Promise<void>;
  let conn: mod.DbConnection;
  let resolveReady!: () => void;
  let rejectReady!: (e: unknown) => void;

  function armReady() {
    ready = new Promise<void>((res, rej) => {
      resolveReady = res;
      rejectReady = rej;
    });
  }

  async function connectWithBindings() {
    if (!url) throw new Error('STDB_URL not set');
    if (!moduleName) throw new Error('STDB_MODULE not set');

    const Db = mod.DbConnection;

    armReady();

    const subscriptions: string[] = [];
    if (process.env.VERIFY === '1') {
      console.log('[spacetimedb] subscribing to accounts');
      subscriptions.push('SELECT * FROM accounts');
    }
    let subscribed = subscriptions.length === 0;

    const builder = Db.builder()
      .withUri(url)
      .withDatabaseName(moduleName)
      .withConfirmedReads(process.env.STDB_CONFIRMED_READS === '1')
      .onConnect((ctx) => {
        console.log('[stdb] connected');
        const conn = ctx;

        resolveReady();

        if (subscriptions.length > 0) {
          conn
            .subscriptionBuilder()
            .onApplied((_sCtx) => {
              subscribed = true;
            })
            .onError((ctx) => {
              console.error('[stdb] subscription failed', ctx.event?.message);
            })
            .subscribe(subscriptions);
        }
      })
      .onConnectError((_ctx, err: any) => {
        console.error('[stdb] onConnectError', err);

        if (err instanceof Error) {
          rejectReady(err);
        } else if (err && typeof err.message === 'string') {
          rejectReady(new Error(err.message));
        } else {
          rejectReady(
            new Error(`Spacetime connection error: ${JSON.stringify(err)}`),
          );
        }
      })
      .onDisconnect((_ctx, _err) => {});

    conn = builder.build();

    while (!subscribed) {
      await new Promise((res) => setTimeout(res, 25));
    }
  }

  return {
    name: 'spacetimedb',
    maxInflightPerWorker: 16384,

    async open() {
      try {
        await connectWithBindings();
        await ready;
      } catch (err) {
        console.error('[spacetimedb] open() failed', err);
        throw err;
      }
    },

    async close() {
      try {
        conn.disconnect();
      } catch (e) {
        console.error('[spacetimedb] close() failed', e);
      }
    },

    async createWorker(): Promise<ReducerConnector> {
      const worker = spacetimedb(url, moduleName);
      await worker.open();
      worker.verify = async () => {
        throw new Error(
          'verify() not supported on spacetimedb worker connector; call verify() on the root connector instead',
        );
      };
      delete worker.createWorker;
      return worker;
    },

    async call(fn: string, args: Record<string, any>) {
      await ready;

      switch (fn) {
        case 'seed': {
          return conn.reducers.seed({
            n: args.accounts,
            initialBalance: args.initialBalance,
          });
        }

        case 'transfer': {
          return conn.reducers.transfer({
            from: args.from,
            to: args.to,
            amount: args.amount,
          });
        }

        default:
          throw new Error(`Unknown reducer: ${fn}`);
      }
    },

    async getAccount(id: number) {
      if (!conn) {
        throw new Error('SpacetimeDB not connected');
      }

      const accounts = conn.db?.accounts;
      if (!accounts) {
        throw new Error('SpacetimeDB not connected or accounts table missing');
      }

      const acc = accounts.id.find(id);
      if (!acc) return null;

      return {
        id: acc.id,
        balance: acc.balance,
      };
    },

    async verify() {
      if (!conn) throw new Error('SpacetimeDB not connected');

      const rawInitial = process.env.SEED_INITIAL_BALANCE;
      if (!rawInitial) {
        console.warn(
          '[spacetimedb] SEED_INITIAL_BALANCE not set; skipping verification',
        );
        return;
      }

      const initial = BigInt(rawInitial);

      const accounts = conn.db?.accounts;
      if (!accounts) {
        console.error(
          '[spacetimedb] verify failed: accounts table missing/invalid',
        );
        throw new Error(
          'spacetimedb verification failed: accounts table missing/invalid',
        );
      }

      let count = 0;
      let total = 0n;
      let changed = 0n;

      console.log(`[spacetimedb] verifying ${accounts.count()} accounts...`);
      for (const row of accounts.iter()) {
        count++;
        const bal = row.balance;
        total += bal;
        if (bal !== initial) changed++;
      }

      const countBig = BigInt(count);
      if (countBig === 0n) {
        console.error('[spacetimedb] verify failed: accounts=0');
        throw new Error('spacetimedb verification failed: no accounts');
      }

      const expected = initial * countBig;

      if (total !== expected) {
        console.error(
          `[spacetimedb] verify failed: accounts=${countBig} total_balance=${total} expected=${expected}`,
        );
        throw new Error(
          'spacetimedb verification failed: total_balance mismatch',
        );
      }

      if (changed === 0n) {
        console.error(
          '[spacetimedb] verify failed: total preserved but no balances changed',
        );
        throw new Error(
          'spacetimedb verification failed: no account balances changed',
        );
      }

      console.log(
        `[spacetimedb] verify ok: accounts=${countBig} total_balance=${total} changed=${changed}`,
      );
    },
  };
}
