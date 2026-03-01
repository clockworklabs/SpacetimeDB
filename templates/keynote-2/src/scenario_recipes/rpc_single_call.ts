import type { RpcConnector } from '../core/connectors';

export async function rpc_single_call(
  conn: RpcConnector,
  from: number,
  to: number,
  amount: number,
): Promise<void> {
  if (from === to || amount <= 0) return;

  const api = conn as RpcConnector;

  // If this env var is "1", call the sharded Convex transfer
  const useShardedConvex = process.env.CONVEX_USE_SHARDED_COUNTER === '1';

  const fn =
    api.name === 'convex'
      ? useShardedConvex
        ? 'transfer:transfer_sharded'
        : 'transfer:transfer'
      : 'transfer';

  for (let attempts = 0; attempts < 3; attempts++) {
    try {
      await api.call(fn, { amount, from_id: from, to_id: to });
      return;
    } catch (e: any) {
      const msg = String(e?.message ?? '');
      const retriable = /429|502|503|504/.test(msg);
      if (!retriable || attempts === 2) throw e;

      await new Promise((r) => setTimeout(r, 50 * (attempts + 1)));
    }
  }
}
