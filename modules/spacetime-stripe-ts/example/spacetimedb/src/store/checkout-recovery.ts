type StripeObject = Record<string, unknown>;
type Request = (method: 'GET' | 'POST', path: string) => StripeObject;

function object(value: unknown): value is StripeObject {
  return value !== null && typeof value === 'object' && !Array.isArray(value);
}

export function retireCheckoutAttempt(
  request: Request,
  args: {
    customers: string[];
    store: string;
    attemptId: string;
    sessionId?: string;
  }
): void {
  function matches(session: StripeObject): boolean {
    return (
      object(session.metadata) &&
      session.metadata.store === args.store &&
      session.metadata.storeAttempt === args.attemptId
    );
  }
  function retire(session: StripeObject): void {
    if (session.status === 'complete')
      throw new Error(
        'This checkout is complete. Check Billing for the payment result.'
      );
    if (typeof session.id !== 'string')
      throw new Error('store.checkout_lookup_failed');
    if (session.status === 'open') {
      const expired = request(
        'POST',
        `/v1/checkout/sessions/${encodeURIComponent(session.id)}/expire`
      );
      if (expired.status !== 'expired')
        throw new Error('store.checkout_cancel_failed');
    } else if (session.status !== 'expired') {
      throw new Error('store.checkout_lookup_failed');
    }
  }
  if (args.sessionId) {
    const session = request(
      'GET',
      `/v1/checkout/sessions/${encodeURIComponent(args.sessionId)}`
    );
    if (
      typeof session.customer !== 'string' ||
      !args.customers.includes(session.customer)
    )
      throw new Error('store.checkout_owner_mismatch');
    if (!matches(session)) throw new Error('store.checkout_attempt_mismatch');
    retire(session);
    return;
  }
  // A lost create response leaves no session ID. Search the buyer's sessions
  // through the final page so an older completed checkout cannot be missed.
  for (const customer of args.customers) {
    let cursor = '';
    for (;;) {
      const list = request(
        'GET',
        `/v1/checkout/sessions?customer=${encodeURIComponent(customer)}&limit=100${cursor ? `&starting_after=${encodeURIComponent(cursor)}` : ''}`
      );
      if (!Array.isArray(list.data) || typeof list.has_more !== 'boolean')
        throw new Error('store.checkout_lookup_failed');
      for (const session of list.data) {
        if (!object(session) || typeof session.id !== 'string')
          throw new Error('store.checkout_lookup_failed');
        cursor = session.id;
        if (matches(session)) retire(session);
      }
      if (!list.has_more) break;
      if (!list.data.length) throw new Error('store.checkout_lookup_failed');
    }
  }
}
