import { createClient, SupabaseClient } from '@supabase/supabase-js';
import { RpcConnector } from '../core/connectors.ts';

function formatSupabaseError(err: any): string {
  if (!err) return 'Unknown Supabase error';
  const parts = [
    err.message ?? '',
    err.details ?? '',
    err.hint ? `hint: ${err.hint}` : '',
    err.code ? `code: ${err.code}` : '',
  ]
    .map((s) => String(s).trim())
    .filter(Boolean);
  return parts.join(' | ');
}

export default function supabase(
  url = process.env.SUPABASE_URL!,
  anon = process.env.SUPABASE_ANON_KEY!,
): RpcConnector {
  let rootClient: SupabaseClient | undefined;

  function createSupabaseClient(): SupabaseClient {
    if (!url || !anon) {
      throw new Error('SUPABASE_URL / SUPABASE_ANON_KEY not set');
    }
    return createClient(url, anon, { auth: { persistSession: false } });
  }

  function ensureRootClient(): SupabaseClient {
    if (!rootClient) {
      rootClient = createSupabaseClient();
    }
    return rootClient;
  }

  const connector: RpcConnector = {
    name: 'supabase',

    async open() {
      ensureRootClient();
    },

    async close() {
      /* noop */
    },

    async call(method: string, args?: Record<string, unknown>) {
      const sb = ensureRootClient();

      try {
        const { data, error } = await sb.rpc(method, (args ?? {}) as any);
        if (error) {
          throw new Error(formatSupabaseError(error));
        }
        return data as any;
      } catch (err: any) {
        if (err instanceof Error) throw err;
        throw new Error(`Supabase RPC failed: ${JSON.stringify(err)}`);
      }
    },

    async getAccount(id: number) {
      const sb = ensureRootClient();

      const { data, error } = await sb
        .from('accounts')
        .select('id,balance')
        .eq('id', id)
        .maybeSingle();

      if (error) {
        throw new Error(formatSupabaseError(error));
      }
      if (!data) return null;

      return {
        id: Number(data.id),
        balance: BigInt(data.balance),
      };
    },

    async verify() {
      const sb = ensureRootClient();

      const rawInitial = process.env.SEED_INITIAL_BALANCE;
      if (!rawInitial) {
        console.warn(
          '[supabase] SEED_INITIAL_BALANCE not set; skipping verification',
        );
        return;
      }

      let initial: bigint;
      try {
        initial = BigInt(rawInitial);
      } catch {
        console.error(
          `[supabase] invalid SEED_INITIAL_BALANCE=${rawInitial}; expected integer`,
        );
        return;
      }

      const PAGE_SIZE = 1000;

      let offset = 0;
      let count = 0n;
      let total = 0n;
      let changed = 0n;

      for (;;) {
        const { data, error } = await sb
          .from('accounts')
          .select('balance')
          .range(offset, offset + PAGE_SIZE - 1);

        if (error) {
          throw new Error(formatSupabaseError(error));
        }

        const rows = (data ?? []) as { balance: bigint }[];

        if (rows.length === 0) break;

        for (const row of rows) {
          const bal = row.balance;
          total += bal;
          count++;
          if (bal !== initial) changed++;
        }

        if (rows.length < PAGE_SIZE) break;
        offset += rows.length;
      }

      if (count === 0n) {
        console.error('[supabase] verify failed: accounts=0');
        throw new Error('supabase verification failed: no accounts');
      }

      const expected = initial * count;

      // 1) total must be conserved
      if (total !== expected) {
        console.error(
          `[supabase] verify failed: accounts=${count} total_balance=${total} expected=${expected}`,
        );
        throw new Error('supabase verification failed: total_balance mismatch');
      }

      // 2) at least one row must have changed
      if (changed === 0n) {
        console.error(
          '[supabase] verify failed: total preserved but no balances changed',
        );
        throw new Error(
          'supabase verification failed: no account balances changed',
        );
      }

      console.log(
        `[supabase] verify ok: accounts=${count} total_balance=${total} changed=${changed}`,
      );
    },

    async createWorker() {
      const sb = createSupabaseClient();

      const worker: RpcConnector = {
        name: 'supabase',

        async open() {
          /* noop */
        },

        async close() {
          /* noop */
        },

        async call(method: string, args?: Record<string, unknown>) {
          try {
            const { data, error } = await sb.rpc(method, (args ?? {}) as any);
            if (error) {
              throw new Error(formatSupabaseError(error));
            }
            return data as any;
          } catch (err: any) {
            if (err instanceof Error) throw err;
            throw new Error(`Supabase RPC failed: ${JSON.stringify(err)}`);
          }
        },

        async getAccount(id: number) {
          const { data, error } = await sb
            .from('accounts')
            .select('id,balance')
            .eq('id', id)
            .maybeSingle();

          if (error) {
            throw new Error(formatSupabaseError(error));
          }
          if (!data) return null;

          return {
            id: Number(data.id),
            balance: BigInt(data.balance as any),
          };
        },

        async verify() {
          throw new Error(
            'verify() not supported on supabase worker; call verify() on the root connector instead',
          );
        },
      };

      return worker;
    },
  };

  return connector;
}
