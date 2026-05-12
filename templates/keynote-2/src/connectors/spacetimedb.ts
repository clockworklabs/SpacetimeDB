import type { ReducerConnector } from '../core/connectors';
import * as mod from '../../module_bindings';
import { deriveWebsocketUrl } from '../core/stdbUrl';
import type { SpacetimeConnectorConfig } from '../config.ts';

export function spacetimedb(config: SpacetimeConnectorConfig): ReducerConnector {
  const {
    initialBalance,
    stdbCompression,
    stdbConfirmedReads,
    stdbModule: moduleName,
    stdbUrl: url,
  } = config;

  let ready: ReturnType<typeof Promise.withResolvers<void>>;
  let conn: mod.DbConnection;

  async function connectWithBindings() {
    if (!url) throw new Error('STDB_URL not set');
    if (!moduleName) throw new Error('STDB_MODULE not set');

    const Db = mod.DbConnection;

    ready = Promise.withResolvers<void>();

    const subscriptions: string[] = [];
    if (process.env.VERIFY === '1') {
      console.log('[spacetimedb] subscribing to accounts');
      subscriptions.push('SELECT * FROM accounts');
    }
    const subscribed = Promise.withResolvers<void>();
    if (subscriptions.length === 0) subscribed.resolve();

    const builder = Db.builder()
      .withUri(deriveWebsocketUrl(url))
      .withDatabaseName(moduleName)
      .withCompression(stdbCompression)
      .withConfirmedReads(stdbConfirmedReads)
      .onConnect((ctx) => {
        console.log('[stdb] connected');
        const conn = ctx;

        ready.resolve();

        if (subscriptions.length > 0) {
          conn
            .subscriptionBuilder()
            .onApplied((_sCtx) => {
              subscribed.resolve();
            })
            .onError((ctx) => {
              console.error('[stdb] subscription failed', ctx.event?.message);
              subscribed.reject(ctx.event);
            })
            .subscribe(subscriptions);
        }
      })
      .onConnectError((_ctx, err: any) => {
        if (err instanceof Error) {
          ready.reject(err);
        } else if (err && err.error instanceof Error) {
          ready.reject(err.error);
        } else if (err && typeof err.message === 'string') {
          ready.reject(new Error(err.message));
        } else {
          ready.reject(
            new Error(`Spacetime connection error: ${JSON.stringify(err)}`),
          );
        }
      })
      .onDisconnect((_ctx, _err) => {});

    conn = builder.build();

    await ready.promise;
    await subscribed.promise;
  }

  return {
    name: 'spacetimedb',
    maxInflightPerWorker: 128,

    async open() {
      try {
        await connectWithBindings();
        await ready.promise;
      } catch (err) {
        console.error('[spacetimedb] open() failed:', err);
        throw err;
      }
    },

    async close() {
      try {
        conn.disconnect();
      } catch (e) {
        console.error('[spacetimedb] close() failed:', e);
      }
    },

    async createWorker(): Promise<ReducerConnector> {
      const worker = spacetimedb(config);
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
      await ready.promise;

      switch (fn) {
        case 'seed': {
          return conn.reducers.seed({
            n: args.accounts,
            initialBalance: BigInt(args.initialBalance),
          });
        }

        case 'transfer': {
          return conn.reducers.transfer({
            from: args.from,
            to: args.to,
            amount: args.amount,
          });
        }

        case 'transfer_with_audit': {
          return conn.reducers.transferWithAudit({
            from: args.from,
            to: args.to,
            amount: args.amount,
            fraudLimit: BigInt((args.fraudLimit ?? args.fraud_limit) as any),
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

      const rawInitial = initialBalance;
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
