const isProgressionIdentifier = (value: string): boolean =>
  /^[a-z0-9]+(?:[._-][a-z0-9]+)*$/.test(value);

export const isDependencyObject = (value: unknown): value is Record<string, unknown> =>
  value !== null && typeof value === 'object' && !Array.isArray(value);

export function dependencyFailure(at: string, message: string): never {
  throw new Error(`invalid dependency mode at ${at}: ${message}`);
}

export function assertDependencyObject(
  value: unknown,
  at: string,
  fields: ReadonlySet<string>,
): asserts value is Record<string, unknown> {
  if (!isDependencyObject(value)) return dependencyFailure(at, 'must be an object');
  for (const key of Object.keys(value)) {
    if (!fields.has(key)) dependencyFailure(`${at}.${key}`, 'unknown field');
  }
}

export function dependencyNonEmptyString(value: unknown, at: string): string {
  if (typeof value !== 'string' || value.trim().length === 0) {
    return dependencyFailure(at, 'must be a non-empty string');
  }
  return value;
}

export function dependencyIdentifier(value: unknown, at: string): string {
  const result = dependencyNonEmptyString(value, at);
  if (!isProgressionIdentifier(result)) {
    return dependencyFailure(at,
      'must contain lowercase letters, numbers, dots, dashes, or underscores');
  }
  return result;
}

export function dependencyPositiveInteger(value: unknown, at: string): number {
  if (!Number.isSafeInteger(value) || Number(value) < 1) {
    return dependencyFailure(at, 'must be a positive integer within the safe range');
  }
  return Number(value);
}
