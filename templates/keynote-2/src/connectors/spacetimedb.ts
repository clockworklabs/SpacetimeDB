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

  // --- reducer completion tracking  ------------------------
  const transferWaiters = new Map<
    bigint,
    { resolve: () => void; reject: (e: unknown) => void }
  >();

  let nextTransferId = 1n;
  let transferHooked = false;

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
      .withModuleName(moduleName)
      .withConfirmedReads(process.env.STDB_CONFIRMED_READS === '1')
      .onConnect((ctx) => {
        console.log('[stdb] connected');
        const conn = ctx;

        const reducers = conn.reducers;

        if (
          process.env.USE_SPACETIME_METRICS_ENDPOINT === '0' &&
          !transferHooked
        ) {
          transferHooked = true;
          console.log('[stdb] hooking onTransfer');
          (reducers as any).onTransfer(
            (eventCtx: any, args: { from: number; to: number; amount: bigint; clientTxnId: bigint }) => {
              const clientTxnId = args.clientTxnId;
              // console.log('[stdb] onTransfer fired', { ...args, status: eventCtx?.event?.status });

              const waiter = transferWaiters.get(clientTxnId);
              if (!waiter) {
                console.warn(
                  '[stdb] no waiter for clientTxnId',
                  clientTxnId.toString(),
                );
                return;
              }

              transferWaiters.delete(clientTxnId);

              const status = eventCtx?.event?.status;

              if (status?.tag === 'Committed') {
                waiter.resolve();
              } else if (status?.tag === 'Failed') {
                waiter.reject(new Error(status?.value ?? 'transfer failed'));
              } else if (status?.tag === 'OutOfEnergy') {
                waiter.reject(new Error('transfer out of energy'));
              } else {
                waiter.reject(new Error('unknown transfer status'));
              }
            },
          );
        }

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
      const err = new Error('SpacetimeDB connection closed');

      // Fail any in-flight transfers
      for (const waiter of transferWaiters.values()) {
        try {
          waiter.reject(err);
        } catch {
          /* ignore */
        }
      }
      transferWaiters.clear();
      transferHooked = false;

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

    async reducer(fn: string, args: Record<string, any>) {
      await ready;

      switch (fn) {
        case 'seed': {
          conn.reducers.seed({
            n: args.n,
            initialBalance: args.initial_balance,
          });
          return;
        }

        case 'createAccount': {
          conn.reducers.createAccount({ id: args.id, balance: args.balance });
          return;
        }

        case 'transfer': {
          const clientTxnId = nextTransferId++;

          if (process.env.USE_SPACETIME_METRICS_ENDPOINT === '0') {
            return new Promise<void>((resolve, reject) => {
              const waiter = { resolve, reject };
              transferWaiters.set(clientTxnId, waiter);

              try {
                conn.reducers.transfer({
                  from: args.from,
                  to: args.to,
                  amount: args.amount,
                  clientTxnId,
                });
              } catch (err) {
                console.log(`ERROR ${err}`);
                transferWaiters.delete(clientTxnId);
                reject(err);
              }
            });
          } else {
            return conn.reducers.transfer({
              from: args.from,
              to: args.to,
              amount: args.amount,
              clientTxnId,
            });
          }
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

      let initial = BigInt(rawInitial);

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
