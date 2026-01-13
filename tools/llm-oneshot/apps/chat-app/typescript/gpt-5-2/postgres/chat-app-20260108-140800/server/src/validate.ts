export class ClientError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export function assertString(name: string, value: unknown): string {
  if (typeof value !== 'string') throw new ClientError(400, `${name} must be a string`);
  return value;
}

export function nonEmptyTrimmed(name: string, value: unknown, maxLen: number): string {
  const s = assertString(name, value).trim();
  if (!s) throw new ClientError(400, `${name} required`);
  if (s.length > maxLen) throw new ClientError(400, `${name} too long`);
  return s;
}

export function assertInt(name: string, value: unknown): number {
  const n = typeof value === 'string' ? Number(value) : typeof value === 'number' ? value : NaN;
  if (!Number.isInteger(n)) throw new ClientError(400, `${name} must be an integer`);
  return n;
}

export function assertOneOf<T extends string>(
  name: string,
  value: unknown,
  allowed: readonly T[],
): T {
  const s = assertString(name, value);
  if (!allowed.includes(s as T)) throw new ClientError(400, `${name} invalid`);
  return s as T;
}

