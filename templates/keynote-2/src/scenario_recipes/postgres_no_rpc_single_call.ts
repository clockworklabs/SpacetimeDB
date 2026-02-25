import type { RpcConnector } from '../core/connectors';

export async function postgres_no_rpc_single_call(
  conn: unknown,
  from: number,
  to: number,
  amount: number,
): Promise<void> {
  if (from === to || amount <= 0) return;
  const connector = conn as RpcConnector;
  const fn = 'transfer';
  try {
    await connector.call(fn, {from: from, to: to, amount: amount});
  } catch (err) {
    console.error(`[postgres_single_call] ${fn} failed:`, err);
  }
  return;
}
