export type StripeEventMetadata = {
  eventId: string;
  eventType: string;
  livemode: boolean;
};

export function parseStripeEventMetadata(
  payloadJson: string
): StripeEventMetadata | undefined {
  let parsed: unknown;
  try {
    parsed = JSON.parse(payloadJson);
  } catch {
    return undefined;
  }
  if (typeof parsed !== 'object' || parsed === null) return undefined;
  const record = parsed as Record<string, unknown>;
  if (typeof record.id !== 'string' || typeof record.type !== 'string')
    return undefined;
  return {
    eventId: record.id,
    eventType: record.type,
    livemode: typeof record.livemode === 'boolean' ? record.livemode : false,
  };
}
