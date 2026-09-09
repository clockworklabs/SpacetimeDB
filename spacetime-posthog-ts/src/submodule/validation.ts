import { SenderError } from 'spacetimedb/server';

export function throwSenderError(message: string): never {
  throw new SenderError(message);
}

export function normalizeHost(host: string): string {
  const trimmed = host.trim();
  if (!trimmed) throwSenderError('posthog.invalid_host');
  return trimmed.replace(/\/+$/, '');
}

export function parseJsonObject(
  json: string | undefined,
  name: string
): unknown {
  if (json === undefined) return undefined;
  let parsed: unknown;
  try {
    parsed = JSON.parse(json);
  } catch {
    throwSenderError(`posthog.invalid_${name}_json`);
  }
  if (parsed === null || typeof parsed !== 'object' || Array.isArray(parsed)) {
    throwSenderError(`posthog.invalid_${name}_json`);
  }
  return parsed;
}
