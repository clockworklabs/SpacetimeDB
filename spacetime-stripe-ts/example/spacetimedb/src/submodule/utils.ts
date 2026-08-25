import { SenderError } from 'spacetimedb/server';

export function throwSenderError(message: string): never {
  throw new SenderError(message);
}

export function safeJsonParse(input: string): unknown {
  try {
    return JSON.parse(input);
  } catch {
    return undefined;
  }
}

export function stringArrayFromJson(value: string | undefined): string[] {
  if (!value) return [];
  const parsed = safeJsonParse(value);
  if (!Array.isArray(parsed)) return [];
  const out: string[] = [];
  for (const item of parsed) {
    if (typeof item === 'string') out.push(item);
  }
  return out;
}
