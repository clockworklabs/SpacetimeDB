import type { BaseConnector } from '../core/connectors';

// Multi-step "typical app" transfer: read source balance, fraud-check,
// transfer, append audit. The exact shape depends on which RPC name we
// call — connectors that support `transfer_with_audit` execute all four
// steps in one server-side call (STDB reducer / PG stored proc).
// Connectors that support `transfer_with_audit_steps` execute the same
// logic as four separate client→server round-trips (BEGIN→SELECT→UPDATE
// →UPDATE→INSERT→COMMIT for PG; not used by STDB since reducers are
// inherently atomic).

const FRAUD_LIMIT = BigInt(process.env.FRAUD_LIMIT ?? '1000000000');

export function makeMultiStepRecipe(rpcName: 'transfer_with_audit' | 'transfer_with_audit_steps') {
  return async function multi_step_transfer(
    conn: BaseConnector,
    from: number,
    to: number,
    amount: number,
  ): Promise<void> {
    if (from === to || amount <= 0) return;
    const api = conn as BaseConnector & {
      call: (name: string, args?: Record<string, unknown>) => Promise<unknown>;
    };
    const amountBigInt = BigInt(amount);
    for (let attempts = 0; attempts < 3; attempts++) {
      try {
        await api.call(rpcName, {
          amount: amountBigInt,
          from_id: from,
          to_id: to,
          from,
          to,
          fraudLimit: FRAUD_LIMIT,
          fraud_limit: FRAUD_LIMIT,
        });
        return;
      } catch (e: any) {
        const msg = String(e?.message ?? '');
        const retriable = /429|502|503|504/.test(msg);
        if (!retriable || attempts === 2) throw e;
        await new Promise((r) => setTimeout(r, 50 * (attempts + 1)));
      }
    }
  };
}
