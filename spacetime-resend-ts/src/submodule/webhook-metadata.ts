export function parseResendEventType(payloadJson: string): string | undefined {
  let parsed: unknown;
  try {
    parsed = JSON.parse(payloadJson);
  } catch {
    return undefined;
  }
  if (typeof parsed !== 'object' || parsed === null) return undefined;
  const eventType = (parsed as Record<string, unknown>).type;
  return typeof eventType === 'string' && eventType.length > 0
    ? eventType
    : undefined;
}
