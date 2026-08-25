import * as v from 'valibot';
import { SenderError } from 'spacetimedb/server';

export type ParseResult<T> =
  | { kind: 'success'; data: T }
  | { kind: 'error'; issues: v.BaseIssue<unknown>[] };

export function attemptToParse<TSchema extends v.GenericSchema>(
  schema: TSchema,
  input: unknown
): ParseResult<v.InferOutput<TSchema>> {
  const result = v.safeParse(schema, input);
  if (result.success) return { kind: 'success', data: result.output };
  return { kind: 'error', issues: result.issues };
}

export function assertExhaustive(value: never): never {
  throw new Error(`Unhandled discriminant: ${value as string}`);
}

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

export function summarizeIssues(issues: v.BaseIssue<unknown>[]): string {
  if (issues.length === 0) return 'no issues';
  const head = issues[0]!;
  const path = (head.path ?? [])
    .map(p =>
      typeof p.key === 'string' || typeof p.key === 'number'
        ? String(p.key)
        : '?'
    )
    .join('.');
  const where = path ? ` at ${path}` : '';
  return `${head.message}${where}`;
}
