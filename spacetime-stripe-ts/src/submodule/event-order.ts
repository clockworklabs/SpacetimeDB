// Stripe does not guarantee webhook delivery order, and replays re-deliver old
// payloads. Each mirrored row records the Stripe time of the state it holds
// and a status rank; an update is applied only when it is not older and does
// not move the status backward.
export type OrderedState = { createdUnix: bigint; rank: number };

export function isStale(
  existing: OrderedState | undefined,
  next: OrderedState
): boolean {
  if (!existing) return false;
  return next.createdUnix < existing.createdUnix || next.rank < existing.rank;
}

// `incomplete` only precedes other states; `canceled` and `incomplete_expired`
// are final.
export function subscriptionStatusRank(status: string): number {
  if (status === 'incomplete') return 0;
  if (status === 'canceled' || status === 'incomplete_expired') return 2;
  return 1;
}

// Invoices move draft -> open -> (uncollectible) -> paid or void.
export function invoiceStatusRank(status: string): number {
  switch (status) {
    case 'draft':
      return 0;
    case 'uncollectible':
      return 2;
    case 'paid':
    case 'void':
      return 3;
    default:
      return 1;
  }
}

// Checkout `payment_status` moves from `unpaid` to `paid`,
// `no_payment_required`, or `failed` (recorded for
// `checkout.session.async_payment_failed`). A paid session never moves to
// `failed`.
export function checkoutPaymentStatusRank(paymentStatus: string): number {
  if (paymentStatus === 'unpaid') return 0;
  if (paymentStatus === 'failed') return 1;
  return 2;
}
