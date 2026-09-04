import { SenderError } from 'spacetimedb/server';

export function fail(message: string): never {
  throw new SenderError(`context_cafe.${message}`);
}

export function requireId(value: unknown, field: string): string {
  if (typeof value !== 'string' || value.trim() === '') {
    fail(`invalid_${field}`);
  }
  return value.trim();
}

export function clampU32(
  value: unknown,
  field: string,
  min: number,
  max: number
): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) {
    fail(`invalid_${field}`);
  }
  const result = Math.round(value);
  if (result < min || result > max) fail(`invalid_${field}`);
  return result;
}
