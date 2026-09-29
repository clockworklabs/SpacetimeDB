import type { Infer, VariantsObj } from 'spacetimedb/server';

type ArgsBuilder = VariantsObj[string];

type IsUnit<T> = [keyof T] extends [never] ? true : false;

export type RetryResult = { ok: true } | { ok: false; error: string };

export const retryOk = (): RetryResult => ({ ok: true });
export const retryFailed = (error: string): RetryResult => ({
  ok: false,
  error,
});

type RunFn<TB extends ArgsBuilder> =
  IsUnit<Infer<TB>> extends true
    ? (ctx: unknown) => RetryResult
    : (ctx: unknown, args: Infer<TB>) => RetryResult;

export type RetryHandler<TB extends ArgsBuilder = ArgsBuilder> = {
  readonly args: TB;
  readonly run: RunFn<TB>;
};

/** Pair a task argument type with the function that runs one attempt. */
export function retryHandler<TB extends ArgsBuilder>(
  args: TB,
  run: RunFn<TB>
): RetryHandler<TB> {
  return { args, run };
}
