import type { ReducerConnector } from '../core/connectors';

export async function reducer_single(
  conn: unknown,
  from: number,
  to: number,
  amount: number,
): Promise<void> {
  if (from === to || amount <= 0) return;

  await (conn as ReducerConnector).reducer('transfer', {
    from,
    to,
    amount: BigInt(amount),
  });
}
