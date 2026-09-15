import type { ElementsObj, Infer as InferBuilder } from 'spacetimedb/server';

type TypeBuilderLike = ElementsObj[string];
type IsUnit<T> = [keyof T] extends [never] ? true : false;

export type RetryResult = { ok: true } | { ok: false; error: string };

export const retryOk = (): RetryResult => ({ ok: true });
export const retryFailed = (error: string): RetryResult => ({
  ok: false,
  error,
});

type RunFn<TB extends TypeBuilderLike> =
  IsUnit<InferBuilder<TB>> extends true
    ? (ctx: unknown) => RetryResult
    : (ctx: unknown, args: InferBuilder<TB>) => RetryResult;

// A symbol key keeps this metadata outside t.enum's Object.keys traversal.
const RUN_KEY = Symbol.for('retry-ts/run');

export type RetryHandler<TB extends TypeBuilderLike = TypeBuilderLike> = TB & {
  [RUN_KEY]: RunFn<TB>;
};

export function retryHandler<TB extends TypeBuilderLike>(
  args: TB,
  run: RunFn<TB>
): RetryHandler<TB> {
  Object.defineProperty(args, RUN_KEY, {
    value: run,
    enumerable: false,
    configurable: false,
    writable: false,
  });
  return args as RetryHandler<TB>;
}

export function makeRetryDispatch<Tx, H extends Record<string, RetryHandler>>(
  handlers: H
) {
  return function dispatch(
    ctx: Tx,
    args: { tag: keyof H & string; value?: unknown }
  ): RetryResult {
    const handler = (handlers as Record<string, RetryHandler>)[args.tag];
    if (!handler) throw new Error(`unknown retry handler: ${args.tag}`);
    const run = handler[RUN_KEY];
    if ('value' in args) {
      return (run as (c: Tx, v: unknown) => RetryResult)(ctx, args.value);
    }
    return (run as (c: Tx) => RetryResult)(ctx);
  };
}
