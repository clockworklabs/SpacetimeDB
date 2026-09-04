import { SenderError, SyncResponse, type Request } from 'spacetimedb/server';

export function jsonResponse(body: unknown, status = 200): SyncResponse {
  return new SyncResponse(
    JSON.stringify(body, (_key, value) =>
      typeof value === 'bigint' ? value.toString() : value
    ),
    {
      status,
      headers: { 'content-type': 'application/json' },
    }
  );
}

export function errorResponse(
  error: string,
  status: number,
  extra: Record<string, unknown> = {}
): SyncResponse {
  return jsonResponse({ ok: false, error, ...extra }, status);
}

export function readBearer(req: Request): string | undefined {
  const header = req.headers.get('authorization') ?? '';
  if (!header.toLowerCase().startsWith('bearer ')) return undefined;
  const token = header.slice(7).trim();
  return token.length > 0 ? token : undefined;
}

export function safeJson(req: Request): unknown {
  try {
    return req.json();
  } catch {
    throw new SenderError('world.invalid_json');
  }
}

export function asObject(value: unknown): Record<string, unknown> {
  if (value === null || Array.isArray(value) || typeof value !== 'object') {
    throw new SenderError('world.invalid_json');
  }
  return value as Record<string, unknown>;
}

export function asI32(value: unknown, field: string): number {
  if (!Number.isInteger(value)) throw new SenderError(`world.invalid_${field}`);
  return value as number;
}

export function asOptionalString(value: unknown): string | undefined {
  return typeof value === 'string' ? value : undefined;
}

export function asString(value: unknown, field: string): string {
  if (typeof value !== 'string') {
    throw new SenderError(`world.invalid_${field}`);
  }
  const out = value.trim();
  if (!out) throw new SenderError(`world.invalid_${field}`);
  return out;
}
